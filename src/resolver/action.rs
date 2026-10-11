//! Composable actions — Issue N (DOC-20261011-composable-actions).
//!
//! `action Explode { trigger Body hits=3  detach Roof push=up  state Charred on }`
//! names a composition of effects on the thing's own structure: detach a
//! lump, reveal or hide a part, swap a part's look, switch a state, play a
//! clip, run another action, each line optionally delayed. Moxi only
//! declares; a consumer runs the effects (and keeps the physics, sound and
//! gameplay). Actions are the only way to say behaviour: `breakable` left the
//! language (Issue N2); a piece that comes off is `trigger P hits=N  detach P`.
//!
//! A thing's actions come with every instance of it, prefixed (`D.Explode`,
//! `D.Roof`), under the instance's arguments. A mirrored instance brings its
//! source's actions, renamed to its own parts (`mirror_of`), so its pushes
//! are the source's reflected.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::value::{self, Value};

use super::{fold_int, resolve_markers, scope_refs, substitute_idents, ParamEnv, MAX_ITERATIONS};

/// Which way a detached lump is pushed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Push {
    /// Away from what it hangs from, along the mate.
    Out,
    /// A direction word, in the space of the thing that declared it.
    Dir(AimDir),
}

/// One effect, its names resolved to flattened parts.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// The part and everything mated under it leave as one piece.
    Detach { part: String, push: Push, strength: Option<f64> },
    Reveal(String),
    Hide(String),
    /// The part wears this material (index into the scene's materials).
    Look { part: String, material: usize },
    State { name: String, on: bool },
    Play(String),
    Run(String),
}

#[derive(Debug, Clone)]
pub struct ResolvedEffect {
    /// Seconds after the action starts.
    pub delay:  f64,
    pub effect: Effect,
}

#[derive(Debug, Clone)]
pub struct ResolvedAction {
    pub name:    String,
    /// `trigger P hits=N`: (flattened part, N); `None` = fired by name.
    pub trigger: Option<(String, u32)>,
    pub effects: Vec<ResolvedEffect>,
    /// The instance the declaration came from: its push words are in that
    /// instance's space. `None` for this thing's own.
    pub space:   Option<String>,
    /// On a mirrored instance's action: the source instance (prefix) whose
    /// action this is the image of; its pushes are the source's, reflected.
    pub mirror_of: Option<(String, String)>,
    pub span:    Span,
}

/// Before folding numbers: kept on the template for instances that
/// override parameters.
#[derive(Debug, Clone)]
pub(super) struct RawAction {
    pub name:    String,
    pub trigger: Option<(String, Expr)>,
    pub effects: Vec<RawEffect>,
    pub space:   Option<String>,
    /// (source prefix, image prefix) on a mirrored instance's copy
    pub mirror_of: Option<(String, String)>,
    pub span:    Span,
}

#[derive(Debug, Clone)]
pub(super) struct RawEffect {
    pub delay:    Option<Expr>,
    pub strength: Option<Expr>,
    /// `strength` is folded into this on resolve.
    pub effect:   Effect,
    pub span:     Span,
}

impl RawAction {
    pub(super) fn subst(&self, env: &ParamEnv) -> RawAction {
        let s = |e: &Option<Expr>| e.as_ref().map(|x| super::subst_expr(x, env));
        RawAction {
            trigger: self.trigger.as_ref().map(|(p, e)| (p.clone(), super::subst_expr(e, env))),
            effects: self.effects.iter().map(|f| RawEffect { delay: s(&f.delay), strength: s(&f.strength), ..f.clone() }).collect(),
            ..self.clone()
        }
    }
    /// Every name through `p` (parts, states, clips, actions, the action's own).
    fn renamed(&self, p: &dyn Fn(&str) -> String) -> RawAction {
        RawAction {
            name:    p(&self.name),
            trigger: self.trigger.as_ref().map(|(part, e)| (p(part), e.clone())),
            effects: self.effects.iter().map(|f| RawEffect {
                effect: match &f.effect {
                    Effect::Detach { part, push, strength } => Effect::Detach { part: p(part), push: *push, strength: *strength },
                    Effect::Reveal(x) => Effect::Reveal(p(x)),
                    Effect::Hide(x)   => Effect::Hide(p(x)),
                    Effect::Look { part, material } => Effect::Look { part: p(part), material: *material },
                    Effect::State { name, on } => Effect::State { name: p(name), on: *on },
                    Effect::Play(x) => Effect::Play(p(x)),
                    Effect::Run(x)  => Effect::Run(p(x)),
                },
                ..f.clone()
            }).collect(),
            ..self.clone()
        }
    }
    pub(super) fn prefixed(&self, prefix: &str) -> RawAction {
        RawAction {
            space: Some(match &self.space { Some(s) => format!("{prefix}.{s}"), None => prefix.to_string() }),
            mirror_of: self.mirror_of.as_ref().map(|(a, b)| (format!("{prefix}.{a}"), format!("{prefix}.{b}"))),
            ..self.renamed(&|n: &str| format!("{prefix}.{n}"))
        }
    }
    /// A mirrored instance's copy of its source's action: `A.X` → `C.X`.
    pub(super) fn imaged(&self, source: &str, image: &str) -> RawAction {
        let from = format!("{source}.");
        let swap = |n: &str| match n.strip_prefix(&from) { Some(rest) => format!("{image}.{rest}"), None => n.to_string() };
        RawAction { mirror_of: Some((source.to_string(), image.to_string())), ..self.renamed(&swap) }
    }
}

// ── flattening: index markers and `for` inside the body ────────────────────

/// Unroll an action's loops: every body item becomes plain effect lines,
/// the loop variable bound syntactically and index markers folded.
pub(super) fn flatten(
    a:           &ActionDecl,
    scope:       &HashMap<String, Expr>,
    env:         &ParamEnv,
    index_exprs: &[Expr],
    errs:        &mut Vec<MoxiError>,
) -> ActionDecl {
    let mut out = Vec::new();
    let mut budget = MAX_ITERATIONS;
    flatten_items(&a.body, scope, env, index_exprs, &mut budget, &mut out, &a.name.name, errs);
    ActionDecl { body: out.into_iter().map(ActionItem::Effect).collect(), ..a.clone() }
}

#[allow(clippy::too_many_arguments)]
fn flatten_items(
    items:       &[ActionItem],
    scope:       &HashMap<String, Expr>,
    env:         &ParamEnv,
    index_exprs: &[Expr],
    budget:      &mut usize,
    out:         &mut Vec<EffectLine>,
    action:      &str,
    errs:        &mut Vec<MoxiError>,
) {
    for it in items {
        match it {
            ActionItem::Effect(l) => {
                let refs = scope_refs(scope);
                let f = |x: &Expr| substitute_idents(x, &refs);
                let mut name_errs = Vec::new();
                let mut n = |id: &Ident| Ident {
                    name: resolve_markers(&id.name, scope, env, index_exprs, &mut name_errs, id.span), span: id.span,
                };
                let kind = match &l.kind {
                    EffectKind::Detach { part, push, strength } => EffectKind::Detach {
                        part: n(part), push: push.clone(), strength: strength.as_ref().map(f) },
                    EffectKind::Reveal { part } => EffectKind::Reveal { part: n(part) },
                    EffectKind::Hide { part }   => EffectKind::Hide { part: n(part) },
                    EffectKind::Look { part, material } => EffectKind::Look { part: n(part), material: material.clone() },
                    EffectKind::State { state, on } => EffectKind::State { state: n(state), on: *on },
                    EffectKind::Play { clip } => EffectKind::Play { clip: n(clip) },
                    EffectKind::Run { action } => EffectKind::Run { action: n(action) },
                };
                errs.extend(name_errs);
                out.push(EffectLine { kind, delay: l.delay.as_ref().map(f), span: l.span });
            }
            ActionItem::For { var, start, end, body, span } => {
                if scope.contains_key(&var.name) {
                    errs.push(MoxiError::ActionError { action: action.to_string(),
                        message: format!("loop variable '{}' is already bound by an enclosing loop", var.name), span: var.span });
                    continue;
                }
                let bound = |e: &Expr, which: &str, errs: &mut Vec<MoxiError>| match fold_int(e, scope, env) {
                    Ok(v) => Some(v),
                    Err(m) => {
                        errs.push(MoxiError::ActionError { action: action.to_string(),
                            message: format!("the {which} of `for {} in …`: {m}", var.name), span: *span });
                        None
                    }
                };
                let (Some(a), Some(b)) = (bound(start, "start", errs), bound(end, "end", errs)) else { continue };
                for k in a..b {
                    if *budget == 0 {
                        errs.push(MoxiError::ActionError { action: action.to_string(),
                            message: format!("loops in one action may unroll to at most {MAX_ITERATIONS} iterations"), span: *span });
                        return;
                    }
                    *budget -= 1;
                    let mut inner = scope.clone();
                    inner.insert(var.name.clone(), Expr::Int(k));
                    flatten_items(body, &inner, env, index_exprs, budget, out, action, errs);
                }
            }
        }
    }
}

// ── names ───────────────────────────────────────────────────────────────────

/// What an action may name in its thing.
pub(super) struct ActionScope<'a> {
    pub thing:          &'a str,
    /// Every flattened part with a shape, in declared order.
    pub part_order:     &'a [String],
    pub instance_of:    &'a HashMap<String, String>,
    /// (written subject, flattened subject) of this thing's own mates.
    pub own_align:      &'a [(String, String)],
    pub relations:      &'a [Placement],
    pub material_index: &'a HashMap<String, usize>,
    pub states:         &'a [String],
    pub clips:          &'a [String],
}

/// This thing's own actions → raw actions on flattened names. Every problem
/// is an `ActionError` listing what could be named. `run` targets are
/// checked later (`check_actions`), once inherited actions are known.
pub(super) fn resolve_action_decls(decls: &[ActionDecl], sc: &ActionScope, errs: &mut Vec<MoxiError>) -> Vec<RawAction> {
    let tops = || -> String {
        sc.part_order.iter().filter(|p| !p.contains('.')).map(|p| p.as_str())
            .chain(sc.instance_of.keys().map(|s| s.as_str())).collect::<Vec<_>>().join(", ")
    };
    let align: HashMap<&str, &str> = sc.own_align.iter().map(|(w, s)| (w.as_str(), s.as_str())).collect();
    let subjects: HashSet<&str> = sc.relations.iter().map(|p| p.subject_name()).collect();
    let detachable = || -> String {
        let mut v: Vec<&str> = sc.own_align.iter().map(|(w, _)| w.as_str()).collect();
        v.extend(sc.relations.iter().filter_map(|p| match p {
            Placement::Mirror { subject, .. } if !subject.contains('.') => Some(subject.as_str()),
            _ => None,
        }));
        v.dedup();
        v.join(", ")
    };
    let mut materials: Vec<(&String, &usize)> = sc.material_index.iter().filter(|(n, _)| !n.contains('@')).collect();
    materials.sort_by_key(|(_, i)| **i);
    let material_list = materials.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
    let none_or = |v: &[String]| if v.is_empty() { "none".to_string() } else { v.join(", ") };

    let mut seen: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for d in decls {
        let aname = d.name.name.clone();
        let aerr = |message: String, span: Span| MoxiError::ActionError { action: aname.clone(), message, span };
        if !seen.insert(d.name.name.as_str()) {
            errs.push(aerr(format!("'{}' has two `action` declarations — give each its own name", aname), d.span));
            continue;
        }
        // a part or a whole instance (every part of it)
        let targets = |id: &Ident, what: &str, errs: &mut Vec<MoxiError>| -> Vec<String> {
            if sc.part_order.contains(&id.name) { return vec![id.name.clone()]; }
            if sc.instance_of.contains_key(&id.name) {
                let pre = format!("{}.", id.name);
                return sc.part_order.iter().filter(|p| p.starts_with(&pre)).cloned().collect();
            }
            errs.push(aerr(format!("{what}: no part '{}' in '{}' — parts: {}", id.name, sc.thing, tops()), id.span));
            Vec::new()
        };
        let trigger = match &d.trigger {
            None => None,
            Some(t) => {
                let part = if sc.part_order.contains(&t.part.name) || subjects.contains(t.part.name.as_str()) {
                    Some(t.part.name.clone())
                } else if let Some(s) = align.get(t.part.name.as_str()) {
                    Some(s.to_string()) // an instance: its root
                } else {
                    errs.push(aerr(format!("trigger: no part '{}' in '{}' — parts: {}", t.part.name, sc.thing, tops()), t.part.span));
                    None
                };
                part.map(|p| (p, t.hits.clone()))
            }
        };
        if d.trigger.is_some() && trigger.is_none() { continue; }
        let mut effects = Vec::new();
        for it in &d.body {
            let ActionItem::Effect(l) = it else { continue }; // flattened
            let mut push_fx = |effect: Effect, strength: Option<Expr>| effects.push(RawEffect {
                delay: l.delay.clone(), strength, effect, span: l.span });
            match &l.kind {
                EffectKind::Detach { part, push, strength } => {
                    let subj = align.get(part.name.as_str()).map(|s| s.to_string())
                        .or_else(|| subjects.contains(part.name.as_str()).then(|| part.name.clone()));
                    let Some(subj) = subj else {
                        let why = if sc.part_order.contains(&part.name) {
                            format!("'{}' is the root of '{}' — nothing holds it, so nothing can detach it", part.name, sc.thing)
                        } else {
                            format!("'{}' is not a part of '{}' that a mate places", part.name, sc.thing)
                        };
                        errs.push(aerr(format!("detach: {why} — parts that can come off: {} \
                                                (parts inside instances by their full names, e.g. `D.Head`)",
                                               detachable()), part.span));
                        continue;
                    };
                    let push = match push {
                        None => Push::Out,
                        Some(w) if w.name == "out" => Push::Out,
                        Some(w) => match AimDir::parse(&w.name) {
                            Some(dir) => Push::Dir(dir),
                            None => {
                                errs.push(aerr(format!("`push={}` is not a direction — use out (away from what it hangs \
                                    from, the default) or {}", w.name, AimDir::WORDS.join(", ")), w.span));
                                continue;
                            }
                        },
                    };
                    push_fx(Effect::Detach { part: subj, push, strength: None }, strength.clone());
                }
                EffectKind::Reveal { part } => for t in targets(part, "reveal", errs) { push_fx(Effect::Reveal(t), None) },
                EffectKind::Hide { part }   => for t in targets(part, "hide", errs) { push_fx(Effect::Hide(t), None) },
                EffectKind::Look { part, material } => {
                    let Some(&m) = sc.material_index.get(&material.name) else {
                        errs.push(aerr(format!("no material '{}' — materials: {material_list}", material.name), material.span));
                        continue;
                    };
                    for t in targets(part, "material", errs) { push_fx(Effect::Look { part: t, material: m }, None) }
                }
                EffectKind::State { state, on } => {
                    if !sc.states.contains(&state.name) {
                        errs.push(aerr(format!("no state '{}' in '{}' — states: {}", state.name, sc.thing, none_or(sc.states)), state.span));
                        continue;
                    }
                    push_fx(Effect::State { name: state.name.clone(), on: *on }, None);
                }
                EffectKind::Play { clip } => {
                    if !sc.clips.contains(&clip.name) {
                        errs.push(aerr(format!("no clip '{}' in '{}' — clips (its `animate` and `revolve` names): {}",
                                               clip.name, sc.thing, none_or(sc.clips)), clip.span));
                        continue;
                    }
                    push_fx(Effect::Play(clip.name.clone()), None);
                }
                EffectKind::Run { action } => push_fx(Effect::Run(action.name.clone()), None),
            }
        }
        out.push(RawAction { name: aname, trigger, effects, space: None, mirror_of: None, span: d.span });
    }
    out
}

// ── numbers ─────────────────────────────────────────────────────────────────

fn num(e: &Expr, env: &ParamEnv) -> Result<f64, String> {
    match value::eval(e, env) {
        Ok(Value::Num(v)) => Ok(v),
        Ok(other) => Err(format!("must be a number, got a {}", other.kind())),
        Err(err) => Err(err.message),
    }
}

/// Fold `hits`, `strength` and `delay`.
pub(super) fn fold_action(raw: &RawAction, env: &ParamEnv, errs: &mut Vec<MoxiError>) -> Option<ResolvedAction> {
    let aerr = |message: String, span: Span| MoxiError::ActionError { action: raw.name.clone(), message, span };
    let mut ok = true;
    let trigger = match &raw.trigger {
        None => None,
        Some((part, e)) => match num(e, env) {
            Ok(v) if v >= 1.0 && v.fract() == 0.0 && v <= u32::MAX as f64 => Some((part.clone(), v as u32)),
            Ok(v) => { errs.push(aerr(format!("trigger: `hits` must be a whole number of at least 1, got {v}"), raw.span)); return None; }
            Err(m) => { errs.push(aerr(format!("trigger: `hits` {m}"), raw.span)); return None; }
        },
    };
    let mut effects = Vec::new();
    for f in &raw.effects {
        let delay = match &f.delay {
            None => 0.0,
            Some(e) => match num(e, env) {
                Ok(v) if v >= 0.0 => v,
                Ok(v) => { errs.push(aerr(format!("`delay` is seconds after the action starts, at least 0 — got {v}"), f.span)); ok = false; continue; }
                Err(m) => { errs.push(aerr(format!("`delay` {m}"), f.span)); ok = false; continue; }
            },
        };
        let mut effect = f.effect.clone();
        if let (Some(e), Effect::Detach { strength, .. }) = (&f.strength, &mut effect) {
            match num(e, env) {
                Ok(v) if v > 0.0 => *strength = Some(v),
                Ok(v) => { errs.push(aerr(format!("`strength` is a speed in the thing's units per second, more than 0 — got {v}"), f.span)); ok = false; continue; }
                Err(m) => { errs.push(aerr(format!("`strength` {m}"), f.span)); ok = false; continue; }
            }
        }
        effects.push(ResolvedEffect { delay, effect });
    }
    ok.then(|| ResolvedAction { name: raw.name.clone(), trigger, effects, space: raw.space.clone(),
                                mirror_of: raw.mirror_of.clone(), span: raw.span })
}

/// Once every action of the thing is known (own and inherited): every `run`
/// names an action, and no chain of `run`s comes back to where it started
/// (the language is total).
pub(super) fn check_actions(actions: &[ResolvedAction], thing: &str, errs: &mut Vec<MoxiError>) {
    let names: Vec<&str> = actions.iter().map(|a| a.name.as_str()).collect();
    let all = || if names.is_empty() { "none".to_string() } else { names.join(", ") };
    for a in actions {
        for f in &a.effects {
            if let Effect::Run(t) = &f.effect {
                if !names.contains(&t.as_str()) {
                    errs.push(MoxiError::ActionError { action: a.name.clone(),
                        message: format!("run: no action '{t}' in '{thing}' — actions: {}", all()), span: a.span });
                }
            }
        }
    }
    // cycles: depth-first, reporting each cycle once
    let runs = |a: &ResolvedAction| -> Vec<String> {
        a.effects.iter().filter_map(|f| match &f.effect { Effect::Run(t) => Some(t.clone()), _ => None }).collect()
    };
    let by_name: HashMap<&str, &ResolvedAction> = actions.iter().map(|a| (a.name.as_str(), a)).collect();
    let mut done: HashSet<String> = HashSet::new();
    for a in actions {
        let mut path: Vec<String> = Vec::new();
        visit(&a.name, &by_name, &runs, &mut path, &mut done, errs);
    }
}

fn visit(
    name:    &str,
    by_name: &HashMap<&str, &ResolvedAction>,
    runs:    &dyn Fn(&ResolvedAction) -> Vec<String>,
    path:    &mut Vec<String>,
    done:    &mut HashSet<String>,
    errs:    &mut Vec<MoxiError>,
) {
    if done.contains(name) { return; }
    if let Some(i) = path.iter().position(|p| p == name) {
        let mut chain: Vec<String> = path[i..].to_vec();
        chain.push(name.to_string());
        let a = by_name[path[i].as_str()];
        errs.push(MoxiError::ActionError { action: path[i].clone(),
            message: format!("`run` goes round in a circle: {} — an action cannot run itself, directly or through others",
                             chain.join(" → ")), span: a.span });
        return;
    }
    let Some(a) = by_name.get(name) else { return };
    path.push(name.to_string());
    for t in runs(a) { visit(&t, by_name, runs, path, done, errs); }
    path.pop();
    done.insert(name.to_string());
}

#[cfg(test)]
mod tests {
    use crate::pipeline::compile_to_scene;
    use crate::scene::EffectOut;

    const HUT: &str = "material W { color = \"#888888\" }
    material Ash { color = \"#222222\" }
    thing Hut(n=3, kick=2) {
        part Floor { shape = box(width=40, height=2, depth=40), material = W }
        part Back  { shape = box(width=40, height=30, depth=2), material = W }
        part Roof  { shape = box(width=44, height=2, depth=44), material = W }
        part Pit   { shape = box(width=10, height=1, depth=10), material = W }
        part Lamp  { shape = sphere(radius=2), material = W }
        relation {
            Back.bottom on Floor.top shift=(0, -19)
            Roof.bottom on Back.top shift=(0, 19)
            Pit.center  on Floor.top
            Lamp.center on Back.top shift=(0, 4)
        }
        for k in 0..n {
            part Plank[k] { shape = box(width=8, height=30, depth=2), material = W }
            relation { Plank[k].bottom on Floor.top shift=(-12 + 12 * k, 19) }
        }
        state Charred { Back material = Ash }
        pose Tilt { Roof lean=(0, 10) }
        animate Sway { 0 rest  1 Tilt }
        ACTIONS
    }";

    fn hut(actions: &str) -> String { HUT.replace("ACTIONS", actions) + "\nprint Hut" }

    fn errors(src: &str) -> String {
        match compile_to_scene(src) {
            Ok(_) => panic!("expected an error"),
            Err(es) => es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"),
        }
    }

    /// Every effect reaches the scene with its names, numbers, pushes and
    /// looks; a revealed part is hidden at rest; loops unroll in order.
    #[test]
    fn an_action_reaches_the_scene() {
        let scene = compile_to_scene(&hut("action Explode { trigger Floor hits=kick + 1 \
            detach Roof push=up strength=60  for k in 0..n { detach Plank[k] delay=k * 0.1 } \
            reveal Pit  hide Lamp  Floor material = Ash  state Charred on  play Sway  run Settle delay=0.5 } \
            action Settle { detach Lamp }")).unwrap();
        let l = &scene.layers[0];
        let a = &l.actions[0];
        assert_eq!(a.name, "Explode");
        let t = a.trigger.as_ref().unwrap();
        assert_eq!((t.part.as_str(), t.hits), ("Floor", 3));
        let e = &a.effects;
        assert_eq!(e.len(), 10, "{e:?}");
        assert!(matches!(&e[0], EffectOut::Detach { part, push, strength: Some(s), delay }
            if part == "Roof" && *push == [0.0, 1.0, 0.0] && *s == 60.0 && *delay == 0.0));
        for k in 0..3 {
            assert!(matches!(&e[1 + k], EffectOut::Detach { part, strength: None, delay, .. }
                if *part == format!("Plank[{k}]") && (*delay - 0.1 * k as f64).abs() < 1e-12), "{:?}", e[1 + k]);
        }
        assert!(matches!(&e[4], EffectOut::Reveal { part, .. } if part == "Pit"));
        assert!(matches!(&e[5], EffectOut::Hide { part, .. } if part == "Lamp"));
        assert!(matches!(&e[6], EffectOut::Look { part, look, .. } if part == "Floor" && look.material.as_deref() == Some("Ash")));
        assert!(matches!(&e[7], EffectOut::State { state, on: true, .. } if state == "Charred"));
        assert!(matches!(&e[8], EffectOut::Play { clip, .. } if clip == "Sway"));
        assert!(matches!(&e[9], EffectOut::Run { action, delay } if action == "Settle" && *delay == 0.5));
        let part = |n: &str| l.parts.iter().find(|p| p.name == n).unwrap();
        assert!(part("Pit").hidden && !part("Lamp").hidden, "revealed: hidden at rest; hidden by an action: not");
        assert!(l.actions[1].trigger.is_none(), "no trigger: fired by name");
    }

    /// Instances bring their actions, prefixed, under their own arguments
    /// (a loop count that depends on a parameter included).
    #[test]
    fn instances_inherit_actions_under_their_arguments() {
        let src = HUT.replace("ACTIONS", "action Kick { trigger Back hits=kick  for k in 0..n { detach Plank[k] } }") + "
        thing Yard {
            part Ground { shape = box(width=200, height=2, depth=100), material = W }
            part A { thing = Hut }
            part B { thing = Hut(n=2, kick=5) }
            relation {
                A.Floor.bottom on Ground.top shift=(-50, 0)
                B.Floor.bottom on Ground.top shift=(50, 0)
            }
        }
        print Yard";
        let scene = compile_to_scene(&src).unwrap_or_else(|e| panic!("{e:?}"));
        let acts = &scene.layers[0].actions;
        let a = acts.iter().find(|x| x.name == "A.Kick").unwrap();
        let b = acts.iter().find(|x| x.name == "B.Kick").unwrap();
        assert_eq!(a.trigger.as_ref().unwrap().part, "A.Back");
        assert_eq!((a.trigger.as_ref().unwrap().hits, a.effects.len()), (2, 3));
        assert_eq!((b.trigger.as_ref().unwrap().hits, b.effects.len()), (5, 2), "B's own arguments");
        assert!(matches!(&b.effects[1], EffectOut::Detach { part, .. } if part == "B.Plank[1]"));
    }

    /// The errors list what could have been named; `run` cannot loop.
    #[test]
    fn names_are_checked_with_the_vocabulary_listed() {
        assert!(errors(&hut("action X { detach Rof }")).contains("parts that can come off: Back, Roof, Pit, Lamp, Plank[0]"));
        assert!(errors(&hut("action X { detach Floor }")).contains("'Floor' is the root of 'Hut'"));
        assert!(errors(&hut("action X { state Burnt on }")).contains("no state 'Burnt' in 'Hut' — states: Charred"));
        assert!(errors(&hut("action X { play Swing }")).contains("clips (its `animate` and `revolve` names): Sway"));
        assert!(errors(&hut("action X { Roof material = Soot }")).contains("no material 'Soot' — materials: W, Ash"));
        assert!(errors(&hut("action X { reveal Pt }")).contains("reveal: no part 'Pt' in 'Hut' — parts: Floor, Back"));
        assert!(errors(&hut("action X { run Y }")).contains("run: no action 'Y' in 'Hut' — actions: X"));
        assert!(errors(&hut("action X { run Y }  action Y { run X }")).contains("goes round in a circle: X → Y → X"));
        assert!(errors(&hut("action X { trigger Floor hits=0 }")).contains("`hits` must be a whole number of at least 1"));
        assert!(errors(&hut("action X { detach Roof strength=-1 }")).contains("`strength` is a speed"));
        assert!(errors(&hut("action X { reveal Pit delay=-1 }")).contains("`delay` is seconds after the action starts"));
        assert!(errors(&hut("action X { reveal Pit }  action X { hide Lamp }")).contains("two `action` declarations"));
        assert!(errors(&hut("breakable Roof { event=Blow }")).contains("`breakable` is gone"));
    }

    /// `out` leaves the socket: up off the body's top; along the head's north
    /// socket for the centre-mated eye; a word is the thing's own direction.
    #[test]
    fn out_pushes_leave_the_socket() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/BREAK.md")).unwrap();
        let scene = compile_to_scene(src.as_str()).unwrap();
        let push = |n: &str| match &scene.layers[0].actions.iter().find(|a| a.name == n).unwrap().effects[0] {
            EffectOut::Detach { push, .. } => *push, _ => panic!() };
        let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9);
        assert!(near(push("D.BreakHead"), [0.0, 1.0, 0.0]), "{:?}", push("D.BreakHead"));
        assert!(near(push("D.BreakEyeR"), [0.0, 0.0, 1.0]), "centre mate: the socket's normal, {:?}", push("D.BreakEyeR"));
        assert!(near(push("D.BreakArmR"), [-1.0, 0.0, 0.0]) && near(push("D.BreakArmL"), [1.0, 0.0, 0.0]));
        assert!(near(push("R.Launch"), [0.0, 1.0, 0.0]));
    }

    /// An action inside a loop: one per iteration, its name indexed.
    #[test]
    fn actions_in_a_loop_are_one_per_iteration() {
        let src = HUT.replace("ACTIONS", "for k in 0..n { action BreakPlank[k] { trigger Plank[k] hits=k + 1  detach Plank[k] } }")
            + "\nprint Hut";
        let scene = compile_to_scene(&src).unwrap_or_else(|e| panic!("{e:?}"));
        let a = &scene.layers[0].actions;
        let names: Vec<&str> = a.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["BreakPlank[0]", "BreakPlank[1]", "BreakPlank[2]"]);
        assert_eq!(a[2].trigger.as_ref().map(|t| (t.part.as_str(), t.hits)), Some(("Plank[2]", 3)));
    }

    /// A mirrored instance brings its source's actions, renamed to its own
    /// parts, under the source's arguments, its pushes reflected (`out` and
    /// words alike). (A part mirrored inside a thing gets its own action
    /// written out: scripts/BREAK.md's arms.)
    #[test]
    fn a_mirrored_instance_brings_its_sources_actions_reflected() {
        let src = "material W { color = \"#888888\" }
        thing Guy(t=5) {
            part Body { shape = box(width=20, height=40, depth=10), material = W }
            part Head { shape = sphere(radius=6), material = W }
            part ArmR { shape = capsule(height=20, radius=3), material = W }
            relation {
                Head.bottom on Body.top
                ArmR.top on Body.east
            }
            action BreakHead { trigger Head hits=t  detach Head push=east }
            action BreakArmR { trigger ArmR hits=2  detach ArmR }
        }
        thing Pit {
            part Floor { shape = box(width=300, height=10, depth=300), material = W }
            part A { thing = Guy(t=9) }
            part C { thing = Guy }
            relation {
                A.Body.bottom on Floor.top shift=(-60, 0)
                C symmetric_across Floor from=A
            }
        }
        print Pit";
        let scene = compile_to_scene(src).unwrap_or_else(|e| panic!("{e:?}"));
        let acts = &scene.layers[0].actions;
        let act = |n: &str| acts.iter().find(|a| a.name == n).unwrap_or_else(|| panic!("no {n}: {:?}",
            acts.iter().map(|a| &a.name).collect::<Vec<_>>()));
        let c = act("C.BreakHead");
        assert_eq!(c.trigger.as_ref().map(|t| (t.part.as_str(), t.hits)), Some(("C.Head", 9)), "the source's arguments");
        let push = |a: &crate::scene::ActionOut| match &a.effects[0] { EffectOut::Detach { push, .. } => *push, _ => panic!() };
        let (pa, pc) = (push(act("A.BreakHead")), push(c));
        assert!((pa[0] - 1.0).abs() < 1e-9 && (pc[0] + 1.0).abs() < 1e-9, "east on A is west on its image: {pa:?} / {pc:?}");
        let (oa, oc) = (push(act("A.BreakArmR")), push(act("C.BreakArmR")));
        assert!((oa[0] - 1.0).abs() < 1e-9 && (oc[0] + 1.0).abs() < 1e-9, "`out` reflected too: {oa:?} / {oc:?}");
    }
}
