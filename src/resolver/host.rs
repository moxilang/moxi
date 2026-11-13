//! Hosted parts and `fuse` groups (Form, stage 4): resolving what each
//! `cuts` / `fuses` part changes, and which parts each `fuse` joins.
//!
//! Nothing here touches a frame. The pipeline assembles the shapes after the
//! solve (`pipeline::assemble`): a target's shape becomes the union of
//! itself and what fuses into it, minus what cuts it, each member placed by
//! its solved frame relative to the target.

use std::collections::{HashMap, HashSet};

use crate::ast::{FuseDecl, Placement};
use crate::error::{MoxiError, Span};
use crate::value::{self, Value};

use super::{ParamEnv, ResolvedPart};

/// A part's `cuts = …` / `fuses = …`, resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedHost {
    pub cut:    bool,
    /// The flattened target part; `None` while it is `host` and the thing
    /// is not (yet) mated onto anything.
    pub target: Option<String>,
    pub blend:  f64,
}

/// `fuse A, B blend=k`, resolved: the member parts in order (the first is
/// the one the others join) and the fillet.
#[derive(Debug, Clone)]
pub struct ResolvedFuse {
    pub members: Vec<String>,
    pub blend:   f64,
    pub span:    Span,
}

impl ResolvedFuse {
    pub(super) fn prefixed(&self, prefix: &str) -> ResolvedFuse {
        ResolvedFuse {
            members: self.members.iter().map(|m| format!("{prefix}.{m}")).collect(),
            blend:   self.blend,
            span:    self.span,
        }
    }
}

fn host_err(part: &str, message: String, span: Span) -> MoxiError {
    MoxiError::ShapeError { message: format!("part '{part}': {message}"), span }
}

/// A `blend` as a number ≥ 0.
pub(super) fn fold_blend(e: &crate::ast::Expr, env: &ParamEnv, what: &str, span: Span,
                         errors: &mut Vec<MoxiError>) -> f64 {
    match value::eval(e, env) {
        Ok(Value::Num(v)) if v >= 0.0 => v,
        Ok(Value::Num(v)) => {
            errors.push(MoxiError::ShapeError { message: format!("{what}: `blend` is a width, 0 or more; got {v}"), span });
            0.0
        }
        Ok(_) | Err(_) => {
            errors.push(MoxiError::ShapeError {
                message: format!("{what}: `blend` must be a number known when the thing is built"), span });
            0.0
        }
    }
}

/// Fill each instance part's `host` with what the instance is mated onto
/// (the object of the mate whose subject is inside the instance and whose
/// object is not), then check every target.
pub(super) fn resolve_hosts(
    parts:       &mut [ResolvedPart],
    relations:   &[Placement],
    instance_of: &HashMap<String, String>,
    mirrored:    &HashSet<String>,
    spans:       &HashMap<String, Span>,
    errors:      &mut Vec<MoxiError>,
) {
    let host_of = |inst: &str| -> Option<String> {
        let pre = format!("{inst}.");
        relations.iter().find_map(|p| match p {
            Placement::Align { subject, object, .. }
                if subject.part.starts_with(&pre) && !object.part.starts_with(&pre) => Some(object.part.clone()),
            _ => None,
        })
    };
    for p in parts.iter_mut() {
        let Some(h) = p.hosted.as_mut() else { continue };
        if h.target.is_some() { continue; }
        let Some((inst, _)) = p.name.split_once('.') else { continue };
        if mirrored.contains(inst) {
            errors.push(host_err(&p.name, format!(
                "'{inst}' is a mirror image, and a mirror image does not cut or fuse into its host — \
                 mirror the host instead, or give the image its own mate"),
                spans.get(inst).copied().unwrap_or(Span::new(0, 0))));
            continue;
        }
        if instance_of.contains_key(inst) {
            h.target = host_of(inst);
        }
    }

    let shaped: HashSet<&str> = parts.iter().filter(|p| p.shape.is_some()).map(|p| p.name.as_str()).collect();
    let hosted: HashSet<&str> = parts.iter().filter(|p| p.hosted.is_some()).map(|p| p.name.as_str()).collect();
    for p in parts.iter() {
        let Some(h) = &p.hosted else { continue };
        let Some(t) = &h.target else { continue };
        // only this thing's own parts: an instance's were checked in its thing
        if p.name.contains('.') && t.contains('.') && t.split('.').next() == p.name.split('.').next() { continue; }
        let span = spans.get(p.name.split('.').next().unwrap_or(&p.name)).copied().unwrap_or(Span::new(0, 0));
        let verb = if h.cut { "cuts" } else { "fuses" };
        if t == &p.name {
            errors.push(host_err(&p.name, format!("it {verb} itself; name the part it changes, or `host`"), span));
        } else if instance_of.contains_key(t) {
            errors.push(host_err(&p.name, format!(
                "'{t}' is an instance; name one of its parts (`{verb} = {t}.Body`)"), span));
        } else if !shaped.contains(t.as_str()) {
            let mut known: Vec<&str> = shaped.iter().copied().filter(|n| !n.contains('.')).collect();
            known.sort_unstable();
            errors.push(host_err(&p.name, format!(
                "it {verb} '{t}', which is not a part of this thing — parts: {}, or `host`", known.join(", ")), span));
        } else if hosted.contains(t.as_str()) {
            errors.push(host_err(&p.name, format!(
                "it {verb} '{t}', which itself cuts or fuses into another part; name that part instead"), span));
        }
    }
}

/// Expand `fuse` names into parts and check the groups (own groups and
/// those inherited from instances together).
pub(super) fn resolve_fuses(
    decls:       &[FuseDecl],
    inherited:   Vec<ResolvedFuse>,
    parts:       &[ResolvedPart],
    instance_of: &HashMap<String, String>,
    env:         &ParamEnv,
    errors:      &mut Vec<MoxiError>,
) -> Vec<ResolvedFuse> {
    let mut out = Vec::new();
    for d in decls {
        let mut members: Vec<String> = Vec::new();
        let mut from_instances: Vec<Vec<&ResolvedPart>> = Vec::new();
        let mut bad = false;
        for n in &d.names {
            let name = n.name.as_str();
            let exact: Vec<&ResolvedPart> = parts.iter().filter(|p| p.name == name && p.shape.is_some()).collect();
            let family: Vec<&ResolvedPart> = parts.iter()
                .filter(|p| p.shape.is_some() && !p.name.contains('.') && p.name.starts_with(&format!("{name}[")))
                .collect();
            let instance: Vec<&ResolvedPart> = if instance_of.contains_key(name) {
                parts.iter().filter(|p| p.shape.is_some() && p.name.starts_with(&format!("{name}."))).collect()
            } else { Vec::new() };
            if !exact.is_empty() {
                members.extend(exact.iter().map(|p| p.name.clone()));
            } else if !family.is_empty() {
                members.extend(family.iter().map(|p| p.name.clone()));
            } else if !instance.is_empty() {
                from_instances.push(instance);
                members.push(format!("\u{1}{}", from_instances.len() - 1)); // placeholder, filled below
            } else {
                let mut known: Vec<&str> = parts.iter().filter(|p| p.shape.is_some() && !p.name.contains('.'))
                    .map(|p| p.name.split('[').next().unwrap_or(&p.name)).collect();
                known.extend(instance_of.keys().map(String::as_str));
                known.sort_unstable();
                known.dedup();
                errors.push(MoxiError::ShapeError { message: format!(
                    "`fuse`: '{name}' is not a part, a family or an instance of this thing — {}", known.join(", ")),
                    span: n.span });
                bad = true;
            }
        }
        if bad { continue; }
        // the group's material: the first named part's, else the first
        // instance part that wears one
        let material_of = |n: &str| parts.iter().find(|p| p.name == n).and_then(|p| p.material_index);
        let group_mat = members.iter().filter(|m| !m.starts_with('\u{1}')).find_map(|m| material_of(m))
            .or_else(|| from_instances.iter().flatten().find_map(|p| p.material_index));
        let mut expanded: Vec<String> = Vec::new();
        for m in members {
            match m.strip_prefix('\u{1}') {
                Some(k) => {
                    let k: usize = k.parse().unwrap_or(0);
                    expanded.extend(from_instances[k].iter()
                        .filter(|p| p.material_index == group_mat && p.hosted.is_none())
                        .map(|p| p.name.clone()));
                }
                None => expanded.push(m),
            }
        }
        // a part already in another's surface is the more telling complaint
        if let Some(m) = expanded.iter().find(|m| parts.iter().any(|p| &p.name == *m && p.hosted.is_some())) {
            errors.push(MoxiError::ShapeError { message: format!(
                "`fuse`: '{m}' already cuts or fuses into a part; take it out of the group, or drop its `cuts`/`fuses`"),
                span: d.span });
            continue;
        }
        let mixed: Vec<&String> = expanded.iter().filter(|m| material_of(m) != group_mat).collect();
        if !mixed.is_empty() {
            errors.push(MoxiError::ShapeError { message: format!(
                "`fuse` makes one surface in one material, but {} wear{} another material than {} — \
                 give them one material, or leave them out of the group",
                mixed.iter().map(|s| format!("'{s}'")).collect::<Vec<_>>().join(", "),
                if mixed.len() == 1 { "s" } else { "" }, expanded[0]), span: d.span });
            continue;
        }
        let blend = fold_blend(&d.blend, env, "`fuse`", d.span, errors);
        out.push(ResolvedFuse { members: expanded, blend, span: d.span });
    }
    out.extend(inherited);

    // every group: two parts or more, each part once, in one group, not hosted
    let hosted: HashSet<&str> = parts.iter().filter(|p| p.hosted.is_some()).map(|p| p.name.as_str()).collect();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut keep = Vec::new();
    for (gi, g) in out.into_iter().enumerate() {
        let mut ok = true;
        let mut here: HashSet<&str> = HashSet::new();
        for m in &g.members {
            if !here.insert(m) {
                errors.push(MoxiError::ShapeError { message: format!("`fuse` names '{m}' twice"), span: g.span });
                ok = false;
            }
            if hosted.contains(m.as_str()) {
                errors.push(MoxiError::ShapeError { message: format!(
                    "`fuse`: '{m}' already cuts or fuses into a part; take it out of the group, or drop its `cuts`/`fuses`"),
                    span: g.span });
                ok = false;
            }
            if let Some(&other) = seen.get(m) {
                if other != gi {
                    errors.push(MoxiError::ShapeError { message: format!(
                        "'{m}' is in two `fuse` groups; a part joins one surface — merge the groups into one `fuse`"),
                        span: g.span });
                    ok = false;
                }
            }
        }
        if g.members.len() < 2 {
            errors.push(MoxiError::ShapeError { message: format!(
                "`fuse` joins two parts or more; it names {} here", g.members.len()), span: g.span });
            ok = false;
        }
        for m in &g.members { seen.insert(m.clone(), gi); }
        if ok { keep.push(g); }
    }
    keep
}
