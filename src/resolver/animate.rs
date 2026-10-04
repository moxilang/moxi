//! Animations — living models, step 3 (DOC-20261004-living-models-design §3.2).
//!
//! `animate Name [loop] { t₀ P₀  t₁ P₁ … }` is resolved here into keyframes
//! with folded times and checked pose names. Solving happens per POSE (every
//! pose is already solved on every compile); the pipeline reads each
//! keyframe's joints off those solves and applies the 179° rule, and the
//! glTF exporter turns them into animation channels. Between keyframes
//! nothing is solved: interpolation belongs to the player.

use std::collections::HashSet;

use crate::ast::*;
use crate::error::MoxiError;
use crate::error::Span;
use crate::value::{self, Value};

use super::{subst_expr, ParamEnv, ResolvedPose};

#[derive(Debug, Clone)]
pub struct ResolvedAnimation {
    pub name:   String,
    pub looped: bool,
    pub keys:   Vec<ResolvedKey>,
    pub span:   Span,
}

#[derive(Debug, Clone)]
pub struct ResolvedKey {
    /// Seconds from the start.
    pub time: f64,
    /// A pose of the thing, or `rest`.
    pub pose: String,
    pub span: Span,
}

/// An animation before its times are folded: kept on the template so an
/// instance replays it under its own arguments, prefixed (`Left.Wave`, keys
/// on the instance's poses `Left.Up`).
#[derive(Debug, Clone)]
pub(super) struct RawAnim {
    pub name:   String,
    pub looped: bool,
    /// (time, pose or `rest`, span)
    pub keys:   Vec<(Expr, String, Span)>,
    pub span:   Span,
}

impl RawAnim {
    pub(super) fn subst(&self, env: &ParamEnv) -> RawAnim {
        RawAnim { keys: self.keys.iter().map(|(t, p, s)| (subst_expr(t, env), p.clone(), *s)).collect(), ..self.clone() }
    }
    pub(super) fn prefixed(&self, prefix: &str) -> RawAnim {
        RawAnim {
            name: format!("{prefix}.{}", self.name),
            keys: self.keys.iter().map(|(t, p, s)| {
                let pose = if p == "rest" { p.clone() } else { format!("{prefix}.{p}") };
                (t.clone(), pose, *s)
            }).collect(),
            ..self.clone()
        }
    }
}

/// Raw forms of this thing's own animations that resolved cleanly.
pub(super) fn raw_of(decls: &[AnimateDecl], resolved: &[ResolvedAnimation]) -> Vec<RawAnim> {
    resolved.iter().filter_map(|r| {
        let d = decls.iter().find(|d| d.name.name == r.name)?;
        Some(RawAnim {
            name: r.name.clone(), looped: r.looped, span: r.span,
            keys: d.keys.iter().map(|k| (k.time.clone(), k.pose.name.clone(), k.span)).collect(),
        })
    }).collect()
}

/// Fold an inherited animation's times (the poses exist by construction:
/// they are the instance's poses, promoted into the outer thing).
pub(super) fn fold_raw(raw: &RawAnim, env: &ParamEnv, errs: &mut Vec<MoxiError>) -> Option<ResolvedAnimation> {
    let aerr = |message: String, span: Span| MoxiError::AnimationError { animation: raw.name.clone(), message, span };
    let mut keys: Vec<ResolvedKey> = Vec::new();
    for (t, pose, span) in &raw.keys {
        let time = match value::eval(t, env) {
            Ok(Value::Num(v)) => v,
            Ok(other) => { errs.push(aerr(format!("a keyframe time must be a number of seconds, got a {}", other.kind()), *span)); return None; }
            Err(e) => { errs.push(aerr(format!("keyframe time: {}", e.message), *span)); return None; }
        };
        if let Some(prev) = keys.last() {
            if time <= prev.time {
                errs.push(aerr(format!("keyframe times must increase: {time} s comes after {} s \
                                        (under this instance's arguments)", prev.time), *span));
                return None;
            }
        }
        keys.push(ResolvedKey { time, pose: pose.clone(), span: *span });
    }
    Some(ResolvedAnimation { name: raw.name.clone(), looped: raw.looped, keys, span: raw.span })
}

pub(super) fn walk_animation_exprs(a: &mut AnimateDecl, f: &mut dyn FnMut(&Expr) -> Expr) {
    for k in &mut a.keys {
        k.time = f(&k.time);
    }
}

pub(super) fn resolve_animations(
    thing: &str,
    decls: &[AnimateDecl],
    poses: &[ResolvedPose],
    env:   &ParamEnv,
    errs:  &mut Vec<MoxiError>,
) -> Vec<ResolvedAnimation> {
    let mut seen = HashSet::new();
    let pose_list = || {
        if poses.is_empty() {
            format!("'{thing}' declares no poses (only `rest`)")
        } else {
            format!("poses of '{thing}': {} (or `rest`)",
                poses.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "))
        }
    };
    let mut out = Vec::new();
    for a in decls {
        let name = a.name.name.clone();
        let aerr = |message: String, span: Span| MoxiError::AnimationError {
            animation: name.clone(), message, span,
        };
        if !seen.insert(name.clone()) {
            errs.push(MoxiError::DuplicateName { name: name.clone(), span: a.name.span });
            continue;
        }
        if a.keys.len() < 2 {
            errs.push(aerr("needs at least two keyframes, e.g. `0 rest  0.5 Up`".into(), a.span));
            continue;
        }
        let mut keys: Vec<ResolvedKey> = Vec::new();
        for k in &a.keys {
            let time = match value::eval(&k.time, env) {
                Ok(Value::Num(t)) => t,
                Ok(other) => {
                    errs.push(aerr(format!("a keyframe time must be a number of seconds, got a {}",
                        other.kind()), k.span));
                    continue;
                }
                Err(e) => { errs.push(aerr(format!("keyframe time: {}", e.message), k.span)); continue; }
            };
            if k.pose.name != "rest" && !poses.iter().any(|p| p.name == k.pose.name) {
                errs.push(aerr(format!("no pose '{}' — {}", k.pose.name, pose_list()), k.pose.span));
                continue;
            }
            match keys.last() {
                None if time != 0.0 => {
                    errs.push(aerr(format!("the first keyframe must be at 0, got {time}"), k.span));
                }
                Some(prev) if time <= prev.time => {
                    errs.push(aerr(format!(
                        "keyframe times must increase: {time} s comes after {} s", prev.time), k.span));
                }
                _ => {}
            }
            keys.push(ResolvedKey { time, pose: k.pose.name.clone(), span: k.span });
        }
        out.push(ResolvedAnimation { name, looped: a.looped, keys, span: a.span });
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::pipeline::compile_to_scene;

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
    pose Up   { Arm lean=(70, 0) }
    pose Far  { Arm lean=(220, 0) }
    pose Spun { Arm twist=170 }
    pose Odd  { Arm twist=178 pitch=178 }
    ANIM
    resolve voxel_size = 3
}
print Lamp
"##;

    fn errors(anim: &str) -> String {
        match compile_to_scene(&LAMP.replace("ANIM", anim)) {
            Ok(_) => panic!("expected an error for {anim}"),
            Err(es) => es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"),
        }
    }

    #[test]
    fn keyframes_land_in_the_scene() {
        let scene = compile_to_scene(&LAMP.replace("ANIM", "animate Wave loop { 0 rest  0.5 Up  1 rest }")).unwrap();
        let a = &scene.layers[0].animations;
        assert_eq!(a.len(), 1);
        assert!(a[0].looped);
        assert_eq!(a[0].keys.iter().map(|k| (k.time, k.pose.as_str())).collect::<Vec<_>>(),
                   vec![(0.0, "rest"), (0.5, "Up"), (1.0, "rest")]);
        assert_eq!(a[0].keys[1].joints.len(), 4, "every part has a joint frame per keyframe");
    }

    #[test]
    fn times_start_at_zero_and_increase() {
        assert!(errors("animate A { 0.1 rest  0.5 Up }").contains("the first keyframe must be at 0, got 0.1"));
        let m = errors("animate A { 0 rest  0.5 Up  0.5 rest }");
        assert!(m.contains("animation 'A'") && m.contains("keyframe times must increase: 0.5 s comes after 0.5 s"), "{m}");
        assert!(errors("animate A { 0 rest }").contains("needs at least two keyframes"));
    }

    #[test]
    fn keyframes_name_existing_poses() {
        let m = errors("animate A { 0 rest  0.5 Upp }");
        assert!(m.contains("no pose 'Upp' — poses of 'Lamp': Up, Far, Spun, Odd (or `rest`)"), "{m}");
        assert!(errors("animate A { 0 rest  1 Up }  animate A { 0 rest  1 Up }").contains("'A' is already defined"));
    }

    /// The 179° rule on written qualifiers: 20 → 220 is a 200° turn that a
    /// rotation matrix alone would read as −160°.
    #[test]
    fn no_joint_turns_more_than_179_degrees_between_keyframes() {
        let m = errors("animate Swing { 0 rest  0.4 Far }");
        assert!(m.contains("animation 'Swing'") && m.contains("'Arm' lean (first angle) turns 200° between 0 s and 0.4 s (rest → Far)")
            && m.contains("insert a keyframe"), "{m}");
        // the same turn split by a keyframe is fine
        assert!(compile_to_scene(&LAMP.replace("ANIM", "animate Swing { 0 rest  0.2 Up  0.4 Far }")).is_ok());
    }

    /// The rule on the joint itself: twist 178 and pitch 178 are each under
    /// the limit, but together they turn the arm a half turn — which slerp
    /// could play either way.
    #[test]
    fn combined_half_turns_of_a_joint_are_caught() {
        assert!(compile_to_scene(&LAMP.replace("ANIM", "animate S { 0 rest  1 Spun }")).is_ok());
        let m = errors("animate O { 0 rest  1 Odd }");
        assert!(m.contains("'Arm' turns 180.0° between 0 s and 1 s (rest → Odd)"), "{m}");
    }
}
