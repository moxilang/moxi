//! Revolves — continuous rotation of mates at a rate (`revolve`; the shape
//! modifier `spin(s, axis=, degrees=)` is a static rotation and unrelated).
//!
//! `revolve Turn { GearA twist=90  GearB twist=-45 }` (degrees per second) is the
//! pose idea continued in time: at t the mate's qualifier is its rest value
//! plus RATE·t. The resolver checks the lines and works out the clip's period
//! — the smallest time after which every part has made a whole number of
//! turns — and its keyframe count, so that no part turns more than 90°
//! between keys (a player slerps the short way; 90° leaves no doubt). The
//! pipeline solves each keyframe as a pose over rest; nothing new reaches
//! the solver.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::value::{self, Value};

use super::{joint::JointScope, subst_expr, ParamEnv, ResolvedJoint};

/// Longest loop a revolve may have, in seconds, and most keyframes it may need.
pub const MAX_PERIOD_S: f64 = 600.0;
pub const MAX_KEYS: usize = 720;

#[derive(Debug, Clone)]
pub struct ResolvedSpin {
    pub name:   String,
    pub parts:  Vec<SpinPart>,
    /// Seconds after which every part has made whole turns.
    pub period: f64,
    /// Keyframe intervals over one period (keys = segments + 1).
    pub segments: usize,
    pub span:   Span,
}

#[derive(Debug, Clone)]
pub struct SpinPart {
    /// Flattened subject part of the mate.
    pub part: String,
    pub qual: Qual,
    /// Degrees per second.
    pub rate: f64,
    /// The rate as written, for instances that override parameters.
    pub rate_expr: Expr,
    pub span: Span,
}

impl ResolvedSpin {
    /// The same revolve under an instance: names prefixed, rates
    /// re-evaluated under the instance's arguments, period recomputed.
    pub(super) fn inherited(&self, prefix: &str, env: &ParamEnv, errs: &mut Vec<MoxiError>) -> Option<ResolvedSpin> {
        let name = format!("{prefix}.{}", self.name);
        let mut parts = Vec::new();
        for p in &self.parts {
            let e = subst_expr(&p.rate_expr, env);
            let rate = match value::eval(&e, env) {
                Ok(Value::Num(v)) if v != 0.0 => v,
                _ => {
                    errs.push(MoxiError::RevolveError { revolve: name.clone(), message: format!(
                        "'{}' {} rate is not a non-zero number under this instance's arguments",
                        p.part, p.qual.label()), span: p.span });
                    return None;
                }
            };
            parts.push(SpinPart { part: format!("{prefix}.{}", p.part), qual: p.qual, rate, rate_expr: e, span: p.span });
        }
        let (period, segments) = period_of(&parts.iter().map(|p| p.rate).collect::<Vec<_>>());
        if period > MAX_PERIOD_S || segments > MAX_KEYS {
            errs.push(MoxiError::RevolveError { revolve: name.clone(), message: format!(
                "under this instance's arguments the rates only line up again after {period} s; a looping \
                 revolve needs at most {MAX_PERIOD_S} s and {MAX_KEYS} keyframes"), span: self.span });
            return None;
        }
        Some(ResolvedSpin { name, parts, period, segments, span: self.span })
    }
}

pub(super) fn walk_spin_exprs(s: &mut SpinDecl, f: &mut dyn FnMut(&Expr) -> Expr) {
    for l in &mut s.lines {
        l.rate = f(&l.rate);
    }
}

fn gcd(a: u64, b: u64) -> u64 { if b == 0 { a } else { gcd(b, a % b) } }

/// Period and keyframe count for a set of rates (degrees per second, none
/// zero). Rates are taken to 1/1000 °/s.
pub fn period_of(rates: &[f64]) -> (f64, usize) {
    let milli: Vec<u64> = rates.iter().map(|r| (r.abs() * 1000.0).round().max(1.0) as u64).collect();
    let g = milli.iter().copied().fold(0, gcd);
    let period = 360_000.0 / g as f64;
    let fastest = rates.iter().fold(0.0_f64, |m, r| m.max(r.abs()));
    let turns = fastest * period / 360.0;
    let segments = ((turns * 4.0) - 1e-9).ceil().max(2.0) as usize;
    (period, segments)
}

pub(super) fn resolve_spins(
    decls:      &[SpinDecl],
    sc:         &JointScope,
    joints:     &[ResolvedJoint],
    animations: &HashSet<String>,
    env:        &ParamEnv,
    errs:       &mut Vec<MoxiError>,
) -> Vec<ResolvedSpin> {
    let align_of: HashMap<&str, &str> = sc.own_align.iter().map(|(w, s)| (w.as_str(), s.as_str())).collect();
    let movable = if sc.own_align.is_empty() {
        format!("'{}' has no mates", sc.thing)
    } else {
        format!("parts placed by a mate in '{}': {}", sc.thing,
            sc.own_align.iter().map(|(w, _)| w.as_str()).collect::<Vec<_>>().join(", "))
    };
    let mut seen_names = HashSet::new();
    let mut out = Vec::new();
    for d in decls {
        let name = d.name.name.clone();
        let serr = |message: String, span: Span| MoxiError::RevolveError { revolve: name.clone(), message, span };
        if !seen_names.insert(name.clone()) || animations.contains(&name) {
            errs.push(MoxiError::DuplicateName { name: name.clone(), span: d.name.span });
            continue;
        }
        if d.lines.is_empty() {
            errs.push(serr("needs at least one line, e.g. `revolve Turn { Wheel twist=90 }` \
                            (degrees per second)".into(), d.span));
            continue;
        }
        let mut parts: Vec<SpinPart> = Vec::new();
        let mut seen_lines = HashSet::new();
        let before = errs.len();
        for l in &d.lines {
            let who = l.part.name.as_str();
            let Some(subject) = align_of.get(who) else {
                let msg = if let Some(src) = sc.own_mirror.get(who) {
                    format!("'{who}' is the mirror image of '{src}'; revolve '{src}' — the image follows, turning the other way")
                } else if sc.part_names.contains_key(who) || sc.instance_of.contains_key(who) {
                    match who.split_once('.').filter(|(i, _)| sc.instance_of.contains_key(*i)) {
                        Some((inst, _)) => format!(
                            "'{who}' is placed inside the instance '{inst}' (a '{}'); declare the revolve in '{}' itself",
                            sc.instance_of[inst], sc.instance_of[inst]),
                        None => format!("'{who}' is not placed by a mate (it is the root of '{}'); a revolve turns a mate — {movable}", sc.thing),
                    }
                } else {
                    format!("'{who}' is not a part of '{}' — {movable}", sc.thing)
                };
                errs.push(serr(msg, l.span));
                continue;
            };
            let qual = match l.qual.name.as_str() {
                "twist" => Qual::Twist,
                "pitch" => Qual::Pitch,
                other => {
                    errs.push(serr(format!(
                        "'{who}' cannot revolve `{other}` — a revolve turns `twist` (about the mate's normal: \
                         a gear on its plate, a halo) or `pitch` (about the socket's x axis: a wheel on \
                         its edge)"), l.span));
                    continue;
                }
            };
            let rate = match value::eval(&l.rate, env) {
                Ok(Value::Num(v)) if v != 0.0 => v,
                Ok(Value::Num(_)) => {
                    errs.push(serr(format!("'{who}' {} rate is 0 — a revolve needs a turning rate in degrees \
                                            per second, e.g. `{who} {}=90`", l.qual.name, l.qual.name), l.span));
                    continue;
                }
                Ok(other) => {
                    errs.push(serr(format!("'{who}' {} rate must be a number of degrees per second, got a {}",
                        l.qual.name, other.kind()), l.span));
                    continue;
                }
                Err(e) => { errs.push(serr(format!("'{who}' {} rate: {}", l.qual.name, e.message), l.span)); continue; }
            };
            if !seen_lines.insert((subject.to_string(), qual)) {
                errs.push(serr(format!("'{who}' revolves {} twice; give it one rate", l.qual.name), l.span));
                continue;
            }
            if let Some((_, lo, hi)) = joints.iter().find(|j| j.part == *subject)
                .and_then(|j| j.limits.iter().find(|(q, _, _)| *q == qual)) {
                errs.push(serr(format!(
                    "'{who}' turns full circles, but joint '{who}' limits {} to {lo}..{hi}; drop the range \
                     or the revolve", l.qual.name), l.span));
                continue;
            }
            parts.push(SpinPart { part: subject.to_string(), qual, rate, rate_expr: l.rate.clone(), span: l.span });
        }
        if errs.len() > before || parts.is_empty() { continue; }
        let rates: Vec<f64> = parts.iter().map(|p| p.rate).collect();
        let (period, segments) = period_of(&rates);
        if period > MAX_PERIOD_S || segments > MAX_KEYS {
            let quoted = parts.iter().map(|p| format!("{} {}", p.rate, p.qual.label()))
                .collect::<Vec<_>>().join(", ");
            errs.push(serr(format!(
                "rates {quoted} (°/s) only line up again after {period} s ({segments} keyframes); a looping \
                 revolve needs at most {MAX_PERIOD_S} s and {MAX_KEYS} keyframes — choose rates with a simple \
                 ratio (90 and -45, 120 and -80)"), d.span));
            continue;
        }
        out.push(ResolvedSpin { name, parts, period, segments, span: d.span });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::resolver::{ResolvedEntity, Resolver};

    fn resolve(src: &str) -> Result<Vec<ResolvedEntity>, Vec<String>> {
        let (tokens, lex) = Lexer::new(src).tokenize();
        assert!(lex.is_empty(), "lex errors: {lex:?}");
        let (doc, perrs) = Parser::new(tokens).parse();
        if !perrs.is_empty() { return Err(perrs.iter().map(|e| e.to_string()).collect()); }
        let (scene, errs) = Resolver::new().resolve(doc);
        if errs.is_empty() { Ok(scene.entities) } else { Err(errs.iter().map(|e| e.to_string()).collect()) }
    }

    const GEARS: &str = r#"
material M { color = gray }
thing G {
    part Plate { shape = box(width=40, height=2, depth=20), material = M }
    part A { shape = cylinder(height=2, radius=4), material = M }
    part B { shape = cylinder(height=2, radius=8), material = M }
    relation {
        A.bottom on Plate.top shift=(-6, 0)
        B.bottom on Plate.top shift=(6, 0) twist=7.5
    }
    SPIN
    resolve voxel_size = 1
}
print G detail=low
"#;

    fn with(spin: &str) -> String { GEARS.replace("SPIN", spin) }

    #[test]
    fn period_is_when_every_part_has_made_whole_turns() {
        assert_eq!(period_of(&[90.0]), (4.0, 4));
        assert_eq!(period_of(&[90.0, -45.0]), (8.0, 8));
        assert_eq!(period_of(&[120.0, -80.0]), (9.0, 12));
        let (t, n) = period_of(&[30.0]);
        assert_eq!(t, 12.0);
        assert_eq!(n, 4, "one turn → four 90° steps");
    }

    #[test]
    fn a_spin_resolves_to_rates_on_mate_subjects() {
        let ents = resolve(&with("revolve Turn { A twist=90  B twist=-45 }")).expect("resolves");
        let sp = &ents[0].spins[0];
        assert_eq!(sp.name, "Turn");
        assert_eq!((sp.period, sp.segments), (8.0, 8));
        assert_eq!(sp.parts.iter().map(|p| (p.part.as_str(), p.rate)).collect::<Vec<_>>(),
            vec![("A", 90.0), ("B", -45.0)]);
    }

    fn err_of(spin: &str) -> String {
        resolve(&with(spin)).expect_err("must fail").join("\n")
    }

    #[test]
    fn errors_name_the_vocabulary() {
        let e = err_of("revolve Turn { Gear twist=90 }");
        assert!(e.contains("'Gear' is not a part of 'G'") && e.contains("parts placed by a mate in 'G': A, B"), "{e}");
        let e = err_of("revolve Turn { Plate twist=90 }");
        assert!(e.contains("is not placed by a mate (it is the root of 'G')"), "{e}");
        let e = err_of("revolve Turn { A lean=90 }");
        assert!(e.contains("cannot revolve `lean`") && e.contains("`twist`") && e.contains("`pitch`"), "{e}");
        let e = err_of("revolve Turn { A twist=0 }");
        assert!(e.contains("rate is 0"), "{e}");
        let e = err_of("revolve Turn { A twist=90  A twist=45 }");
        assert!(e.contains("revolves twist twice"), "{e}");
        let e = err_of("revolve Turn { A twist=90  B twist=37.001 }");
        assert!(e.contains("only line up again after") && e.contains("simple"), "{e}");
        let e = err_of("joint A { twist=0..40 }\n revolve Turn { A twist=90 }");
        assert!(e.contains("limits twist to 0..40"), "{e}");
        let e = err_of("revolve Turn { }");
        assert!(e.contains("needs at least one line"), "{e}");
    }

    #[test]
    fn a_spin_cannot_share_a_name_with_an_animation() {
        let src = with("pose Up { A twist=10 }\n animate Turn { 0 rest  1 Up }\n revolve Turn { A twist=90 }");
        let e = resolve(&src).expect_err("must fail").join("\n");
        assert!(e.contains("Turn"), "{e}");
    }
}
