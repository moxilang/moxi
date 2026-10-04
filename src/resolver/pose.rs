//! Poses — living models, step 2 (DOC-20261004-living-models-design §3.1).
//!
//! A pose names qualifier overrides on a thing's OWN mates. It is resolved
//! here, after loops are unrolled and instances flattened, into a list of
//! `(part, qualifiers)` with every expression folded to a number. Applying
//! it (`apply_pose`) writes those numbers into the mate lines — nothing
//! else. That is the whole semantics: the solved frames of `pose P` are the
//! solved frames of the script with P's values pasted in, and the frame
//! solver never learns that poses exist.
//!
//! What a pose may name, and what it may not (each a `PoseError` that
//! names the pose and lists what it could have named):
//! - a part or instance placed by one of this thing's own mates — yes;
//! - `Inst pose=Name` — the instance takes its thing's pose `Name`
//!   (prefixed into this thing's names);
//! - the root (no mate) — nothing to move;
//! - a `symmetric_across` image — pose its source; the image follows;
//! - a part placed inside an instance — use that instance's poses.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::value::{self, Value};

use super::{fold_int, resolve_markers, scope_refs, subst_expr, substitute_idents, ParamEnv, MAX_ITERATIONS};

/// A resolved pose: overrides per part, in first-mention order.
#[derive(Debug, Clone)]
pub struct ResolvedPose {
    pub name:      String,
    pub overrides: Vec<PoseOverride>,
    pub span:      Span,
}

#[derive(Debug, Clone)]
pub struct PoseOverride {
    /// Flattened subject part of the mate this overrides.
    pub part:  String,
    pub quals: PoseQuals,
    pub span:  Span,
}

impl ResolvedPose {
    /// The same pose under another parameter environment (an instance
    /// that overrides its thing's parameters).
    pub(super) fn subst(&self, env: &ParamEnv) -> ResolvedPose {
        ResolvedPose {
            name: self.name.clone(),
            overrides: self.overrides.iter().map(|o| PoseOverride {
                part:  o.part.clone(),
                quals: map_quals(&o.quals, &mut |e| subst_expr(e, env)),
                span:  o.span,
            }).collect(),
            span: self.span,
        }
    }

    pub(super) fn prefixed(&self, prefix: &str) -> ResolvedPose {
        ResolvedPose {
            name: self.name.clone(),
            overrides: self.overrides.iter().map(|o| PoseOverride {
                part: format!("{prefix}.{}", o.part), ..o.clone()
            }).collect(),
            span: self.span,
        }
    }
}

/// Write a pose's values into the mate lines. Every other placement, and
/// every qualifier the pose does not set, is untouched.
pub fn apply_pose(relations: &[Placement], pose: &ResolvedPose) -> Vec<Placement> {
    let by_part: HashMap<&str, &PoseQuals> =
        pose.overrides.iter().map(|o| (o.part.as_str(), &o.quals)).collect();
    relations.iter().map(|p| match p {
        Placement::Align { subject, object, twist, pitch, gap, offsets, span } => {
            let Some(q) = by_part.get(subject.part.as_str()) else { return p.clone() };
            Placement::Align {
                subject: subject.clone(),
                object:  object.clone(),
                twist:   q.twist.clone().unwrap_or_else(|| twist.clone()),
                pitch:   q.pitch.clone().unwrap_or_else(|| pitch.clone()),
                gap:     q.gap.clone().unwrap_or_else(|| gap.clone()),
                offsets: Box::new(MateOffsets {
                    shift: q.shift.clone().unwrap_or_else(|| offsets.shift.clone()),
                    lean:  q.lean.clone().unwrap_or_else(|| offsets.lean.clone()),
                }),
                span: *span,
            }
        }
        Placement::Mirror { .. } => p.clone(),
    }).collect()
}

pub(super) fn map_quals(q: &PoseQuals, f: &mut dyn FnMut(&Expr) -> Expr) -> PoseQuals {
    PoseQuals {
        twist: q.twist.as_ref().map(&mut *f),
        pitch: q.pitch.as_ref().map(&mut *f),
        gap:   q.gap.as_ref().map(&mut *f),
        shift: q.shift.as_ref().map(|(a, b)| (f(a), f(b))),
        lean:  q.lean.as_ref().map(|(a, b)| (f(a), f(b))),
    }
}

/// Every expression in a pose declaration, rewritten in place (used to
/// expand `fn` calls up front, like every other expression in a thing).
pub(super) fn walk_pose_exprs(p: &mut PoseDecl, f: &mut dyn FnMut(&Expr) -> Expr) {
    fn lines(ls: &mut [PoseLine], f: &mut dyn FnMut(&Expr) -> Expr) {
        for l in ls {
            if let PoseSet::Quals(q) = &l.set {
                l.set = PoseSet::Quals(Box::new(map_quals(q, f)));
            }
        }
    }
    fn loops(ls: &mut [PoseFor], f: &mut dyn FnMut(&Expr) -> Expr) {
        for b in ls {
            b.start = f(&b.start);
            b.end   = f(&b.end);
            lines(&mut b.lines, f);
            loops(&mut b.loops, f);
        }
    }
    lines(&mut p.lines, f);
    loops(&mut p.loops, f);
}

/// One pose line after unrolling: a concrete name, its span, what it sets
/// (loop variables substituted, parameters still symbolic).
pub(super) struct Line {
    pub name: String,
    pub span: Span,
    pub set:  PoseSet,
}

pub(super) fn unroll_pose(
    decl:        &PoseDecl,
    env:         &ParamEnv,
    index_exprs: &[Expr],
    errs:        &mut Vec<MoxiError>,
) -> Vec<Line> {
    let mut out = Vec::new();
    let mut budget = MAX_ITERATIONS;
    let scope: HashMap<String, Expr> = HashMap::new();
    emit(&decl.lines, &scope, env, index_exprs, &mut out, errs);
    for b in &decl.loops {
        unroll_for(b, &decl.name.name, &scope, env, index_exprs, &mut budget, &mut out, errs);
    }
    out
}

fn emit(
    lines: &[PoseLine], scope: &HashMap<String, Expr>, env: &ParamEnv,
    index_exprs: &[Expr], out: &mut Vec<Line>, errs: &mut Vec<MoxiError>,
) {
    for l in lines {
        let name = resolve_markers(&l.part.name, scope, env, index_exprs, errs, l.part.span);
        let set = match &l.set {
            PoseSet::Quals(q) => {
                let refs = scope_refs(scope);
                PoseSet::Quals(Box::new(map_quals(q, &mut |e| substitute_idents(e, &refs))))
            }
            PoseSet::Pose(p) => PoseSet::Pose(p.clone()),
        };
        out.push(Line { name, span: l.part.span, set });
    }
}

#[allow(clippy::too_many_arguments)]
fn unroll_for(
    b: &PoseFor, pose: &str, outer: &HashMap<String, Expr>, env: &ParamEnv,
    index_exprs: &[Expr], budget: &mut usize, out: &mut Vec<Line>, errs: &mut Vec<MoxiError>,
) {
    let var = &b.var.name;
    if outer.contains_key(var) {
        errs.push(MoxiError::PoseError {
            pose: pose.to_string(),
            message: format!("loop variable '{var}' is already bound by an enclosing loop"),
            span: b.var.span,
        });
        return;
    }
    if env.contains_key(var) {
        errs.push(MoxiError::PoseError {
            pose: pose.to_string(),
            message: format!("loop variable '{var}' shadows a parameter or `let` — pick another name"),
            span: b.var.span,
        });
        return;
    }
    let bound = |e: &Expr, which: &str, errs: &mut Vec<MoxiError>| match fold_int(e, outer, env) {
        Ok(v) => Some(v),
        Err(m) => {
            errs.push(MoxiError::PoseError {
                pose: pose.to_string(),
                message: format!("the {which} of `for {var} in …`: {m}"),
                span: b.span,
            });
            None
        }
    };
    let (Some(start), Some(end)) = (bound(&b.start, "start", errs), bound(&b.end, "end", errs)) else { return };
    for k in start..end {
        if *budget == 0 {
            errs.push(MoxiError::PoseError {
                pose: pose.to_string(),
                message: format!("loops in one pose may unroll to at most {MAX_ITERATIONS} iterations"),
                span: b.span,
            });
            return;
        }
        *budget -= 1;
        let mut scope = outer.clone();
        scope.insert(var.clone(), Expr::Int(k));
        emit(&b.lines, &scope, env, index_exprs, out, errs);
        for inner in &b.loops {
            unroll_for(inner, pose, &scope, env, index_exprs, budget, out, errs);
        }
    }
}

/// What a pose can see of its thing.
pub(super) struct PoseScope<'a> {
    pub thing:             &'a str,
    /// This thing's own `Align` mates: (subject as written — a part or an
    /// instance — , flattened subject part), in declaration order.
    pub own_align:         &'a [(String, String)],
    /// This thing's own `symmetric_across`: image as written → source.
    pub own_mirror:        &'a HashMap<String, String>,
    pub part_names:        &'a HashMap<String, Span>,
    /// instance → thing it instances
    pub instance_of:       &'a HashMap<String, String>,
    /// instance → its thing's poses, prefixed with the instance name and
    /// substituted under the instance's arguments
    pub instance_poses:    &'a HashMap<String, Vec<ResolvedPose>>,
}

impl PoseScope<'_> {
    fn movable(&self) -> String {
        if self.own_align.is_empty() {
            format!("'{}' has no mates to pose", self.thing)
        } else {
            format!("a pose can move: {}",
                self.own_align.iter().map(|(w, _)| w.as_str()).collect::<Vec<_>>().join(", "))
        }
    }

    fn pose_names(&self, inst: &str) -> String {
        match self.instance_poses.get(inst) {
            Some(ps) if !ps.is_empty() => ps.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "),
            _ => "it has none".to_string(),
        }
    }
}

/// Resolve unrolled lines into overrides on flattened subject parts. The
/// result still holds parameter names; the caller folds it.
pub(super) fn resolve_lines(
    pose: &str, span: Span, lines: Vec<Line>, sc: &PoseScope, errs: &mut Vec<MoxiError>,
) -> ResolvedPose {
    let perr = |message: String, span: Span| MoxiError::PoseError { pose: pose.to_string(), message, span };
    let align_of: HashMap<&str, &str> =
        sc.own_align.iter().map(|(w, s)| (w.as_str(), s.as_str())).collect();

    let mut order: Vec<String> = Vec::new();
    let mut merged: HashMap<String, (PoseQuals, Span)> = HashMap::new();
    let mut add = |part: String, q: PoseQuals, span: Span, errs: &mut Vec<MoxiError>| {
        let entry = merged.entry(part.clone()).or_insert_with(|| {
            order.push(part.clone());
            (PoseQuals::default(), span)
        });
        let e = &mut entry.0;
        let mut twice = Vec::new();
        macro_rules! take { ($f:ident) => {
            if let Some(v) = q.$f { if e.$f.is_some() { twice.push(stringify!($f)); } else { e.$f = Some(v); } }
        } }
        take!(twist); take!(pitch); take!(gap); take!(shift); take!(lean);
        for f in twice {
            errs.push(perr(format!("sets '{part}' {f} twice"), span));
        }
    };

    for Line { name, span, set } in lines {
        let is_instance = sc.instance_of.contains_key(&name);
        match set {
            PoseSet::Pose(inner) => {
                if !is_instance {
                    let msg = if sc.part_names.contains_key(&name) {
                        format!("'{name}' is a part, not an instance; `pose=` takes one of an \
                                 instance's poses — set its qualifiers instead, e.g. `{name} lean=(30, 0)`")
                    } else {
                        format!("'{name}' is not an instance of '{}'", sc.thing)
                    };
                    errs.push(perr(msg, span));
                    continue;
                }
                if let Some(src) = sc.own_mirror.get(&name) {
                    errs.push(perr(format!(
                        "'{name}' is the mirror image of '{src}'; pose '{src}' (`{src} pose={}`) — \
                         the image follows", inner.name), span));
                    continue;
                }
                let found = sc.instance_poses.get(&name)
                    .and_then(|ps| ps.iter().find(|p| p.name == inner.name));
                match found {
                    Some(p) => for o in &p.overrides { add(o.part.clone(), o.quals.clone(), span, errs); },
                    None => errs.push(perr(format!(
                        "'{name}' (a '{}') has no pose '{}'; its poses: {}",
                        sc.instance_of[&name], inner.name, sc.pose_names(&name)), inner.span)),
                }
            }
            PoseSet::Quals(q) => {
                if let Some(subj) = align_of.get(name.as_str()) {
                    add(subj.to_string(), *q, span, errs);
                    continue;
                }
                let msg = if let Some(src) = sc.own_mirror.get(&name) {
                    format!("'{name}' is the mirror image of '{src}'; pose '{src}', or give '{name}' its own mate")
                } else if is_instance || sc.part_names.contains_key(&name) {
                    match name.split_once('.').filter(|(i, _)| sc.instance_of.contains_key(*i)) {
                        Some((inst, _)) if !is_instance => format!(
                            "'{name}' is placed inside the instance '{inst}' (a '{}'); a thing poses only \
                             its own mates — use one of its poses, `{inst} pose=…` ({})",
                            sc.instance_of[inst], sc.pose_names(inst)),
                        _ => format!(
                            "'{name}' is not placed by a mate (it is the root of '{}'), so there is \
                             nothing to move; {}", sc.thing, sc.movable()),
                    }
                } else {
                    format!("'{name}' is not a part of '{}'; {}", sc.thing, sc.movable())
                };
                errs.push(perr(msg, span));
            }
        }
    }

    ResolvedPose {
        name: pose.to_string(),
        overrides: order.into_iter().map(|part| {
            let (quals, span) = merged.remove(&part).expect("every ordered part was merged");
            PoseOverride { part, quals, span }
        }).collect(),
        span,
    }
}

/// Fold every value under `env`; a value that is not a number is an error
/// naming the pose, the part and the qualifier.
pub(super) fn fold_pose(raw: &ResolvedPose, env: &ParamEnv, errs: &mut Vec<MoxiError>) -> ResolvedPose {
    let mut out = raw.subst(env);
    for o in &mut out.overrides {
        let mut check = |what: &str, e: &Expr| -> Expr {
            match value::eval(e, env) {
                Ok(Value::Num(v)) => Expr::Float(v),
                Ok(other) => {
                    errs.push(MoxiError::PoseError {
                        pose: raw.name.clone(),
                        message: format!("'{}' {what} must be a number, got a {}", o.part, other.kind()),
                        span: o.span,
                    });
                    Expr::Float(0.0)
                }
                Err(err) => {
                    errs.push(MoxiError::PoseError {
                        pose: raw.name.clone(),
                        message: format!("'{}' {what}: {}", o.part, err.message),
                        span: o.span,
                    });
                    Expr::Float(0.0)
                }
            }
        };
        let q = &o.quals;
        o.quals = PoseQuals {
            twist: q.twist.as_ref().map(|e| check("twist", e)),
            pitch: q.pitch.as_ref().map(|e| check("pitch", e)),
            gap:   q.gap.as_ref().map(|e| check("gap", e)),
            shift: q.shift.as_ref().map(|(a, b)| (check("shift", a), check("shift", b))),
            lean:  q.lean.as_ref().map(|(a, b)| (check("lean", a), check("lean", b))),
        };
    }
    out
}

/// Names a pose declaration may not take, and duplicates.
pub(super) fn check_pose_names(poses: &[PoseDecl], errs: &mut Vec<MoxiError>) {
    let mut seen = HashSet::new();
    for p in poses {
        if p.name.name == "rest" {
            errs.push(MoxiError::PoseError {
                pose: "rest".into(),
                message: "'rest' is the script's own pose (every mate as written) and cannot be redefined"
                    .into(),
                span: p.name.span,
            });
        } else if !seen.insert(p.name.name.clone()) {
            errs.push(MoxiError::DuplicateName { name: p.name.name.clone(), span: p.name.span });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::pipeline::{compile_to_scene, compile_to_scene_posed};

    fn frames(src: &str, pose: Option<&str>) -> Vec<(String, [f64; 3], [[f64; 3]; 3])> {
        let scene = compile_to_scene_posed(src, pose)
            .unwrap_or_else(|e| panic!("compile failed: {e:?}"));
        scene.layers.iter().flat_map(|l| l.parts.iter())
            .map(|p| (p.name.clone(), p.frame.pos, p.frame.rot)).collect()
    }

    /// Test 1 of the design: `pose P` solves to exactly the frames of the
    /// script with P's values written into the mate lines.
    fn assert_substitution(posed_src: &str, pose: &str, pasted_src: &str) {
        let a = frames(posed_src, Some(pose));
        let b = frames(pasted_src, None);
        assert_eq!(a.len(), b.len());
        for ((na, pa, ra), (nb, pb, rb)) in a.iter().zip(&b) {
            assert_eq!(na, nb);
            for k in 0..3 {
                assert!((pa[k] - pb[k]).abs() < 1e-9, "{na} pos {pa:?} vs {pb:?}");
                for c in 0..3 { assert!((ra[k][c] - rb[k][c]).abs() < 1e-9, "{na} rot"); }
            }
        }
        // and the pose really moved something (the test is not vacuous)
        let rest = frames(posed_src, None);
        assert!(rest.iter().zip(&a).any(|((_, p, _), (_, q, _))|
            (0..3).any(|k| (p[k] - q[k]).abs() > 1e-6)), "pose {pose} moved nothing");
    }

    fn errors(src: &str) -> String {
        match compile_to_scene(src) {
            Ok(_) => panic!("expected an error"),
            Err(es) => es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"),
        }
    }

    const LAMP: &str = r##"
material Iron { color = "#333333" }
thing Lamp(height=300, reach=90) {
    part Base { shape = cylinder(height=12, radius=45), material = Iron }
    part Post { shape = cylinder(height=height, radius=6), material = Iron }
    part Arm  { shape = capsule(height=reach, radius=4), material = Iron }
    part Bulb { shape = sphere(radius=18), material = Iron }
    relation {
        Post.bottom on Base.top
        Arm.bottom  on Post.side(t=0.92, angle=0) lean=(20, 0)
        Bulb.center on Arm.top
    }
    POSES
    resolve voxel_size = 3
}
print Lamp
"##;

    fn lamp(poses: &str) -> String { LAMP.replace("POSES", poses) }

    #[test]
    fn a_pose_is_its_values_pasted_into_the_mates() {
        let src = lamp("pose Up { Arm lean=(reach - 20, 5) twist=15 gap=2 }  pose Low { Arm lean=(-30, 0) }");
        let up = lamp("").replace("lean=(20, 0)", "lean=(70, 5) twist=15 gap=2");
        let low = lamp("").replace("lean=(20, 0)", "lean=(-30, 0)");
        assert_substitution(&src, "Up", &up);
        assert_substitution(&src, "Low", &low);
        // `rest` is the script as written
        assert_eq!(frames(&src, Some("rest")), frames(&src, None));
    }

    #[test]
    fn poses_unroll_in_loops() {
        let tail = |relation_lean: &str, poses: &str| format!(r#"
material M {{ color = red }}
thing Tail(segs=5) {{
    part Seg[0] {{ shape = capsule(height=10, radius=2), material = M }}
    for k in 1..segs {{
        part Seg[k] {{ shape = capsule(height=10, radius=2), material = M }}
        relation {{ Seg[k].bottom on Seg[k - 1].top {relation_lean} }}
    }}
    {poses}
    resolve voxel_size = 1
}}
print Tail
"#);
        let posed = tail("lean=(5, 0)", "pose Curl { for k in 1..segs { Seg[k] lean=(10 * k, 0) } }");
        let pasted = tail("lean=(10 * k, 0)", "");
        assert_substitution(&posed, "Curl", &pasted);
    }

    const BIRD: &str = r#"
material F { color = white }
thing Wing(len=30) {
    part Root  { shape = sphere(radius=2), material = F }
    part Blade { shape = ellipsoid(rx=2, ry=len, rz=6), material = F }
    relation { Blade.bottom on Root.top LEAN }
    anchor root = Root.center
    WINGPOSE
    resolve voxel_size = 1
}
thing Bird {
    part Body  { shape = sphere(radius=10), material = F }
    part WingR { thing = Wing(len=40) }
    part WingL { thing = Wing(len=40) }
    relation {
        WingR.root on Body.east
        WingL symmetric_across Body from=WingR
    }
    BIRDPOSE
    resolve voxel_size = 1
}
print Bird
"#;

    fn bird(lean: &str, wing_pose: &str, bird_pose: &str) -> String {
        BIRD.replace("LEAN", lean).replace("WINGPOSE", wing_pose).replace("BIRDPOSE", bird_pose)
    }

    /// An instance takes its thing's pose by name, under its own arguments,
    /// and the mirror image follows its source.
    #[test]
    fn instances_take_their_things_poses_and_mirrors_follow() {
        let posed = bird("lean=(0, 0)", "pose Up { Blade lean=(len, 0) }", "pose Flap { WingR pose=Up }");
        let pasted = bird("lean=(len, 0)", "", "");
        assert_substitution(&posed, "Flap", &pasted);
        let a = frames(&posed, Some("Flap"));
        let (r, l) = (a.iter().find(|p| p.0 == "WingR.Blade").unwrap(), a.iter().find(|p| p.0 == "WingL.Blade").unwrap());
        assert!((r.1[0] + l.1[0]).abs() < 1e-9 && (r.1[1] - l.1[1]).abs() < 1e-9, "mirror follows");
    }

    #[test]
    fn a_pose_cannot_move_the_root() {
        let m = errors(&lamp("pose P { Base gap=3 }"));
        assert!(m.contains("pose 'P'") && m.contains("'Base' is not placed by a mate")
            && m.contains("a pose can move: Post, Arm, Bulb"), "{m}");
    }

    #[test]
    fn a_pose_names_known_parts_only() {
        let m = errors(&lamp("pose P { Armm lean=(1, 0) }"));
        assert!(m.contains("'Armm' is not a part of 'Lamp'") && m.contains("Post, Arm, Bulb"), "{m}");
    }

    #[test]
    fn a_mirror_image_is_posed_through_its_source() {
        let m = errors(&bird("lean=(0, 0)", "pose Up { Blade lean=(10, 0) }", "pose Flap { WingL pose=Up }"));
        assert!(m.contains("'WingL' is the mirror image of 'WingR'; pose 'WingR'"), "{m}");
        let m = errors(&bird("lean=(0, 0)", "", "pose P { WingL gap=1 }"));
        assert!(m.contains("'WingL' is the mirror image of 'WingR'; pose 'WingR', or give 'WingL' its own mate"), "{m}");
    }

    #[test]
    fn a_thing_poses_only_its_own_mates() {
        let m = errors(&bird("lean=(0, 0)", "pose Up { Blade lean=(10, 0) }", "pose P { WingR.Blade lean=(5, 0) }"));
        assert!(m.contains("'WingR.Blade' is placed inside the instance 'WingR'")
            && m.contains("`WingR pose=…` (Up)"), "{m}");
    }

    #[test]
    fn unknown_inner_pose_lists_the_things_poses() {
        let m = errors(&bird("lean=(0, 0)", "pose Up { Blade lean=(10, 0) }", "pose P { WingR pose=Down }"));
        assert!(m.contains("'WingR' (a 'Wing') has no pose 'Down'; its poses: Up"), "{m}");
        let m = errors(&lamp("pose P { Arm pose=Up }"));
        assert!(m.contains("'Arm' is a part, not an instance"), "{m}");
    }

    #[test]
    fn a_whole_instance_moves_by_its_own_mate() {
        let posed = bird("lean=(0, 0)", "", "pose Out { WingR shift=(0, 3) }");
        let pasted = bird("lean=(0, 0)", "", "").replace("WingR.root on Body.east", "WingR.root on Body.east shift=(0, 3)");
        assert_substitution(&posed, "Out", &pasted);
    }

    #[test]
    fn rest_and_duplicates_are_rejected() {
        assert!(errors(&lamp("pose rest { Arm lean=(1, 0) }")).contains("'rest' is the script's own pose"));
        assert!(errors(&lamp("pose A { Arm lean=(1, 0) }  pose A { Arm lean=(2, 0) }")).contains("'A' is already defined"));
        let m = errors(&bird("lean=(0, 0)", "pose Up { Blade lean=(10, 0) }",
            "pose P { WingR pose=Up  WingR.Blade lean=(1, 0) }"));
        assert!(m.contains("placed inside the instance"), "{m}");
    }

    #[test]
    fn a_qualifier_is_set_once_per_part() {
        let m = errors(&lamp("pose P { Arm lean=(1, 0)  Arm lean=(2, 0) }"));
        assert!(m.contains("sets 'Arm' lean twice"), "{m}");
        let m = errors(&lamp("pose P { Arm lean=(1, 0) lean=(2, 0) }"));
        assert!(m.contains("each qualifier at most once"), "{m}");
    }

    #[test]
    fn a_pose_sets_only_motion_qualifiers() {
        let m = errors(&lamp("pose P { Arm from=Post }"));
        assert!(m.contains("a qualifier a pose can set: twist, pitch, gap, shift, lean"), "{m}");
        let m = errors(&lamp("pose P { Arm }"));
        assert!(m.contains("at least one of twist="), "{m}");
    }

    #[test]
    fn values_must_fold_to_numbers() {
        let m = errors(&lamp("pose P { Arm lean=(reachh, 0) }"));
        assert!(m.contains("pose 'P'") && m.contains("'Arm' lean") && m.contains("reachh"), "{m}");
    }

    /// Constraints hold in every pose, and the error names the pose — even
    /// when the command prints rest.
    #[test]
    fn constraints_are_checked_in_every_pose() {
        let src = lamp("pose Droop { Arm lean=(-80, 0)  Bulb gap=300 }  constraint Bulb above Base");
        let m = errors(&src);
        assert!(m.contains("in pose 'Droop'"), "{m}");
        assert!(compile_to_scene(&lamp("pose Up { Arm lean=(60, 0) }  constraint Bulb above Base")).is_ok());
    }

    #[test]
    fn solver_errors_in_a_pose_name_the_pose() {
        let m = errors(&lamp("pose Tilt { Bulb lean=(30, 0) }"));
        assert!(m.contains("in pose 'Tilt'") && m.contains("needs an oriented anchor"), "{m}");
    }

    #[test]
    fn unknown_printed_pose_lists_the_poses() {
        let src = lamp("pose Up { Arm lean=(60, 0) }");
        let e = compile_to_scene_posed(&src, Some("Down")).unwrap_err();
        assert!(e[0].message.contains("pose 'Down': no printed thing has this pose — poses: Lamp: Up"), "{:?}", e);
    }
}
