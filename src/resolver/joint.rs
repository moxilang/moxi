//! Joint ranges — living models, step 5 (DOC-20261004-living-models-design §3.4).
//!
//! `joint ArmR { lean=(-90..120, -20..20)  twist=-30..30 }` bounds the values a
//! mate's qualifiers may take. Every mate is already a joint; this adds a
//! check and metadata. The check is on NUMBERS — the mate's value at rest and
//! in every pose (poses are folded overrides, so nothing is solved here) —
//! which is exactly what the author wrote and what the error can quote.
//! A thing's joints are inherited by its instances, prefixed, under the
//! instance's arguments, so `Wing pose=Up` is checked in the outer thing too.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::value::{self, Value};

use super::{subst_expr, ParamEnv, ResolvedPose};

#[derive(Debug, Clone)]
pub struct ResolvedJoint {
    /// Flattened subject part of the mate.
    pub part:   String,
    /// (qualifier, lo, hi), each bound folded.
    pub limits: Vec<(Qual, f64, f64)>,
    pub span:   Span,
}

/// Before folding: kept on the template for instances that override
/// parameters.
#[derive(Debug, Clone)]
pub(super) struct RawJoint {
    pub part:   String,
    pub limits: Vec<(Qual, Expr, Expr)>,
    pub span:   Span,
}

impl RawJoint {
    pub(super) fn subst(&self, env: &ParamEnv) -> RawJoint {
        RawJoint {
            part: self.part.clone(),
            limits: self.limits.iter().map(|(q, a, b)| (*q, subst_expr(a, env), subst_expr(b, env))).collect(),
            span: self.span,
        }
    }
    pub(super) fn prefixed(&self, prefix: &str) -> RawJoint {
        RawJoint { part: format!("{prefix}.{}", self.part), ..self.clone() }
    }
}

pub(super) struct JointScope<'a> {
    pub thing:       &'a str,
    pub own_align:   &'a [(String, String)],
    pub own_mirror:  &'a HashMap<String, String>,
    pub part_names:  &'a HashMap<String, Span>,
    pub instance_of: &'a HashMap<String, String>,
}

/// This thing's own `joint` declarations → raw joints on flattened subjects.
pub(super) fn resolve_joint_decls(decls: &[JointDecl], sc: &JointScope, errs: &mut Vec<MoxiError>) -> Vec<RawJoint> {
    let align_of: HashMap<&str, &str> = sc.own_align.iter().map(|(w, s)| (w.as_str(), s.as_str())).collect();
    let movable = if sc.own_align.is_empty() {
        format!("'{}' has no mates", sc.thing)
    } else {
        format!("parts placed by a mate in '{}': {}", sc.thing,
            sc.own_align.iter().map(|(w, _)| w.as_str()).collect::<Vec<_>>().join(", "))
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in decls {
        let name = d.part.name.clone();
        let jerr = |message: String| MoxiError::JointError { part: name.clone(), message, span: d.span };
        let Some(subject) = align_of.get(name.as_str()) else {
            let msg = if let Some(src) = sc.own_mirror.get(&name) {
                format!("is the mirror image of '{src}'; give '{src}' the joint — the image follows")
            } else if sc.part_names.contains_key(&name) || sc.instance_of.contains_key(&name) {
                match name.split_once('.').filter(|(i, _)| sc.instance_of.contains_key(*i)) {
                    Some((inst, _)) => format!(
                        "is placed inside the instance '{inst}' (a '{}'); declare the joint in '{}' itself",
                        sc.instance_of[inst], sc.instance_of[inst]),
                    None => format!("is not placed by a mate (it is the root of '{}'); a joint is a mate — {movable}", sc.thing),
                }
            } else {
                format!("is not a part of '{}' — {movable}", sc.thing)
            };
            errs.push(jerr(msg));
            continue;
        };
        if !seen.insert(subject.to_string()) {
            errs.push(jerr("has two `joint` declarations; merge them into one".into()));
            continue;
        }
        out.push(RawJoint {
            part:   subject.to_string(),
            limits: d.ranges.iter().map(|r| (r.qual, r.lo.clone(), r.hi.clone())).collect(),
            span:   d.span,
        });
    }
    out
}

/// Fold the bounds. `label` is the part as the author wrote it.
pub(super) fn fold_joint(raw: &RawJoint, env: &ParamEnv, errs: &mut Vec<MoxiError>) -> ResolvedJoint {
    let jerr = |message: String| MoxiError::JointError { part: raw.part.clone(), message, span: raw.span };
    let mut limits = Vec::new();
    for (q, lo, hi) in &raw.limits {
        let num = |e: &Expr, errs: &mut Vec<MoxiError>| match value::eval(e, env) {
            Ok(Value::Num(v)) => Some(v),
            Ok(other) => { errs.push(jerr(format!("{} bound must be a number, got a {}", q.label(), other.kind()))); None }
            Err(e) => { errs.push(jerr(format!("{} bound: {}", q.label(), e.message))); None }
        };
        let (Some(a), Some(b)) = (num(lo, errs), num(hi, errs)) else { continue };
        if a > b {
            errs.push(jerr(format!("{} range {a}..{b} is reversed — write the smaller bound first", q.label())));
            continue;
        }
        limits.push((*q, a, b));
    }
    ResolvedJoint { part: raw.part.clone(), limits, span: raw.span }
}

/// The number a mate gives a qualifier (folded relations hold literals).
pub fn qual_value(p: &Placement, q: Qual) -> Option<f64> {
    let Placement::Align { twist, pitch, gap, offsets, .. } = p else { return None };
    let e = match q {
        Qual::Twist => twist, Qual::Pitch => pitch, Qual::Gap => gap,
        Qual::ShiftA => &offsets.shift.0, Qual::ShiftB => &offsets.shift.1,
        Qual::LeanA => &offsets.lean.0, Qual::LeanB => &offsets.lean.1,
    };
    match value::eval(e, &HashMap::new()) { Ok(Value::Num(v)) => Some(v), _ => None }
}

fn pose_value(p: &ResolvedPose, part: &str, q: Qual) -> Option<f64> {
    let o = p.overrides.iter().find(|o| o.part == part)?;
    let e = match q {
        Qual::Twist => o.quals.twist.as_ref(), Qual::Pitch => o.quals.pitch.as_ref(), Qual::Gap => o.quals.gap.as_ref(),
        Qual::ShiftA => o.quals.shift.as_ref().map(|s| &s.0), Qual::ShiftB => o.quals.shift.as_ref().map(|s| &s.1),
        Qual::LeanA => o.quals.lean.as_ref().map(|s| &s.0), Qual::LeanB => o.quals.lean.as_ref().map(|s| &s.1),
    }?;
    match value::eval(e, &HashMap::new()) { Ok(Value::Num(v)) => Some(v), _ => None }
}

/// Check every joint against rest (when `check_rest`) and every pose.
pub(super) fn check_joints(
    joints:     &[(ResolvedJoint, bool)],
    relations:  &[Placement],
    poses:      &[ResolvedPose],
    errs:       &mut Vec<MoxiError>,
) {
    let mate_of: HashMap<&str, &Placement> = relations.iter()
        .filter(|p| matches!(p, Placement::Align { .. }))
        .map(|p| (p.subject_name(), p)).collect();
    for (j, check_rest) in joints {
        let Some(mate) = mate_of.get(j.part.as_str()) else { continue };
        for &(q, lo, hi) in &j.limits {
            let rest = qual_value(mate, q);
            if *check_rest {
                if let Some(v) = rest.filter(|v| *v < lo || *v > hi) {
                    errs.push(MoxiError::JointError { part: j.part.clone(), span: j.span, message: format!(
                        "{} is {v} at rest, outside {lo}..{hi} — change the mate or widen the range", q.label()) });
                }
            }
            for p in poses {
                if let Some(v) = pose_value(p, &j.part, q).filter(|v| *v < lo || *v > hi) {
                    errs.push(MoxiError::JointError { part: j.part.clone(), span: p.span, message: format!(
                        "pose '{}' sets {} = {v}, outside {lo}..{hi}", p.name, q.label()) });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::pipeline::compile_to_scene;

    const ARM: &str = r##"
material Iron { color = "#333333" }
thing Wing(top=60) {
    part Root  { shape = sphere(radius=2), material = Iron }
    part Blade { shape = ellipsoid(rx=2, ry=30, rz=6), material = Iron }
    relation { Blade.bottom on Root.top lean=(10, 0) }
    anchor root = Root.center
    joint Blade { lean=(0..top, -5..5) }
    pose Up { Blade lean=(50, 0) }
    resolve voxel_size = 1
}
thing Lamp {
    part Base { shape = cylinder(height=12, radius=45), material = Iron }
    part Post { shape = cylinder(height=300, radius=6), material = Iron }
    part Arm  { shape = capsule(height=90, radius=4), material = Iron }
    part WingR { thing = WINGARGS }
    part WingL { thing = Wing }
    relation {
        Post.bottom on Base.top
        Arm.bottom  on Post.side(t=0.92, angle=0) lean=(20, 0)
        WingR.root on Base.east
        WingL symmetric_across Base from=WingR
    }
    EXTRA
    resolve voxel_size = 3
}
print Lamp
"##;

    fn src(extra: &str) -> String { ARM.replace("EXTRA", extra).replace("WINGARGS", "Wing") }

    fn errors(s: &str) -> String {
        match compile_to_scene(s) {
            Ok(_) => panic!("expected an error"),
            Err(es) => es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"),
        }
    }

    #[test]
    fn ranges_are_checked_at_rest_and_in_every_pose() {
        assert!(compile_to_scene(&src("joint Arm { lean=(-90..120, 0..0) twist=-30..30 }  pose Up { Arm lean=(110, 0) }")).is_ok());
        let m = errors(&src("joint Arm { lean=(-90..120, 0..0) }  pose Smash { Arm lean=(140, 0) }"));
        assert!(m.contains("joint 'Arm': pose 'Smash' sets lean (first value) = 140, outside -90..120"), "{m}");
        let m = errors(&src("joint Arm { lean=(30..120, 0..0) }"));
        assert!(m.contains("joint 'Arm': lean (first value) is 20 at rest, outside 30..120"), "{m}");
        let m = errors(&src("joint Arm { twist=-30..30 }  pose T { Arm twist=45 }"));
        assert!(m.contains("pose 'T' sets twist = 45, outside -30..30"), "{m}");
    }

    /// An instance inherits its thing's joints under its own arguments, so
    /// `WingR pose=Up` is checked against `lean=(0..top, …)` with WingR's top.
    #[test]
    fn instances_inherit_joints_under_their_arguments() {
        assert!(compile_to_scene(&src("pose Flap { WingR pose=Up }")).is_ok());
        let narrow = ARM.replace("EXTRA", "pose Flap { WingR pose=Up }").replace("WINGARGS", "Wing(top=40)");
        let m = errors(&narrow);
        assert!(m.contains("joint 'WingR.Blade': pose 'Flap' sets lean (first value) = 50, outside 0..40"), "{m}");
    }

    #[test]
    fn a_joint_belongs_to_a_mate() {
        let m = errors(&src("joint Base { twist=0..1 }"));
        assert!(m.contains("joint 'Base': is not placed by a mate (it is the root of 'Lamp')")
            && m.contains("Post, Arm, WingR"), "{m}");
        let m = errors(&src("joint WingL { gap=0..1 }"));
        assert!(m.contains("is the mirror image of 'WingR'; give 'WingR' the joint"), "{m}");
        let m = errors(&src("joint WingR.Blade { gap=0..1 }"));
        assert!(m.contains("is placed inside the instance 'WingR' (a 'Wing'); declare the joint in 'Wing' itself"), "{m}");
        let m = errors(&src("joint Armm { gap=0..1 }"));
        assert!(m.contains("joint 'Armm': is not a part of 'Lamp'"), "{m}");
        let m = errors(&src("joint Arm { gap=0..1 }  joint Arm { twist=0..1 }"));
        assert!(m.contains("has two `joint` declarations"), "{m}");
        let m = errors(&src("joint Arm { twist=30..-30 }"));
        assert!(m.contains("twist range 30..-30 is reversed"), "{m}");
    }

    #[test]
    fn limits_and_rest_values_reach_the_scene() {
        let s = compile_to_scene(&src("joint Arm { lean=(-90..120, 0..0) }")).unwrap();
        let arm = s.layers[0].parts.iter().find(|p| p.name == "Arm").unwrap();
        let j = arm.joint.as_ref().unwrap();
        assert_eq!(j.limits["lean.0"], [-90.0, 120.0]);
        assert_eq!(j.rest["lean.0"], 20.0);
        let blade = s.layers[0].parts.iter().find(|p| p.name == "WingR.Blade").unwrap();
        assert_eq!(blade.joint.as_ref().unwrap().limits["lean.0"], [0.0, 60.0], "inherited");
        let post = s.layers[0].parts.iter().find(|p| p.name == "Post").unwrap();
        assert!(post.joint.as_ref().unwrap().limits.is_empty());
    }

    const CHAIN: &str = r##"
material Iron { color = "#333333" }
thing Chain(n=4) {
    part Link[0] { shape = box(width=2, height=10, depth=2), material = Iron }
    for k in 1..n {
        let lim = 10 * k
        part Link[k] { shape = box(width=2, height=10, depth=2), material = Iron }
        relation { Link[k].bottom on Link[k - 1].top lean=(5, 0) }
        joint Link[k] { lean=(-lim..lim, -5..5) }
    }
    EXTRA
    resolve voxel_size = 1
}
print Chain
"##;

    #[test]
    fn joints_unroll_in_loops_with_the_loop_variable_in_their_bounds() {
        let s = compile_to_scene(&CHAIN.replace("EXTRA", "pose Curl { for k in 1..4 { Link[k] lean=(8 * k, 0) } }")).unwrap();
        for k in 1..4 {
            let link = s.layers[0].parts.iter().find(|p| p.name == format!("Link[{k}]")).unwrap();
            let lim = 10.0 * k as f64;
            assert_eq!(link.joint.as_ref().unwrap().limits["lean.0"], [-lim, lim], "Link[{k}]");
        }
    }

    #[test]
    fn an_unrolled_joint_is_named_by_its_index_in_errors() {
        let m = errors(&CHAIN.replace("EXTRA", "pose Curl { for k in 1..4 { Link[k] lean=(25, 0) } }"));
        assert!(m.contains("joint 'Link[1]': pose 'Curl' sets lean (first value) = 25, outside -10..10"), "{m}");
        assert!(m.contains("joint 'Link[2]': pose 'Curl' sets lean (first value) = 25, outside -20..20"), "{m}");
        assert!(!m.contains("Link[3]") && !m.contains("[#"), "{m}");
    }

    #[test]
    fn a_top_level_joint_may_name_an_indexed_part() {
        let plain = CHAIN.replace("        joint Link[k] { lean=(-lim..lim, -5..5) }\n", "");
        let s = compile_to_scene(&plain.replace("EXTRA", "joint Link[n - 1] { twist=-15..15 }")).unwrap();
        let link = s.layers[0].parts.iter().find(|p| p.name == "Link[3]").unwrap();
        assert_eq!(link.joint.as_ref().unwrap().limits["twist"], [-15.0, 15.0]);
        let m = errors(&plain.replace("EXTRA", "joint Link[0] { twist=-15..15 }"));
        assert!(m.contains("joint 'Link[0]'") && !m.contains("[#"), "{m}");
    }
}
