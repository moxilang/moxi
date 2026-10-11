// src/frame_resolver.rs
//
// Replaces relation_resolver.rs.
//
// Pipeline position: after semantic resolution, BEFORE any stamping.
// Input:  parts (name + shape) and placement statements.
// Output: a world Frame per part, in continuous world units.
//
// The old resolver ran 4 blind fixed-point passes over voxel bounding
// boxes and silently produced wrong offsets for chains longer than 4.
// This one builds the dependency graph explicitly, topologically sorts
// it, and solves each frame exactly once — cycles and double-placements
// are hard errors with spans, per strict-mode-default.
//
// Everything here is analytic: anchors and extents come from shape
// parameters, never from grids. Voxelization is a separate realization
// step (`realize`) at the very end — the only voxel-aware code. A future
// mesh/simplex backend replaces `realize` and nothing else.

use std::collections::HashMap;

use crate::anchors::{analytic_extents, resolve_anchor, Anchor, AnchorKind};
use crate::ast::{AimTarget, AnchorRef, Placement, RelationKind, RelationStmt, ShapeExpr};
use crate::error::Span;
use crate::frame::{snap_axis_aligned, Frame, Mat3, Vec3};
use crate::geom::Axis;

// ── Placement statements ───────────────────────────────────────────────────
//
// The placement AST (Align / Mirror / AnchorRef) lives in ast/mod.rs now —
// it IS the language: the 13 relation keywords are desugared into it at
// parse time (parser::desugar_placement). This module only solves it.

// ── Errors ─────────────────────────────────────────────────────────────────
//
// These become new MoxiError variants; the enum patch is in MIGRATION.md.
// Kept local here so the draft is self-contained.

#[derive(Debug, Clone)]
pub enum PlacementError {
    /// A part is the subject of more than one placement. Strict mode:
    /// error now; a relaxing constraint solver can lift this later.
    OverConstrained { part: String, first: Span, second: Span },
    /// The placement graph contains a cycle; `chain` is the full loop
    /// for the error message: "Skull → Spine → Pelvis → Skull".
    CyclicPlacement { chain: Vec<String> },
    /// Unknown part name in a placement (resolver should catch earlier;
    /// defense in depth here).
    UnknownPart { part: String, span: Span },
    /// Anchor errors from anchors.rs, with placement context.
    Anchor { part: String, message: String, span: Span },
    /// Phase-1 realization: the solved rotation is not one of the 24
    /// axis-aligned orientations the voxel stamper can realize exactly.
    /// Names the capability instead of silently resampling.
    NonAxisAlignedRotation { part: String },
    /// A declared constraint was violated, with expected/actual content.
    ConstraintViolation { description: String, span: Span },
    /// `aim=` at a part that hangs off the aiming part (or is it): the
    /// target cannot be solved before the subject. `via` is the dependency
    /// path from the target back to the subject, one (from, kind, to) per
    /// edge — "HandR hangs off ArmR".
    /// `verb` is "aims at" or "faces".
    AimCycle { subject: String, verb: &'static str, target: String, via: Vec<(String, &'static str, String)>, span: Span },
}

impl std::fmt::Display for PlacementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlacementError::OverConstrained { part, first, second } =>
                write!(f, "part '{part}' is placed twice (at {first} and {second}); \
                           each part may be the subject of at most one placement"),
            PlacementError::CyclicPlacement { chain } =>
                write!(f, "placement cycle: {}", chain.join(" → ")),
            PlacementError::UnknownPart { part, span } =>
                write!(f, "[{span}] unknown part '{part}' in placement"),
            PlacementError::Anchor { part, message, span } =>
                write!(f, "[{span}] on part '{part}': {message}"),
            PlacementError::NonAxisAlignedRotation { part } =>
                write!(f, "part '{part}' resolved to a rotation the voxel backend \
                           cannot yet realize exactly (only the 24 axis-aligned \
                           orientations are supported in this phase)"),
            PlacementError::ConstraintViolation { description, span } =>
                write!(f, "[{span}] constraint violated: {description}"),
            PlacementError::AimCycle { subject, verb, target, via, span } => {
                if via.is_empty() {
                    return write!(f, "[{span}] '{subject}' {verb} {target}, which is on '{subject}' \
                                      itself — name another part, a point `[x, y, z]` or a \
                                      direction (north, south, east, west, up, down)");
                }
                let steps: Vec<String> = via.iter().enumerate().map(|(i, (from, kind, to))| {
                    if i == 0 { format!("{from} {kind} {to}") } else { format!("which {kind} {to}") }
                }).collect();
                let mut chain: Vec<&str> = via.iter().map(|(from, _, _)| from.as_str()).collect();
                chain.push(subject);
                chain.reverse();
                write!(f, "[{span}] '{subject}' {verb} {target}, but {} ({}) — a part cannot aim \
                           at or face anything that hangs off it; name another part, a point \
                           `[x, y, z]` or a direction", steps.join(", "), chain.join(" → "))
            }
        }
    }
}

// ── The solver ─────────────────────────────────────────────────────────────

pub type FrameMap = HashMap<String, Frame>;

/// Solve world frames for all parts of one entity.
///
/// Rules:
///   1. Each part is the subject of at most one placement (else error).
///   2. Parts with no placement are roots: Frame::IDENTITY.
///   3. Kahn's toposort over the dependency graph; each frame is solved
///      exactly once, in order. Leftover nodes ⇒ cycle ⇒ error with chain.
pub fn resolve_frames(
    parts:      &[(String, ShapeExpr)],
    placements: &[Placement],
) -> Result<FrameMap, Vec<PlacementError>> {
    resolve_frames_full(parts, placements).map(|(frames, _)| frames)
}

/// Local reflection normals of parts placed by `symmetric_across`.
pub type ReflectMap = HashMap<String, Vec3>;

/// Frames, plus the LOCAL reflection each mirrored part's geometry needs.
///
/// A mirror's true pose M·R is improper; the solver keeps frames proper by
/// computing R' = M·R·M. Since M·R = (M·R·M)·M, the difference is a local
/// reflection by the same matrix — numerically, the world normal n. So a
/// mirrored part's geometry is `mirror(shape, n)` placed at R'. Before this
/// the local factor was dropped: right only for parts symmetric under it
/// (capsules, spheres), silently wrong for anything chiral.
///
/// `resolve_frames` keeps its old signature for callers that never realize
/// geometry (the resolver's instance compass anchors).
pub fn resolve_frames_full(
    parts:      &[(String, ShapeExpr)],
    placements: &[Placement],
) -> Result<(FrameMap, ReflectMap), Vec<PlacementError>> {
    resolve_frames_noted(parts, placements).map(|(f, r, _)| (f, r))
}

/// A remark about a solved scene that is not an error (DOC-20261011 §2): an
/// `aim` that turns its socket normal near a half turn with no `facing` —
/// there the roll swings with the target.
#[derive(Debug, Clone, PartialEq)]
pub struct SolveNote {
    pub part:    String,
    pub message: String,
}

/// Turns larger than this, with no `facing`, get a note.
pub const AIM_ROLL_NOTE_DEG: f64 = 150.0;

/// As `resolve_frames_full`, plus notes. The frames are identical: notes are
/// read off the solve, never fed back into it.
pub fn resolve_frames_noted(
    parts:      &[(String, ShapeExpr)],
    placements: &[Placement],
) -> Result<(FrameMap, ReflectMap, Vec<SolveNote>), Vec<PlacementError>> {
    let mut errors: Vec<PlacementError> = Vec::new();
    let mut notes: Vec<SolveNote> = Vec::new();

    let shape_of: HashMap<&str, &ShapeExpr> =
        parts.iter().map(|(n, s)| (n.as_str(), s)).collect();

    // Rule 1: one placement per subject.
    let mut placement_of: HashMap<&str, &Placement> = HashMap::new();
    for p in placements {
        let subj = p.subject_name();
        if !shape_of.contains_key(subj) {
            errors.push(PlacementError::UnknownPart { part: subj.to_string(), span: p.span() });
            continue;
        }
        if !shape_of.contains_key(p.object_name()) {
            errors.push(PlacementError::UnknownPart {
                part: p.object_name().to_string(), span: p.span(),
            });
            continue;
        }
        if let Some(r) = target_anchors(p).into_iter().map(|(_, r)| r)
            .find(|r| !shape_of.contains_key(r.part.as_str()))
        {
            errors.push(PlacementError::UnknownPart { part: r.part.clone(), span: r.span });
            continue;
        }
        if let Some(first) = placement_of.insert(subj, p) {
            errors.push(PlacementError::OverConstrained {
                part:   subj.to_string(),
                first:  first.span(),
                second: p.span(),
            });
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    // Dependency edges: subject depends on object (and, for Mirror, on the
    // plane's part too — the reflection needs both solved).
    let deps = |name: &str| -> Vec<String> {
        match placement_of.get(name) {
            None => Vec::new(),
            Some(p @ Placement::Align { object, .. }) => {
                let mut d = vec![object.part.clone()];
                for (_, r) in target_anchors(p) {
                    if !d.contains(&r.part) {
                        d.push(r.part.clone());
                    }
                }
                d
            }
            Some(Placement::Mirror { source, plane, .. }) => {
                let mut d = vec![source.clone()];
                if plane.part != *source {
                    d.push(plane.part.clone());
                }
                d
            }
        }
    };

    // The root part of every instance an `aim` was written in: its frame
    // carries that thing's space (the root sits at the identity there).
    let mut space_roots: HashMap<String, String> = HashMap::new();
    for p in placements {
        let Placement::Align { offsets, .. } = p else { continue };
        for space in [&offsets.aim, &offsets.facing].into_iter().flatten().filter_map(|a| a.space.as_ref()) {
        if space_roots.contains_key(space) {
            continue;
        }
        let prefix = format!("{space}.");
        let root = parts.iter().map(|(n, _)| n.as_str()).find(|n| {
            n.starts_with(&prefix) && match placement_of.get(n) {
                None => true,
                Some(Placement::Align { object, .. }) => !object.part.starts_with(&prefix),
                Some(Placement::Mirror { source, .. }) => !source.starts_with(&prefix),
            }
        });
        if let Some(root) = root {
            space_roots.insert(space.clone(), root.to_string());
        }
        }
    }

    // Kahn's algorithm.
    let mut in_deg: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();
    for (name, _) in parts {
        let d = deps(name);
        in_deg.insert(name.as_str(), d.len());
        for dep in d {
            // dep must be solved before name
            let dep_key = shape_of.keys().find(|k| **k == dep).copied();
            if let Some(dep_key) = dep_key {
                dependents.entry(dep_key).or_default().push(name.as_str());
            }
        }
    }

    let mut queue: Vec<&str> = in_deg.iter()
        .filter(|(_, &d)| d == 0)
        .map(|(k, _)| *k)
        .collect();
    queue.sort(); // deterministic order

    let mut frames: FrameMap = HashMap::new();
    let mut reflects: ReflectMap = HashMap::new();
    let mut solved_count = 0usize;

    while let Some(name) = queue.pop() {
        solved_count += 1;

        let frame = match placement_of.get(name) {
            None => Frame::IDENTITY, // Rule 2: root
            Some(p) => match solve_one(p, &shape_of, &frames, &reflects, &space_roots) {
                Ok((f, reflect, aim_turn)) => {
                    if let Some(n) = reflect {
                        reflects.insert(name.to_string(), n);
                    }
                    if let Some(deg) = aim_turn.filter(|d| *d > AIM_ROLL_NOTE_DEG) {
                        notes.push(SolveNote {
                            part:    name.to_string(),
                            message: format!(
                                "'{name}' aims {deg:.2}° away from its socket's normal with no `facing`: \
                                 near a half turn its roll swings with the smallest move of the target — \
                                 add `facing=` to fix which way it rolls"),
                        });
                    }
                    f
                }
                Err(e) => { errors.push(e); Frame::IDENTITY }
            },
        };
        frames.insert(name.to_string(), frame);

        if let Some(next) = dependents.get(name) {
            for &n in next {
                let d = in_deg.get_mut(n).unwrap();
                *d -= 1;
                if *d == 0 {
                    queue.push(n);
                }
            }
        }
    }

    // Rule 3: leftovers form a cycle. An aim at something that hangs off
    // the aiming part is reported as that, with the path; any other cycle
    // walks the mates, as before.
    if solved_count < parts.len() {
        if let Some(e) = aim_cycle(parts, &placement_of, &frames) {
            errors.push(e);
            return Err(errors);
        }
        let mut in_cycle: Vec<&str> = parts.iter()
            .map(|(n, _)| n.as_str())
            .filter(|n| !frames.contains_key(*n))
            .collect();
        in_cycle.sort();
        if let Some(&start) = in_cycle.first() {
            let mut chain = vec![start.to_string()];
            let mut cur = start;
            loop {
                let next = deps(cur).into_iter().next().unwrap_or_default();
                chain.push(next.clone());
                if next == start || chain.len() > parts.len() + 1 {
                    break;
                }
                // hold ownership for the borrow below
                let leaked: &str = Box::leak(next.into_boxed_str());
                cur = leaked;
            }
            errors.push(PlacementError::CyclicPlacement { chain });
        }
    }

    if errors.is_empty() { Ok((frames, reflects, notes)) } else { Err(errors) }
}

/// The mate formula — the ONE place placement math exists.
///
/// Oriented sockets:
///   T_subj = W ∘ Trans(0, gap, 0) ∘ RotY(twist) ∘ RotX(pitch) ∘ FLIP ∘ A_subj⁻¹
/// where W = T_obj ∘ A_obj is the socket's world frame and FLIP is 180°
/// about X (normals oppose, tangents align X-to-X at twist = pitch = 0).
///
/// Orientation-free sockets (center): no flip, no twist/pitch — the
/// subject INHERITS the object part's rotation and is positioned so the
/// two anchor points coincide (plus gap along the object's world +Y).
/// Solve one placement. Returns the subject's frame and, for a mirror, the
/// local reflection normal its geometry needs (see `resolve_frames_full`).
fn solve_one(
    placement: &Placement,
    shape_of:  &HashMap<&str, &ShapeExpr>,
    frames:    &FrameMap,
    reflects:  &ReflectMap,
    spaces:    &HashMap<String, String>,
) -> Result<(Frame, Option<Vec3>, Option<f64>), PlacementError> {
    match placement {
        Placement::Align { subject, object, twist, pitch, gap, offsets, span } => {
            // Qualifiers arrive as expressions but are folded by the
            // resolver, so every one is a literal here. A non-literal
            // means an unresolved name slipped past `check_no_free_idents`
            // — treat it as zero rather than panicking; the resolver has
            // already reported it.
            let num = |e: &crate::ast::Expr| match e {
                crate::ast::Expr::Float(f) => *f,
                crate::ast::Expr::Int(n)   => *n as f64,
                _ => 0.0,
            };
            let (twist, pitch, gap) = (num(twist), num(pitch), num(gap));
            let shift = (num(&offsets.shift.0), num(&offsets.shift.1));
            let lean  = (num(&offsets.lean.0),  num(&offsets.lean.1));

            let a_subj = lookup_anchor(subject, shape_of, reflects, *span)?;
            let a_obj  = lookup_anchor(object, shape_of, reflects, *span)?;

            let t_obj = frames.get(&object.part).copied().unwrap_or(Frame::IDENTITY);
            let w     = t_obj.compose(&a_obj.frame); // socket in world space

            let free = a_subj.kind == AnchorKind::Free || a_obj.kind == AnchorKind::Free;

            // `shift` and `gap` are one translation in SOCKET-LOCAL space:
            // (X, Y, Z) = (shift.0, gap, shift.1). Y is the outward normal,
            // X and Z span the tangent plane. It sits leftmost in the
            // adjust chain so it is applied in the socket's own frame and
            // is NOT re-rotated by twist or pitch — a feature keeps its
            // place on the surface no matter how it is aimed.
            //
            // On a FLAT face the second component must mean the same named
            // direction on every face, or a symmetric pair hides the bug
            // and an asymmetric part comes out mirrored (feet pointing
            // backward). See `flat_face_sign`.
            let b_sign = flat_face_sign(&object.anchor, &a_obj);
            let shift = (shift.0, shift.1 * b_sign);
            let slide = Vec3::new(shift.0, gap, shift.1);

            // `lean=(a, b)` tips the part's axis — the socket normal after
            // the mate — toward the face's `a` direction by `a` degrees, then
            // toward `b` by `b` degrees: the same named directions `shift`
            // uses. On a trunk's `side`, `a` is up, so `lean=(45, 0)` is 45°
            // upward. Rotating about the socket's own tangent axes, right
            // after the slide, means it never sweeps the part around a
            // curved shape (what `pitch` does on `side`) and never rolls it
            // (what `twist=-90 pitch=…` does to anything but a capsule).
            // Axis Y toward +X is a rotation about Z by −a; toward +Z is a
            // rotation about X by +b.
            let lean_rot = Mat3::rot_z(-lean.0.to_radians())
                .mul(&Mat3::rot_x((lean.1 * b_sign).to_radians()));

            if offsets.aim.is_some() || offsets.facing.is_some() {
                if free {
                    let qual = if offsets.aim.is_some() { "aim" } else { "facing" };
                    return Err(PlacementError::Anchor {
                        part:    subject.part.clone(),
                        message: format!(
                            "`{qual}` needs an oriented anchor on both sides; '{}.{}' or '{}.{}' is \
                             orientation-free (like `center`), so there is no axis to {} — \
                             mate a face such as `bottom`, `side(...)` or `surface(...)`",
                            subject.part, subject.anchor, object.part, object.anchor,
                            if qual == "aim" { "aim" } else { "turn about" }),
                        span: *span,
                    });
                }
                // The faced, aimed rest (DOC-20261007-aim-facing-report §3–§4).
                // The slide stays in the ORIGINAL socket frame, so the part
                // stays where `shift`/`gap` put it.
                let p = w.pos.add(w.rot.apply(slide));
                let mut rot = w.rot;
                // How far the aim turned the socket normal (degrees), for the
                // near-half-turn note; read off, never used in the solve.
                let mut aim_turn: Option<f64> = None;
                // aim: pre-multiply by the shortest turn of the socket
                // normal n onto d.
                if let Some(aim) = &offsets.aim {
                    let d = target_direction(aim, &subject.part, p, shape_of, frames, reflects, spaces, *span)?;
                    let n = w.rot.col(1);
                    if offsets.facing.is_none() {
                        aim_turn = Some(n.dot(d).clamp(-1.0, 1.0).acos().to_degrees());
                    }
                    let turn = shortest_turn(n, d).ok_or_else(|| PlacementError::Anchor {
                        part:    subject.part.clone(),
                        message: format!(
                            "`aim` points straight back into its own socket on '{}.{}' — the half \
                             turn has no shortest way; aim off the socket's normal, or use `lean`",
                            object.part, object.anchor),
                        span: *span,
                    })?;
                    rot = turn.mul(&rot);
                }
                // facing: then swivel about the (aimed) axis until the
                // part's front AT REST — every offset zero — points as
                // close as possible to f.
                if let Some(facing) = &offsets.facing {
                    let axis = rot.col(1);
                    let f = target_direction(facing, &subject.part, p, shape_of, frames, reflects, spaces, *span)?;
                    let f_across = f.sub(axis.scale(f.dot(axis)));
                    if f_across.length() < 1e-6 {
                        return Err(PlacementError::Anchor {
                            part:    subject.part.clone(),
                            message: format!(
                                "faces {}, which is its own axis; name a direction across it \
                                 (north, east, …)", target_words(facing)),
                            span: *span,
                        });
                    }
                    let front = rot.mul(&Mat3::FLIP_X).mul(&a_subj.frame.rot.transpose())
                        .apply(part_front(&a_subj));
                    let front_across = front.sub(axis.scale(front.dot(axis)));
                    let angle = axis.dot(front_across.cross(f_across)).atan2(front_across.dot(f_across));
                    rot = turn_about(axis, angle).mul(&rot);
                }
                let rest = Frame::new(rot, p);
                let adjust = Frame::from_rot(lean_rot)
                    .compose(&Frame::from_rot(Mat3::rot_y(twist.to_radians())))
                    .compose(&Frame::from_rot(Mat3::rot_x(pitch.to_radians())))
                    .compose(&Frame::from_rot(Mat3::FLIP_X));
                return Ok((rest.compose(&adjust).compose(&a_subj.frame.inverse()), None, aim_turn));
            }

            if free {
                // A `center` mate has no direction to lean from.
                if lean.0 != 0.0 || lean.1 != 0.0 {
                    return Err(PlacementError::Anchor {
                        part:    subject.part.clone(),
                        message: format!(
                            "`lean` needs an oriented anchor on both sides; '{}.{}' or '{}.{}' is \
                             orientation-free (like `center`), so there is no direction to lean \
                             from — mate a face such as `bottom`, `side(...)` or `surface(...)`",
                            subject.part, subject.anchor, object.part, object.anchor),
                        span: *span,
                    });
                }
                // Inherit object rotation; coincide anchor points.
                let rot = t_obj.rot;
                let target = w.pos.add(w.rot.apply(slide));
                let pos = target.sub(rot.apply(a_subj.frame.pos));
                Ok((Frame::new(rot, pos), None, None))
            } else {
                let adjust = Frame::from_pos(slide)
                    .compose(&Frame::from_rot(lean_rot))
                    .compose(&Frame::from_rot(Mat3::rot_y(twist.to_radians())))
                    .compose(&Frame::from_rot(Mat3::rot_x(pitch.to_radians())))
                    .compose(&Frame::from_rot(Mat3::FLIP_X));
                Ok((w.compose(&adjust).compose(&a_subj.frame.inverse()), None, None))
            }
        }

        Placement::Mirror { source, plane, axis, span, .. } => {
            if reflects.contains_key(source) {
                return Err(PlacementError::Anchor {
                    part:    source.clone(),
                    message: "is itself a mirror image; mirroring a mirror image is not \
                              supported yet — mirror the original part instead".to_string(),
                    span:    *span,
                });
            }
            let src = frames.get(source).copied().ok_or_else(|| PlacementError::UnknownPart {
                part: source.clone(), span: *span,
            })?;
            let a_plane = lookup_anchor(plane, shape_of, reflects, *span)?;
            let t_plane = frames.get(&plane.part).copied().unwrap_or(Frame::IDENTITY);
            let w       = t_plane.compose(&a_plane.frame);

            // Mirror plane: through the anchor point, normal = `axis` in the
            // plane PART's local space (so a rotated spine mirrors its limbs
            // correctly). Default X = bilateral symmetry — v1 used the
            // anchor's +Y here, which mirrored top-to-bottom. Wrong.
            let axis_vec = match axis {
                Axis::X => Vec3::X,
                Axis::Y => Vec3::Y,
                Axis::Z => Vec3::Z,
            };
            let n  = t_plane.rot.apply(axis_vec).normalize().unwrap_or(Vec3::X);
            let m  = reflection_matrix(n);
            let p  = m.apply(src.pos.sub(w.pos)).add(w.pos);
            // Sandwich: M·R·M is a proper rotation (det M² = +1). The local
            // factor M it drops is returned, and becomes the part's
            // `mirror` wrapper — the geometry is a mirror IMAGE.
            let r  = m.mul(&src.rot).mul(&m);
            Ok((Frame::new(r, p), Some(n), None))
        }
    }
}

/// The resolved anchors an `Align` aims at or faces, with the verb for
/// messages.
fn target_anchors(p: &Placement) -> Vec<(&'static str, &AnchorRef)> {
    let Placement::Align { offsets, .. } = p else { return Vec::new() };
    [("aims at", &offsets.aim), ("faces", &offsets.facing)].into_iter()
        .filter_map(|(verb, t)| match t.as_ref().map(|a| &a.target) {
            Some(AimTarget::Anchor(r)) => Some((verb, r)),
            _ => None,
        })
        .collect()
}

/// A part's FRONT, in its own space: its `north` (+Z) — unless it is mated
/// by its `north` or `south` (subject anchor normal within 45° of ±Z),
/// where north is (nearly) its axis and the front is its `top` (+Y).
fn part_front(a_subj: &Anchor) -> Vec3 {
    let m = a_subj.frame.rot.col(1);
    if m.z.abs() > std::f64::consts::FRAC_1_SQRT_2 { Vec3::Y } else { Vec3::Z }
}

/// How a target reads in a message: `up`, `[0, 5, 0]`, `Angel.center`.
fn target_words(t: &crate::ast::Aim) -> String {
    let num = |e: &crate::ast::Expr| match e {
        crate::ast::Expr::Float(f) => *f,
        crate::ast::Expr::Int(n)   => *n as f64,
        _ => 0.0,
    };
    match &t.target {
        AimTarget::Dir(d) => d.word().to_string(),
        AimTarget::Point(x, y, z) => format!("[{}, {}, {}]", num(x), num(y), num(z)),
        AimTarget::Anchor(r) if r.anchor == "point" => r.part.clone(),
        AimTarget::Anchor(r) => format!("{}.{}", r.part, r.anchor),
        AimTarget::Path { segs, .. } => segs.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join("."),
    }
}

/// Rotation by `angle` (radians) about the unit `axis` (Rodrigues).
fn turn_about(axis: Vec3, angle: f64) -> Mat3 {
    let (s, c) = angle.sin_cos();
    let k = [axis.x, axis.y, axis.z];
    let mut m = [[0.0f64; 3]; 3];
    for (r, row) in m.iter_mut().enumerate() {
        for (col, x) in row.iter_mut().enumerate() {
            let cross = match (r, col) {
                (0, 1) => -k[2], (0, 2) => k[1], (1, 0) => k[2],
                (1, 2) => -k[0], (2, 0) => -k[1], (2, 1) => k[0], _ => 0.0,
            };
            *x = if r == col { c } else { 0.0 } + s * cross + (1.0 - c) * k[r] * k[col];
        }
    }
    Mat3(m)
}

/// Unit direction from the socket point `p` toward an aim or facing
/// target, in the space being solved. Directions and points written inside
/// an instance are carried by that instance's root frame.
#[allow(clippy::too_many_arguments)]
fn target_direction(
    aim:      &crate::ast::Aim,
    subject:  &str,
    p:        Vec3,
    shape_of: &HashMap<&str, &ShapeExpr>,
    frames:   &FrameMap,
    reflects: &ReflectMap,
    spaces:   &HashMap<String, String>,
    span:     Span,
) -> Result<Vec3, PlacementError> {
    let num = |e: &crate::ast::Expr| match e {
        crate::ast::Expr::Float(f) => *f,
        crate::ast::Expr::Int(n)   => *n as f64,
        _ => 0.0,
    };
    let space = aim.space.as_ref()
        .and_then(|s| spaces.get(s))
        .and_then(|root| frames.get(root))
        .copied()
        .unwrap_or(Frame::IDENTITY);
    let (to, what) = match &aim.target {
        AimTarget::Dir(dir) => {
            let [x, y, z] = dir.vector();
            return Ok(space.apply_dir(Vec3::new(x, y, z)));
        }
        AimTarget::Point(x, y, z) => {
            let q = Vec3::new(num(x), num(y), num(z));
            (space.apply_point(q), format!("[{}, {}, {}]", q.x, q.y, q.z))
        }
        AimTarget::Anchor(r) => {
            let a = lookup_anchor(r, shape_of, reflects, span)?;
            let t = frames.get(&r.part).copied().unwrap_or(Frame::IDENTITY);
            let what = if r.anchor == "point" { r.part.clone() } else { format!("{}.{}", r.part, r.anchor) };
            (t.apply_point(a.frame.pos), what)
        }
        // The resolver turns every path into an anchor; one left here was
        // already reported.
        AimTarget::Path { .. } => return Err(PlacementError::Anchor {
            part: subject.to_string(), message: "unresolved aim target".into(), span,
        }),
    };
    to.sub(p).normalize().ok_or_else(|| PlacementError::Anchor {
        part:    subject.to_string(),
        message: format!(
            "{what} gives no direction: the target is the socket point itself — \
             name a point off the socket"),
        span,
    })
}

/// The rotation turning unit `n` onto unit `d` the shortest way — about
/// n × d by ∠(n, d) (Rodrigues: I + K + K²/(1 + n·d), K = [n × d]ₓ).
/// `None` when d = −n: a half turn has no unique axis.
fn shortest_turn(n: Vec3, d: Vec3) -> Option<Mat3> {
    let c = n.dot(d);
    if c < -1.0 + 1e-9 {
        return None;
    }
    let v = n.cross(d);
    let k = [[0.0, -v.z, v.y], [v.z, 0.0, -v.x], [-v.y, v.x, 0.0]];
    let mut m = [[0.0f64; 3]; 3];
    for (r, row) in m.iter_mut().enumerate() {
        for (col, x) in row.iter_mut().enumerate() {
            let k2: f64 = (0..3).map(|i| k[r][i] * k[i][col]).sum();
            *x = if r == col { 1.0 } else { 0.0 } + k[r][col] + k2 / (1.0 + c);
        }
    }
    Some(Mat3(m))
}

/// After Kahn leaves parts unsolved: if one of them aims at or faces a
/// target that depends on it (itself, or anything that hangs off it), name
/// that.
fn aim_cycle(
    parts:        &[(String, ShapeExpr)],
    placement_of: &HashMap<&str, &Placement>,
    frames:       &FrameMap,
) -> Option<PlacementError> {
    // Each unsolved part's dependencies, with the kind of edge.
    let edges = |name: &str| -> Vec<(&'static str, String)> {
        match placement_of.get(name) {
            None => Vec::new(),
            Some(p @ Placement::Align { object, .. }) => {
                let mut e = vec![("hangs off", object.part.clone())];
                for (verb, r) in target_anchors(p) {
                    e.push((verb, r.part.clone()));
                }
                e
            }
            Some(Placement::Mirror { source, plane, .. }) =>
                vec![("mirrors", source.clone()), ("is mirrored across", plane.part.clone())],
        }
    };
    for (subject, _) in parts {
        if frames.contains_key(subject) { continue; }
        let Some(p @ Placement::Align { span, .. }) = placement_of.get(subject.as_str()).copied() else { continue };
        for (verb, r) in target_anchors(p) {
        if frames.contains_key(&r.part) { continue; }
        let target = if r.anchor == "point" { r.part.clone() } else { format!("{}.{}", r.part, r.anchor) };
        if r.part == *subject {
            return Some(PlacementError::AimCycle { subject: subject.clone(), verb, target, via: Vec::new(), span: *span });
        }
        // Breadth-first from the target back to the subject.
        let mut prev: HashMap<String, (String, &'static str)> = HashMap::new();
        let mut queue = std::collections::VecDeque::from([r.part.clone()]);
        let mut seen: std::collections::HashSet<String> = [r.part.clone()].into();
        while let Some(cur) = queue.pop_front() {
            if cur == *subject {
                let mut via = Vec::new();
                let mut at = cur;
                while let Some((from, kind)) = prev.get(&at) {
                    via.push((from.clone(), *kind, at.clone()));
                    at = from.clone();
                }
                via.reverse();
                return Some(PlacementError::AimCycle { subject: subject.clone(), verb, target, via, span: *span });
            }
            for (kind, next) in edges(&cur) {
                if !frames.contains_key(&next) && seen.insert(next.clone()) {
                    prev.insert(next.clone(), (cur.clone(), kind));
                    queue.push_back(next);
                }
            }
        }
        }
    }
    None
}

/// Sign for `shift`'s second component on a flat face, so it always points
/// the same NAMED way in the object's own frame: `right` (+X) on the front
/// and back faces, `front` (+Z) on the top, bottom, left and right faces.
/// The first component is already consistent: `up` on vertical faces,
/// `right` on horizontal ones.
///
/// Why it is needed: a flat face's tangent frame is built right-handed
/// from its normal, and on three of the six faces (`south`, `east`,
/// `bottom`) that makes the second tangent axis point the negative way —
/// back, or left. A translation has no handedness to preserve, so the fix
/// is simply to flip it there.
///
/// Applies to the six compass faces and to `point(...)` anchors with an
/// axis-aligned normal (instance compass anchors are rewritten to those).
/// Curved anchors — `side`, `surface`, rims — keep their tangent
/// convention (up the meridian, around the shape): their normal varies
/// continuously, and a sign that changed at 90° would be a discontinuity.
fn flat_face_sign(anchor: &str, a: &Anchor) -> f64 {
    if !(crate::anchors::is_compass_name(anchor) || anchor == "point") {
        return 1.0;
    }
    let n = a.frame.rot.col(1);
    let axis_aligned = [n.x, n.y, n.z].iter().any(|c| c.abs() > 1.0 - 1e-9);
    if !axis_aligned {
        return 1.0;
    }
    let z = a.frame.rot.col(2);
    let dominant = [z.x, z.y, z.z]
        .into_iter()
        .max_by(|p, q| p.abs().total_cmp(&q.abs()))
        .unwrap_or(1.0);
    if dominant < 0.0 { -1.0 } else { 1.0 }
}

fn lookup_anchor(
    r:        &AnchorRef,
    shape_of: &HashMap<&str, &ShapeExpr>,
    reflects: &ReflectMap,
    span:     Span,
) -> Result<Anchor, PlacementError> {
    let shape = shape_of.get(r.part.as_str()).ok_or_else(|| PlacementError::UnknownPart {
        part: r.part.clone(), span,
    })?;
    // A mirrored part's anchors come from its REFLECTED geometry, or a part
    // mated to the left rib's front tip would land on the right one.
    let wrapped;
    let shape: &ShapeExpr = match reflects.get(&r.part) {
        Some(n) => {
            wrapped = crate::geometry::mirror_shape((*shape).clone(), *n);
            &wrapped
        }
        None => shape,
    };
    resolve_anchor(shape, &r.anchor, &r.args).map_err(|e| PlacementError::Anchor {
        part:    r.part.clone(),
        message: format!("{e:?}"),
        span:    r.span,
    })
}

/// Householder reflection: M = I − 2·n·nᵀ for unit normal n.
fn reflection_matrix(n: Vec3) -> Mat3 {
    let n = n.normalize().unwrap_or(Vec3::X);
    let mut m = [[0.0f64; 3]; 3];
    let nv = [n.x, n.y, n.z];
    for r in 0..3 {
        for c in 0..3 {
            let id = if r == c { 1.0 } else { 0.0 };
            m[r][c] = id - 2.0 * nv[r] * nv[c];
        }
    }
    Mat3(m)
}

// ── Constraint checking ────────────────────────────────────────────────────
//
// Same predicate vocabulary, evaluated as a CHECK on solved frames instead
// of an assignment. This ships the README's 🔧 "constraint validator".
// Semantic tolerance: half a voxel of slack, in world units.

pub fn check_relation_constraint(
    rel:      &RelationStmt,
    shape_of: &HashMap<&str, &ShapeExpr>,
    frames:   &FrameMap,
    tol:      f64,
) -> Result<(), PlacementError> {
    let world_bounds = |name: &str| -> Option<(Vec3, Vec3)> {
        let shape = shape_of.get(name)?;
        let f     = frames.get(name)?;
        let e     = analytic_extents(shape);
        // Transform the 8 corners; take the world AABB.
        let mut min = Vec3::new(f64::MAX, f64::MAX, f64::MAX);
        let mut max = Vec3::new(f64::MIN, f64::MIN, f64::MIN);
        for &cx in &[e.min.x, e.max.x] {
            for &cy in &[e.min.y, e.max.y] {
                for &cz in &[e.min.z, e.max.z] {
                    let p = f.apply_point(Vec3::new(cx, cy, cz));
                    min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
                    max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
                }
            }
        }
        Some((min, max))
    };

    let subj = rel.subject.name.as_str();
    let obj  = rel.object.name.as_str();
    let (s_min, s_max) = world_bounds(subj).ok_or_else(|| PlacementError::UnknownPart {
        part: subj.to_string(), span: rel.span,
    })?;
    let (o_min, o_max) = world_bounds(obj).ok_or_else(|| PlacementError::UnknownPart {
        part: obj.to_string(), span: rel.span,
    })?;

    let fail = |desc: String| Err(PlacementError::ConstraintViolation {
        description: desc, span: rel.span,
    });

    match rel.predicate {
        RelationKind::Above => {
            if s_min.y + tol < o_max.y {
                return fail(format!(
                    "'{subj}' above '{obj}': expected {subj}.bottom.y ≥ {obj}.top.y, \
                     got {:.2} < {:.2}", s_min.y, o_max.y));
            }
        }
        RelationKind::Below => {
            if s_max.y - tol > o_min.y {
                return fail(format!(
                    "'{subj}' below '{obj}': expected {subj}.top.y ≤ {obj}.bottom.y, \
                     got {:.2} > {:.2}", s_max.y, o_min.y));
            }
        }
        RelationKind::Inside | RelationKind::Surrounds => {
            let (inner, outer, in_min, in_max, out_min, out_max) =
                if rel.predicate == RelationKind::Inside {
                    (subj, obj, s_min, s_max, o_min, o_max)
                } else {
                    (obj, subj, o_min, o_max, s_min, s_max)
                };
            let contained = in_min.x + tol >= out_min.x && in_max.x - tol <= out_max.x
                && in_min.y + tol >= out_min.y && in_max.y - tol <= out_max.y
                && in_min.z + tol >= out_min.z && in_max.z - tol <= out_max.z;
            if !contained {
                return fail(format!("'{inner}' is not contained in '{outer}'"));
            }
        }
        // Remaining predicates: analogous lateral checks; elided in draft.
        _ => {}
    }
    Ok(())
}

// ── Phase-1 realization (the ONLY voxel-aware step) ───────────────────────

/// A solved frame lowered to what the current stamp-then-offset pipeline
/// can execute: an exact axis-aligned rotation + integer voxel offset.
#[derive(Debug, Clone, Copy)]
pub struct RealizedPlacement {
    pub rot:    Mat3, // exact signed permutation
    pub offset: (i32, i32, i32),
}

/// Lower a world frame to voxel space. Errors (rather than resampling)
/// if the rotation is outside the 24 axis-aligned orientations — phase 2
/// (containment-function shapes, stamper tests contains(F⁻¹·p)) deletes
/// this restriction instead of patching it.
pub fn realize(
    part:       &str,
    frame:      &Frame,
    voxel_size: f64,
) -> Result<RealizedPlacement, PlacementError> {
    let rot = snap_axis_aligned(&frame.rot, 1e-6)
        .ok_or_else(|| PlacementError::NonAxisAlignedRotation { part: part.to_string() })?;

    Ok(RealizedPlacement {
        rot,
        offset: (
            (frame.pos.x / voxel_size).round() as i32,
            (frame.pos.y / voxel_size).round() as i32,
            (frame.pos.z / voxel_size).round() as i32,
        ),
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, MateOffsets, NamedArg};

    fn sphere(r: f64) -> ShapeExpr {
        ShapeExpr::Sphere {
            args: vec![NamedArg { key: "radius".into(), value: Expr::Float(r) }],
        }
    }

    fn cylinder(h: f64, r: f64) -> ShapeExpr {
        ShapeExpr::Cylinder {
            args: vec![
                NamedArg { key: "height".into(), value: Expr::Float(h) },
                NamedArg { key: "radius".into(), value: Expr::Float(r) },
            ],
        }
    }

    fn aref(part: &str, anchor: &str) -> AnchorRef {
        AnchorRef {
            part:   part.to_string(),
            anchor: anchor.to_string(),
            args:   Vec::new(),
            span:   Span::new(1, 1),
        }
    }

    /// Qualifiers are expressions in the AST; tests build literal ones.
    fn qz() -> Expr { Expr::Float(0.0) }
    fn qn(v: f64) -> Expr { Expr::Float(v) }

    fn above(subject: &str, object: &str) -> Placement {
        Placement::Align {
            subject: aref(subject, "bottom"),
            object:  aref(object, "top"),
            twist: qz(), pitch: qz(), gap: qz(), offsets: Box::new(MateOffsets::zero()),
            span: Span::new(1, 1),
        }
    }

    /// P1 — `shift` slides a mate within the socket's tangent plane, so
    /// one anchor can host several features. Coordinates are pinned
    /// exactly, because the failure mode here is silently wrong geometry.
    ///
    /// Head is a sphere r=5 at the origin. `Head.north` sits at (0,0,5)
    /// with normal +Z; `frame_from_normal(pos, +Z, x_hint=+Y)` gives that
    /// socket the columns X=+Y, Y=+Z, Z=+X. So a shift of (a, b) displaces
    /// by a along world +Y and b along world +X — the meridian convention,
    /// not an accident. An eye sphere r=1 mated south-to-north therefore
    /// centers at (b, a, 6).
    #[test]
    fn shift_slides_within_the_socket_tangent_plane() {
        let eye = |name: &str, sx: f64, sz: f64| Placement::Align {
            subject: aref(name, "south"),
            object:  aref("Head", "north"),
            twist: qz(), pitch: qz(), gap: qz(), offsets: Box::new(MateOffsets::shift_only(qn(sx), qn(sz))),
            span: Span::new(1, 1),
        };

        let parts = vec![
            ("Head".to_string(),     sphere(5.0)),
            ("EyeL".to_string(),     sphere(1.0)),
            ("EyeR".to_string(),     sphere(1.0)),
            ("Cyclops".to_string(),  sphere(1.0)),
        ];
        let placements = vec![
            eye("EyeL", 1.0, -2.0),
            eye("EyeR", 1.0,  2.0),
            eye("Cyclops", 0.0, 0.0),
        ];

        let frames = resolve_frames(&parts, &placements).unwrap();

        // Zero shift is exactly the old behaviour: dead-center on the face.
        let c = frames["Cyclops"].pos;
        assert!(c.x.abs() < 1e-9 && c.y.abs() < 1e-9, "zero shift must not move anything");
        assert!((c.z - 6.0).abs() < 1e-9);

        let l = frames["EyeL"].pos;
        let r = frames["EyeR"].pos;

        // Displaced laterally, in opposite directions.
        assert!((l.x + 2.0).abs() < 1e-9, "EyeL x = -2, got {}", l.x);
        assert!((r.x - 2.0).abs() < 1e-9, "EyeR x = +2, got {}", r.x);

        // Raised equally up the meridian.
        assert!((l.y - 1.0).abs() < 1e-9, "EyeL y = 1, got {}", l.y);
        assert!((r.y - 1.0).abs() < 1e-9, "EyeR y = 1, got {}", r.y);

        // And COPLANAR — the property the old language could not produce
        // at the same time as distinctness. This is bench case char-001.
        assert!((l.z - r.z).abs() < 1e-9, "eyes must share a plane");
        assert!((l.z - 6.0).abs() < 1e-9);
    }

    /// `shift` means the same named directions on every flat face: the
    /// first component is up on vertical faces and right on horizontal
    /// ones; the second is right on the front/back faces and front on the
    /// others. Before this, `south`, `east` and `bottom` ran the second
    /// component backward — a robot's feet came out pointing behind it,
    /// and no symmetric test could tell.
    #[test]
    fn shift_means_the_same_directions_on_every_flat_face() {
        let bx = ShapeExpr::Box_ { args: vec![
            NamedArg { key: "width".into(),  value: Expr::Float(4.0) },
            NamedArg { key: "height".into(), value: Expr::Float(4.0) },
            NamedArg { key: "depth".into(),  value: Expr::Float(4.0) },
        ] };
        // (face, first-component direction, second-component direction)
        let cases: [(&str, Vec3, Vec3); 6] = [
            ("top",    Vec3::X, Vec3::Z),
            ("bottom", Vec3::X, Vec3::Z),
            ("north",  Vec3::Y, Vec3::X),
            ("south",  Vec3::Y, Vec3::X),
            ("east",   Vec3::Y, Vec3::Z),
            ("west",   Vec3::Y, Vec3::Z),
        ];
        for (face, first, second) in cases {
            let at = |sx: f64, sz: f64| {
                let parts = vec![("Core".to_string(), bx.clone()), ("Dot".to_string(), sphere(0.5))];
                let placements = vec![Placement::Align {
                    subject: aref("Dot", "center"),
                    object:  aref("Core", face),
                    twist: qz(), pitch: qz(), gap: qz(), offsets: Box::new(MateOffsets::shift_only(qn(sx), qn(sz))),
                    span: Span::new(1, 1),
                }];
                resolve_frames(&parts, &placements).unwrap()["Dot"].pos
            };
            let origin = at(0.0, 0.0);
            let d1 = at(1.0, 0.0).sub(origin);
            let d2 = at(0.0, 1.0).sub(origin);
            assert!((d1.sub(first)).length() < 1e-9,  "{face}: first component went {d1:?}, want {first:?}");
            assert!((d2.sub(second)).length() < 1e-9, "{face}: second component went {d2:?}, want {second:?}");
        }
    }

    /// `lean=(a, b)` tips a part toward the face's `a`/`b` directions — the
    /// same named directions as `shift` — about the socket's own tangent
    /// axes, so it rises without sweeping around a curved host and without
    /// rolling. A flat plate is the witness: a capsule hides roll, a plate
    /// does not.
    #[test]
    fn lean_rises_without_rolling_where_twist_and_pitch_roll() {
        let na = |k: &str, v: f64| NamedArg { key: k.into(), value: Expr::Float(v) };
        let plate = ShapeExpr::Box_ { args: vec![na("width", 0.2), na("height", 4.0), na("depth", 1.0)] };
        let side = AnchorRef {
            part: "Trunk".into(), anchor: "side".into(),
            args: vec![na("t", 0.5), na("angle", 90.0)], span: Span::new(1, 1),
        };
        let solve = |offsets: MateOffsets, twist: f64, pitch: f64| {
            let parts = vec![("Trunk".to_string(), cylinder(10.0, 1.0)), ("Plate".to_string(), plate.clone())];
            let placements = vec![Placement::Align {
                subject: aref("Plate", "bottom"), object: side.clone(),
                twist: qn(twist), pitch: qn(pitch), gap: qz(),
                offsets: Box::new(offsets), span: Span::new(1, 1),
            }];
            resolve_frames(&parts, &placements).unwrap()["Plate"].rot
        };

        // lean=(45, 0): the axis rises 45° off the radial (+X), and the
        // plate's broad face stays upright — its thin axis stays level.
        let r = solve(MateOffsets { shift: (qz(), qz()), lean: (qn(45.0), qz()), aim: None, facing: None }, 0.0, 0.0);
        let axis = r.apply(Vec3::Y);
        let h = std::f64::consts::FRAC_1_SQRT_2;
        assert!((axis.sub(Vec3::new(h, h, 0.0))).length() < 1e-9, "axis {axis:?}");
        assert!(r.apply(Vec3::Z).y.abs() < 1e-9, "lean must not roll the plate");

        // The old idiom reaches the same 45° elevation, but rolls the plate
        // a quarter turn about its own axis — the rib-on-its-side bug.
        let r_old = solve(MateOffsets::zero(), 90.0, 45.0);
        assert!((r_old.apply(Vec3::Y).y - h).abs() < 1e-9, "same elevation");
        assert!(r_old.apply(Vec3::Z).y.abs() > 0.5, "…but rolled");
    }

    /// `b` follows the same flat-face table as `shift`: on a `bottom` face
    /// it is front, so a part hanging below leans forward, not back.
    #[test]
    fn lean_b_on_a_bottom_face_is_forward() {
        let parts = vec![("Base".to_string(), sphere(2.0)), ("Rod".to_string(), cylinder(4.0, 0.2))];
        let placements = vec![Placement::Align {
            subject: aref("Rod", "top"), object: aref("Base", "bottom"),
            twist: qz(), pitch: qz(), gap: qz(),
            offsets: Box::new(MateOffsets { shift: (qz(), qz()), lean: (qz(), qn(30.0)), aim: None, facing: None }),
            span: Span::new(1, 1),
        }];
        let rod = resolve_frames(&parts, &placements).unwrap()["Rod"];
        // The rod's far end (its base, local origin) hangs down and forward.
        let base = rod.pos;
        assert!(base.y < -2.0 && base.z > 0.5, "hangs down and leans forward, got {base:?}");
    }

    #[test]
    fn lean_on_an_orientation_free_mate_is_an_error() {
        let parts = vec![("A".to_string(), sphere(1.0)), ("B".to_string(), sphere(1.0))];
        let placements = vec![Placement::Align {
            subject: aref("B", "center"), object: aref("A", "center"),
            twist: qz(), pitch: qz(), gap: qz(),
            offsets: Box::new(MateOffsets { shift: (qz(), qz()), lean: (qn(10.0), qz()), aim: None, facing: None }),
            span: Span::new(1, 1),
        }];
        let errs = resolve_frames(&parts, &placements).unwrap_err();
        assert!(errs.iter().any(|e| e.to_string().contains("`lean` needs an oriented anchor")), "got: {errs:?}");
    }

    /// The solver hands back the local reflection a mirrored part's
    /// geometry needs — only for mirrored parts.
    #[test]
    fn mirror_records_the_local_reflection_normal() {
        let parts = vec![
            ("Core".to_string(), sphere(1.0)),
            ("Top".to_string(),  sphere(1.0)),
            ("Twin".to_string(), sphere(1.0)),
        ];
        let placements = vec![
            above("Top", "Core"),
            Placement::Mirror {
                subject: "Twin".to_string(),
                source:  "Top".to_string(),
                plane:   aref("Core", "center"),
                axis:    Axis::X,
                span:    Span::new(1, 1),
            },
        ];
        let (_, reflects) = resolve_frames_full(&parts, &placements).unwrap();
        assert!((reflects["Twin"].x.abs() - 1.0).abs() < 1e-9);
        assert!(!reflects.contains_key("Top"), "only mirrored parts are reflected");
    }

    #[test]
    fn mirroring_a_mirror_image_is_a_clear_error() {
        let parts = vec![
            ("Core".to_string(),  sphere(1.0)),
            ("Top".to_string(),   sphere(1.0)),
            ("Twin".to_string(),  sphere(1.0)),
            ("Twin2".to_string(), sphere(1.0)),
        ];
        let mirror = |subject: &str, source: &str| Placement::Mirror {
            subject: subject.to_string(),
            source:  source.to_string(),
            plane:   aref("Core", "center"),
            axis:    Axis::X,
            span:    Span::new(1, 1),
        };
        let placements = vec![above("Top", "Core"), mirror("Twin", "Top"), mirror("Twin2", "Twin")];
        let errs = resolve_frames_full(&parts, &placements).unwrap_err();
        assert!(errs.iter().any(|e| e.to_string().contains("mirror image")), "got: {errs:?}");
    }

    /// Mirror with a real source across the plane part's local X axis —
    /// bilateral symmetry. Pins the v1 bug fix (+Y normal mirrored
    /// top-to-bottom instead of left-to-right).
    #[test]
    fn mirror_is_bilateral() {
        let parts = vec![
            ("Core".to_string(), sphere(1.0)),
            ("ArmR".to_string(), sphere(1.0)),
            ("ArmL".to_string(), sphere(1.0)),
        ];
        let placements = vec![
            // ArmR.west on Core.east → ArmR centered at +2 on X
            Placement::Align {
                subject: aref("ArmR", "west"),
                object:  aref("Core", "east"),
                twist: qz(), pitch: qz(), gap: qz(), offsets: Box::new(MateOffsets::zero()),
                span: Span::new(1, 1),
            },
            Placement::Mirror {
                subject: "ArmL".to_string(),
                source:  "ArmR".to_string(),
                plane:   aref("Core", "center"),
                axis:    Axis::X,
                span:    Span::new(1, 1),
            },
        ];
        let frames = resolve_frames(&parts, &placements).unwrap();
        let r = frames["ArmR"];
        let l = frames["ArmL"];
        assert!((r.pos.x - 2.0).abs() < 1e-9);
        assert!((l.pos.x + 2.0).abs() < 1e-9);
        assert!((l.pos.y - r.pos.y).abs() < 1e-9);
        assert!((l.pos.z - r.pos.z).abs() < 1e-9);
        // Mirrored rotation is still axis-aligned → phase-1 realizable.
        assert!(snap_axis_aligned(&l.rot, 1e-6).is_some());
    }

    /// The migration proof in miniature: Skull above Spine through the
    /// mate formula lands the skull center at spine-top + radius —
    /// surfaces touching, identity rotation, exactly like the skeleton.
    #[test]
    fn skull_above_spine_touches() {
        let parts = vec![
            ("Spine".to_string(), cylinder(24.0, 0.8)),
            ("Skull".to_string(), sphere(4.0)),
        ];
        let frames = resolve_frames(&parts, &[above("Skull", "Spine")]).unwrap();

        let skull = frames["Skull"];
        assert!((skull.pos.y - 28.0).abs() < 1e-9); // 24 (spine top) + 4 (radius)
        assert_eq!(snap_axis_aligned(&skull.rot, 1e-9), Some(Mat3::IDENTITY));
    }

    /// Chains longer than 4 — the old resolver's silent failure mode —
    /// solve exactly in one pass.
    #[test]
    fn long_chain_is_exact() {
        let parts: Vec<(String, ShapeExpr)> =
            (0..8).map(|i| (format!("S{i}"), sphere(1.0))).collect();
        let placements: Vec<Placement> =
            (1..8).map(|i| above(&format!("S{i}"), &format!("S{}", i - 1))).collect();

        let frames = resolve_frames(&parts, &placements).unwrap();
        assert!((frames["S7"].pos.y - 14.0).abs() < 1e-9); // 7 spheres × 2 units
    }

    #[test]
    fn cycle_is_a_hard_error_with_chain() {
        let parts = vec![
            ("A".to_string(), sphere(1.0)),
            ("B".to_string(), sphere(1.0)),
        ];
        let errs = resolve_frames(&parts, &[above("A", "B"), above("B", "A")]).unwrap_err();
        assert!(matches!(errs[0], PlacementError::CyclicPlacement { .. }));
    }

    #[test]
    fn double_placement_is_over_constrained() {
        let parts = vec![
            ("A".to_string(), sphere(1.0)),
            ("B".to_string(), sphere(1.0)),
            ("C".to_string(), sphere(1.0)),
        ];
        let errs = resolve_frames(&parts, &[above("A", "B"), above("A", "C")]).unwrap_err();
        assert!(matches!(errs[0], PlacementError::OverConstrained { .. }));
    }

    /// side(t, angle) + the mate: a branch on a trunk points straight out,
    /// orthogonal — the user-confirmed twist-0 datum, emerging from the
    /// socket normal rather than from any rotate keyword.
    #[test]
    fn branch_on_trunk_side_is_orthogonal() {
        let parts = vec![
            ("Trunk".to_string(), cylinder(10.0, 1.0)),
            ("Branch".to_string(), cylinder(4.0, 0.3)),
        ];
        let placements = vec![Placement::Align {
            subject: AnchorRef {
                part: "Branch".into(), anchor: "bottom".into(),
                args: vec![], span: Span::new(1, 1),
            },
            object: AnchorRef {
                part: "Trunk".into(), anchor: "side".into(),
                args: vec![
                    NamedArg { key: "t".into(),     value: Expr::Float(0.7) },
                    NamedArg { key: "angle".into(), value: Expr::Float(0.0) },
                ],
                span: Span::new(1, 1),
            },
            twist: qz(), pitch: qz(), gap: qz(), offsets: Box::new(MateOffsets::zero()),
            span: Span::new(1, 1),
        }];

        let frames = resolve_frames(&parts, &placements).unwrap();
        let branch = frames["Branch"];

        // The branch's local +Y (its axis) maps to world +Z — radially
        // outward at angle 0, orthogonal to the trunk axis.
        let axis = branch.apply_dir(Vec3::Y);
        assert!((axis.z - 1.0).abs() < 1e-9);
        assert!(axis.y.abs() < 1e-9);

        // And it's a quarter-turn: realizable in phase 1.
        assert!(snap_axis_aligned(&branch.rot, 1e-6).is_some());
    }
}

// ── `aim=` (Issue C, DOC-20261007-aim-facing-report §3) ────────────────────
//
// Source-level: every test parses real Moxi, solves it through the pipeline
// and pins world coordinates, because a wrong aim compiles clean.

#[cfg(test)]
mod aim_tests {
    use std::collections::HashMap;

    use crate::frame::{Frame, Mat3, Vec3};
    use crate::pipeline::compile_to_scene_posed;

    fn frames_posed(src: &str, pose: Option<&str>) -> HashMap<String, Frame> {
        let scene = compile_to_scene_posed(src, pose).unwrap_or_else(|e| panic!("compiles: {e:?}"));
        scene.layers.iter().flat_map(|l| l.parts.iter()).map(|p| {
            let f = &p.frame;
            (p.name.clone(), Frame::new(Mat3(f.rot), Vec3::new(f.pos[0], f.pos[1], f.pos[2])))
        }).collect()
    }
    fn frames(src: &str) -> HashMap<String, Frame> { frames_posed(src, None) }

    fn errors(src: &str) -> String {
        match compile_to_scene_posed(src, None) {
            Ok(_) => panic!("expected an error"),
            Err(es) => es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"),
        }
    }

    fn axis(f: &Frame) -> Vec3 { f.rot.col(1) }
    fn near(a: Vec3, b: Vec3) -> bool { a.sub(b).length() < 1e-9 }
    fn same(a: &Frame, b: &Frame) -> bool {
        near(a.pos, b.pos) && (0..3).all(|c| near(a.rot.col(c), b.rot.col(c)))
    }

    /// A rod on a slab's top face (normal +Y, socket point (0, 5, 0)) with
    /// the given qualifiers.
    fn rod(quals: &str) -> String {
        format!("thing T {{
            part Slab {{ shape = box(width=100, height=10, depth=100) }}
            part Rod  {{ shape = cylinder(height=40, radius=2) }}
            part Ball {{ shape = sphere(radius=5) }}
            relation {{
                Rod.bottom on Slab.top {quals}
                Ball.bottom on Slab.top shift=(40, 40)
            }}
        }}
        print T")
    }

    #[test]
    fn aim_north_on_a_top_face_points_the_axis_north_without_rolling() {
        let rest = frames(&rod(""))["Rod"];
        let f = frames(&rod("aim=north"))["Rod"];
        assert!(near(axis(&f), Vec3::new(0.0, 0.0, 1.0)), "axis {:?}", axis(&f));
        // The shortest turn from +Y to +Z is about +X: the rod's own x axis
        // stays put — no roll.
        assert!(near(f.rot.col(0), Vec3::X), "x {:?}", f.rot.col(0));
        // The pivot stays at the socket point.
        assert!(near(f.pos, rest.pos), "{:?} vs {:?}", f.pos, rest.pos);
        for (word, want) in [("south", Vec3::new(0.0, 0.0, -1.0)), ("east", Vec3::X),
                             ("west", Vec3::new(-1.0, 0.0, 0.0)), ("up", Vec3::Y)] {
            let f = frames(&rod(&format!("aim={word}")))["Rod"];
            assert!(near(axis(&f), want), "aim={word}: axis {:?}", axis(&f));
        }
        // `aim=up` on an up-facing socket is no turn at all.
        assert!(same(&frames(&rod("aim=up"))["Rod"], &rest));
    }

    #[test]
    fn a_point_target_points_the_axis_at_it_from_the_slid_socket() {
        let slid = frames(&rod("shift=(10, -5)"))["Rod"];
        // shift slides the socket point, in the ORIGINAL socket frame.
        assert!(near(slid.pos, Vec3::new(10.0, 5.0, -5.0)), "socket point {:?}", slid.pos);
        let f = frames(&rod("shift=(10, -5) aim=[60, 80, -30]"))["Rod"];
        assert!(near(f.pos, slid.pos), "aim does not move the pivot");
        let want = Vec3::new(50.0, 75.0, -25.0).normalize().unwrap();
        assert!(near(axis(&f), want), "axis {:?}, want {want:?}", axis(&f));

        // A part target is its center; an anchor target is that anchor.
        let b = frames(&rod(""))["Ball"].pos; // sphere origin = its center
        let f = frames(&rod("aim=Ball"))["Rod"];
        assert!(near(axis(&f), b.sub(slid.pos.sub(Vec3::new(10.0, 0.0, -5.0))).normalize().unwrap()));
        let f = frames(&rod("aim=Ball.top"))["Rod"];
        let top = b.add(Vec3::new(0.0, 5.0, 0.0));
        assert!(near(axis(&f), top.sub(Vec3::new(0.0, 5.0, 0.0)).normalize().unwrap()), "{:?}", axis(&f));
    }

    /// Metamorphic: aiming at the point a one-component `lean` points at
    /// gives the identical frame — `aim` is the lean the compiler computes.
    /// On a curved `side` anchor, so nothing is axis-aligned.
    #[test]
    fn aiming_where_a_one_component_lean_points_is_that_lean() {
        let src = |quals: &str| format!("thing T {{
            part Trunk {{ shape = cylinder(height=100, radius=10) }}
            part Rod   {{ shape = capsule(height=30, radius=2) }}
            relation {{ Rod.bottom on Trunk.side(t=0.6, angle=30) {quals} }}
        }}
        print T");
        for lean in ["lean=(30, 0)", "lean=(0, 25)", "lean=(-40, 0)"] {
            let leaned = frames(&src(lean))["Rod"];
            // The capsule's origin sits on its axis, so a point along the
            // axis from it is a point along the axis from the socket.
            let t = leaned.pos.add(axis(&leaned).scale(50.0));
            let aimed = frames(&src(&format!("aim=[{}, {}, {}]", t.x, t.y, t.z)))["Rod"];
            assert!(same(&aimed, &leaned), "{lean}: aimed {aimed:?}\nleaned {leaned:?}");
        }
    }

    /// `lean`, `twist` and `pitch` act from the aimed rest. Aiming east from
    /// a top face turns the socket −90° about Z: its a-direction (+X) goes
    /// to −Y, its b-direction (+Z) stays.
    #[test]
    fn aim_then_lean_tips_from_the_aimed_rest() {
        let (s, c) = (10f64.to_radians().sin(), 10f64.to_radians().cos());
        let f = frames(&rod("aim=east lean=(10, 0)"))["Rod"];
        assert!(near(axis(&f), Vec3::new(c, -s, 0.0)), "lean a: {:?}", axis(&f));
        let f = frames(&rod("aim=east lean=(0, 10)"))["Rod"];
        assert!(near(axis(&f), Vec3::new(c, 0.0, s)), "lean b: {:?}", axis(&f));
        // twist spins about the aimed axis: the axis stays east.
        let f = frames(&rod("aim=east twist=90"))["Rod"];
        assert!(near(axis(&f), Vec3::X), "twist: {:?}", axis(&f));
    }

    const AIM: &str = include_str!("../scripts/AIM.md");

    /// Every pose re-solves the thing, so an aim follows a target the pose
    /// moves: the watcher's eye tracks the ball through `Roll`.
    #[test]
    fn a_pose_that_moves_the_target_moves_the_aimer() {
        let (rest, roll) = (frames(AIM), frames_posed(AIM, Some("Roll")));
        for f in [&rest, &roll] {
            let eye = f["Eye"].pos.sub(Vec3::ZERO);
            let want = f["Ball"].pos.sub(eye).normalize().unwrap();
            assert!(near(axis(&f["Eye"]), want), "eye {:?} vs {want:?}", axis(&f["Eye"]));
        }
        assert!(near(rest["Eye"].pos, roll["Eye"].pos), "the eye's pivot stays");
        let turned = axis(&rest["Eye"]).dot(axis(&roll["Eye"])).acos().to_degrees();
        assert!(turned > 5.0, "the eye turned only {turned}°");
        // Spikes stand straight up wherever the dome puts them; the boom
        // points at its point.
        for k in 0..6 {
            assert!(near(axis(&rest[&format!("Spike[{k}]")]), Vec3::Y));
        }
        let boom = &rest["Boom"];
        assert!(near(axis(boom), Vec3::new(330.0, 210.0, 40.0).sub(boom.pos).normalize().unwrap()));
    }

    /// Directions and points written inside a thing are in THAT thing's
    /// space: an instance turned onto a wall carries its aims with it, so
    /// each part sits on the instance's root exactly as it does standalone.
    #[test]
    fn aims_inside_an_instance_turn_with_it() {
        let src = |aim: &str| format!("thing Spike {{
            part Foot {{ shape = box(width=10, height=2, depth=10) }}
            part Pin  {{ shape = cylinder(height=20, radius=1) }}
            relation {{ Pin.bottom on Foot.top {aim} }}
        }}
        thing Board {{
            part Wall {{ shape = box(width=100, height=100, depth=10) }}
            part S    {{ thing = Spike }}
            relation {{ S.bottom on Wall.north }}
        }}
        print Board");
        let alone = |aim: &str| src(aim).split("thing Board").next().unwrap().to_string() + "print Spike";
        for aim in ["aim=north", "aim=[0, 30, 30]", "aim=west lean=(20, 0)"] {
            let f = frames(&src(aim));
            let (foot, pin) = (f["S.Foot"], f["S.Pin"]);
            let local = foot.inverse().compose(&pin);
            let g = frames(&alone(aim));
            let alone = g["Foot"].inverse().compose(&g["Pin"]);
            assert!(same(&local, &alone), "{aim}: in Board {local:?}\nalone {alone:?}");
        }
        // And concretely: the wall's north face turns the spike's up to +Z,
        // so the spike's own north is no longer world north.
        let f = frames(&src("aim=north"));
        let want = f["S.Foot"].rot.apply(Vec3::Z);
        assert!(near(axis(&f["S.Pin"]), want));
        assert!(axis(&f["S.Pin"]).z.abs() < 0.5, "turned with the instance");
    }

    /// From outside, an aim may name a part inside an instance (its center)
    /// or an anchor on one, like either side of a mate.
    #[test]
    fn an_aim_reaches_into_an_instance() {
        let src = |target: &str| format!("thing Arm {{
            part Upper {{ shape = capsule(height=30, radius=3) }}
            part Hand  {{ shape = sphere(radius=5) }}
            relation {{ Hand.center on Upper.top }}
        }}
        thing Smith {{
            part Body {{ shape = box(width=40, height=60, depth=30) }}
            part A    {{ thing = Arm }}
            part Head {{ shape = cone(height=20, radius=8) }}
            relation {{
                A.bottom on Body.top shift=(-15, 0) lean=(-30, 0)
                Head.bottom on Body.top shift=(12, 0) aim={target}
            }}
        }}
        print Smith");
        let f = frames(&src("A.Hand"));
        let (head, hand) = (f["Head"], f["A.Hand"]);
        assert!(near(axis(&head), hand.pos.sub(head.pos).normalize().unwrap()), "{:?}", axis(&head));
        let f = frames(&src("A.Upper.top"));
        // A capsule's universal `top` is the apex of its cap: height + radius.
        let top = f["A.Upper"].apply_point(Vec3::new(0.0, 33.0, 0.0));
        assert!(near(axis(&f["Head"]), top.sub(f["Head"].pos).normalize().unwrap()));
    }

    /// A mirror image follows its source, so its aim is the mirror image.
    #[test]
    fn a_mirror_image_aims_mirrored() {
        let f = frames("thing T {
            part Body { shape = box(width=40, height=40, depth=40) }
            part ArmR { shape = capsule(height=30, radius=3) }
            part ArmL { shape = capsule(height=30, radius=3) }
            relation {
                ArmR.bottom on Body.east aim=[60, 40, 30]
                ArmL symmetric_across Body from=ArmR
            }
        }
        print T");
        let (r, l) = (axis(&f["ArmR"]), axis(&f["ArmL"]));
        assert!(near(l, Vec3::new(-r.x, r.y, r.z)), "{l:?} vs {r:?}");
    }

    /// Loops: each link of a neck aims at the same point.
    #[test]
    fn indexed_mates_in_a_loop_aim() {
        let f = frames("thing Hydra {
            part Body { shape = sphere(radius=20) }
            for k in 0..4 { part Neck[k] { shape = capsule(height=18, radius=3) } }
            relation { Neck[0].bottom on Body.top aim=[0, 120, 40] }
            for k in 1..4 { relation { Neck[k].bottom on Neck[k - 1].top aim=[0, 120, 40] } }
        }
        print Hydra");
        for k in 0..4 {
            let n = f[&format!("Neck[{k}]")];
            assert!(near(axis(&n), Vec3::new(0.0, 120.0, 40.0).sub(n.pos).normalize().unwrap()), "Neck[{k}]");
        }
    }

    /// A pose may set `aim` (absolute, like every qualifier) — on a mate
    /// that aims and on one that does not. Semantics are substitution: the
    /// posed frames are the frames of the script with the aim pasted in.
    #[test]
    fn a_pose_may_set_aim_by_substitution() {
        let src = |rod: &str, pose: &str| format!("thing T {{
            part Slab {{ shape = box(width=100, height=10, depth=100) }}
            part Rod  {{ shape = cylinder(height=40, radius=2) }}
            part Ball {{ shape = sphere(radius=5) }}
            relation {{
                Ball.bottom on Slab.top shift=(40, 40)
                Rod.bottom on Slab.top {rod}
            }}
            {pose}
        }}
        print T");
        for (rest, posed) in [("", "aim=north"), ("aim=Ball", "aim=Ball.top lean=(5, 0)"), ("aim=east", "aim=[10, 50, 0]")] {
            let p = frames_posed(&src(rest, &format!("pose P {{ Rod {posed} }}")), Some("P"));
            let q = frames(&src(posed, ""));
            assert!(same(&p["Rod"], &q["Rod"]), "pose {{ Rod {posed} }} over `{rest}`");
        }
    }

    /// An instance's pose carries its aims into the parent, in the
    /// instance's space.
    #[test]
    fn an_instance_pose_aims_in_the_instance_space() {
        let spike = "thing Spike {
            part Foot { shape = box(width=10, height=2, depth=10) }
            part Pin  { shape = cylinder(height=20, radius=1) }
            relation { Pin.bottom on Foot.top }
            pose Tilt { Pin aim=[0, 10, 30] }
        }";
        let board = format!("{spike}
        thing Board {{
            part Wall {{ shape = box(width=100, height=100, depth=10) }}
            part S    {{ thing = Spike }}
            relation {{ S.bottom on Wall.north }}
            pose T {{ S pose=Tilt }}
        }}
        print Board");
        let f = frames_posed(&board, Some("T"));
        let g = frames_posed(&format!("{spike}\nprint Spike"), Some("Tilt"));
        let local = f["S.Foot"].inverse().compose(&f["S.Pin"]);
        let alone = g["Foot"].inverse().compose(&g["Pin"]);
        assert!(same(&local, &alone), "in Board {local:?}\nalone {alone:?}");
        assert!(!near(axis(&g["Pin"]), Vec3::Y), "the pose turned the pin");
    }

    // ── errors ────────────────────────────────────────────────────────────

    fn arm(extra: &str, aim: &str) -> String {
        format!("thing Smith {{
            part Body  {{ shape = box(width=40, height=60, depth=30) }}
            part ArmR  {{ shape = capsule(height=30, radius=3) }}
            part HandR {{ shape = sphere(radius=5) }}
            {extra}
            relation {{
                ArmR.bottom on Body.east {aim}
                HandR.center on ArmR.top
            }}
        }}
        print Smith")
    }

    #[test]
    fn an_aim_cycle_names_the_chain() {
        let m = errors(&arm("", "aim=HandR"));
        assert!(m.contains("'ArmR' aims at HandR.center, but HandR hangs off ArmR (ArmR → HandR)"), "{m}");
        let m = errors(&arm("part Finger { shape = sphere(radius=1) }\n relation { Finger.bottom on HandR.top }",
                            "aim=Finger.top"));
        assert!(m.contains("'ArmR' aims at Finger.top, but Finger hangs off HandR, which hangs off ArmR \
                            (ArmR → HandR → Finger)"), "{m}");
        let m = errors(&arm("", "aim=ArmR.top"));
        assert!(m.contains("'ArmR' aims at ArmR.top, which is on 'ArmR' itself"), "{m}");
    }

    #[test]
    fn degenerate_aims_are_errors_with_fixes() {
        // Straight back into the socket: the half turn has no shortest way.
        let m = errors(&rod("aim=down"));
        assert!(m.contains("points straight back into its own socket on 'Slab.top'"), "{m}");
        // At the socket point itself: no direction.
        let m = errors(&rod("aim=[0, 5, 0]"));
        assert!(m.contains("on part 'Rod': [0, 5, 0] gives no direction: the target is the \
                            socket point itself"), "{m}");
        // Orientation-free mates have no axis to aim.
        let m = errors(&arm("", "").replace("HandR.center on ArmR.top", "HandR.center on ArmR.top aim=up"));
        assert!(m.contains("`aim` needs an oriented anchor on both sides"), "{m}");
    }

    #[test]
    fn an_unknown_aim_target_lists_directions_and_parts() {
        let m = errors(&rod("aim=nrth"));
        assert!(m.contains("aim of 'Rod': 'nrth' is neither a direction (north, south, east, west, up, down) \
                            nor a part of 'T' — parts: Ball, Rod, Slab"), "{m}");
        let m = errors(&rod("aim=Bal.top"));
        assert!(m.contains("'Bal' is not a part of 'T'"), "{m}");
        let m = errors(&rod("aim=Ball.tpo"));
        assert!(m.contains("part 'Ball' has no anchor 'tpo' — valid anchors:"), "{m}");
    }
}

// ── `facing=` (Issue D, DOC-20261007-aim-facing-report §4) ─────────────────

#[cfg(test)]
mod facing_tests {
    use std::collections::HashMap;

    use crate::frame::{Frame, Mat3, Vec3};
    use crate::pipeline::{compile_to_scene, compile_to_scene_posed};

    fn frames_posed(src: &str, pose: Option<&str>) -> HashMap<String, Frame> {
        let scene = compile_to_scene_posed(src, pose).unwrap_or_else(|e| panic!("compiles: {e:?}"));
        scene.layers.iter().flat_map(|l| l.parts.iter()).map(|p| {
            let f = &p.frame;
            (p.name.clone(), Frame::new(Mat3(f.rot), Vec3::new(f.pos[0], f.pos[1], f.pos[2])))
        }).collect()
    }
    fn frames(src: &str) -> HashMap<String, Frame> { frames_posed(src, None) }
    fn errors(src: &str) -> String {
        match compile_to_scene(src) {
            Ok(_) => panic!("expected an error"),
            Err(es) => es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"),
        }
    }
    fn axis(f: &Frame) -> Vec3 { f.rot.col(1) }
    /// A part's own north (+Z) in the world.
    fn north(f: &Frame) -> Vec3 { f.rot.col(2) }
    fn near(a: Vec3, b: Vec3, tol: f64) -> bool { a.sub(b).length() < tol }
    fn same(a: &Frame, b: &Frame) -> bool {
        near(a.pos, b.pos, 1e-9) && (0..3).all(|c| near(a.rot.col(c), b.rot.col(c), 1e-9))
    }
    const SOUTH: Vec3 = Vec3 { x: 0.0, y: 0.0, z: -1.0 };
    const WEST:  Vec3 = Vec3 { x: -1.0, y: 0.0, z: 0.0 };

    /// A flat, asymmetric singer on a stage's top and hanging from a
    /// ceiling's bottom — the choir finding: legacy twist ran the other way
    /// on the bottom face; `facing` reads the same on both.
    fn choir(quals: &str) -> String {
        format!("thing Choir {{
            part Stage   {{ shape = box(width=200, height=10, depth=100) }}
            part Ceiling {{ shape = box(width=200, height=10, depth=100) }}
            part Up      {{ shape = box(width=20, height=40, depth=4) }}
            part Down    {{ shape = box(width=20, height=40, depth=4) }}
            relation {{
                Ceiling.bottom on Stage.top gap=150
                Up.bottom      on Stage.top shift=(-40, 0) {quals}
                Down.top       on Ceiling.bottom shift=(40, 0) {quals}
            }}
        }}
        print Choir")
    }

    #[test]
    fn choir_facing_north_fronts_north_on_top_and_bottom_faces() {
        for (word, want) in [("north", Vec3::Z), ("east", Vec3::X), ("south", SOUTH), ("west", WEST)] {
            let f = frames(&choir(&format!("facing={word}")));
            assert!(near(north(&f["Up"]), want, 1e-9),   "top, facing={word}: {:?}", north(&f["Up"]));
            assert!(near(north(&f["Down"]), want, 1e-9), "bottom, facing={word}: {:?}", north(&f["Down"]));
            // The swivel is about the axis: neither part tips.
            assert!(near(axis(&f["Up"]), Vec3::Y, 1e-9));
            assert!(near(axis(&f["Down"]), Vec3::Y, 1e-9));
        }
        // Rest without facing stays exactly as before.
        let rest = frames(&choir(""));
        assert!(near(north(&rest["Up"]), Vec3::Z, 1e-9));
    }

    /// `twist` is an offset from the faced rest, and turns about the axis.
    #[test]
    fn twist_is_an_offset_from_the_faced_rest() {
        let faced = frames(&choir("facing=east"))["Up"];
        let twisted = frames(&choir("facing=east twist=30"))["Up"];
        let turned = north(&faced).dot(north(&twisted)).acos().to_degrees();
        assert!((turned - 30.0).abs() < 1e-9, "turned {turned}°");
        assert!(near(axis(&twisted), Vec3::Y, 1e-9));
    }

    /// Lids on `point` anchors: the same `facing=north` front whether the
    /// normal is vertical or horizontal (east).
    #[test]
    fn lids_face_north_on_vertical_and_horizontal_point_normals() {
        let lid = |n: &str| format!("thing Chest {{
            part Body {{ shape = box(width=60, height=40, depth=40) }}
            part Lid  {{ shape = box(width=30, height=4, depth=20) }}
            relation {{ Lid.bottom on Body.point(x=0, y=20, z=0, {n}) facing=north }}
        }}
        print Chest");
        let up   = frames(&lid("nx=0, ny=1, nz=0"))["Lid"];
        let east = frames(&lid("nx=1, ny=0, nz=0"))["Lid"];
        assert!(near(north(&up), Vec3::Z, 1e-9), "vertical: {:?}", north(&up));
        assert!(near(north(&east), Vec3::Z, 1e-9), "horizontal: {:?}", north(&east));
        assert!(near(axis(&east), Vec3::X, 1e-9));
    }

    /// The near-vertical datum: a normal a hair off vertical used to swing
    /// the tangent frame; `facing=north` consults no datum.
    #[test]
    fn a_near_vertical_normal_still_fronts_north() {
        let lid = |n: &str| format!("thing Chest {{
            part Body {{ shape = box(width=60, height=40, depth=40) }}
            part Lid  {{ shape = box(width=30, height=4, depth=20) }}
            relation {{ Lid.bottom on Body.point(x=0, y=20, z=0, {n}) facing=north }}
        }}
        print Chest");
        let exact = frames(&lid("nx=0, ny=1, nz=0"))["Lid"];
        let near_v = frames(&lid("nx=0.001, ny=1, nz=0"))["Lid"];
        assert!(north(&near_v).z > 0.9999, "front {:?}", north(&near_v));
        assert!(near(north(&near_v), north(&exact), 2e-3), "no flip: {:?} vs {:?}", north(&near_v), north(&exact));
    }

    /// A part mated by its own north or south has its `top` as its front.
    #[test]
    fn a_part_mated_by_its_north_fronts_with_its_top() {
        let f = frames("thing Wall {
            part Slab   { shape = box(width=100, height=100, depth=10) }
            part Plaque { shape = box(width=30, height=20, depth=2) }
            relation { Plaque.south on Slab.north facing=up }
        }
        print Wall");
        let p = f["Plaque"];
        assert!(near(north(&p), Vec3::Z, 1e-9), "mated outward along +Z: {:?}", north(&p));
        assert!(near(p.rot.col(1), Vec3::Y, 1e-9), "its top fronts up: {:?}", p.rot.col(1));
        let f = frames("thing Wall {
            part Slab   { shape = box(width=100, height=100, depth=10) }
            part Plaque { shape = box(width=30, height=20, depth=2) }
            relation { Plaque.south on Slab.north facing=east }
        }
        print Wall");
        assert!(near(f["Plaque"].rot.col(1), Vec3::X, 1e-9), "top fronts east: {:?}", f["Plaque"].rot.col(1));
    }

    /// A source `facing=east` mirrors to a front facing west on its image.
    #[test]
    fn a_mirror_image_faces_mirrored() {
        let f = frames("thing T {
            part Body { shape = box(width=40, height=40, depth=40) }
            part ArmR { shape = box(width=6, height=30, depth=2) }
            part ArmL { shape = box(width=6, height=30, depth=2) }
            relation {
                ArmR.bottom on Body.top shift=(10, 0) facing=east
                ArmL symmetric_across Body from=ArmR
            }
        }
        print T");
        assert!(near(north(&f["ArmR"]), Vec3::X, 1e-9));
        assert!(near(north(&f["ArmL"]), WEST, 1e-9), "image fronts {:?}", north(&f["ArmL"]));
    }

    /// A `revolve` on twist turns about the axis from the faced rest: its
    /// first key is the faced rest, and a quarter second later the part has
    /// turned rate · t about its axis.
    #[test]
    fn a_revolve_on_twist_turns_from_the_faced_rest() {
        let src = "thing Top {
            part Plate { shape = box(width=200, height=10, depth=100) }
            part Fin   { shape = box(width=20, height=40, depth=4) }
            relation { Fin.bottom on Plate.top facing=east }
            revolve Spin { Fin twist=90 }
        }
        print Top";
        let scene = compile_to_scene(src).expect("compiles");
        let layer = &scene.layers[0];
        let fin = layer.parts.iter().find(|p| p.name == "Fin").unwrap();
        let rest = fin.joint.as_ref().unwrap().frame.clone();
        let anim = layer.animations.iter().find(|a| a.name == "Spin").expect("the revolve");
        let k0 = &anim.keys[0];
        assert_eq!(k0.time, 0.0);
        assert_eq!(k0.joints["Fin"].rot, rest.rot, "the clip starts at the faced rest");
        let k1 = &anim.keys[1];
        let r1 = Mat3(k1.joints["Fin"].rot);
        let turned = Mat3(rest.rot).apply(Vec3::Z).dot(r1.apply(Vec3::Z)).clamp(-1.0, 1.0).acos().to_degrees();
        assert!((turned - 90.0 * k1.time).abs() < 1e-6, "turned {turned}° by t={}", k1.time);
        assert!(near(r1.col(1), Mat3(rest.rot).col(1), 1e-9), "about its axis");
    }

    /// `aim` tips, `facing` swivels: a full look-at is the two together.
    #[test]
    fn aim_then_facing_is_a_look_at() {
        let f = frames("thing T {
            part Neck { shape = cylinder(height=40, radius=5) }
            part Head { shape = box(width=20, height=20, depth=10) }
            relation { Head.bottom on Neck.top aim=east facing=up }
        }
        print T");
        assert!(near(axis(&f["Head"]), Vec3::X, 1e-9));
        assert!(near(north(&f["Head"]), Vec3::Y, 1e-9), "front {:?}", north(&f["Head"]));
    }

    /// A part target faces the part, across the axis; a pose that moves the
    /// target turns the face.
    #[test]
    fn facing_a_part_follows_it_through_a_pose() {
        let src = "thing Stage {
            part Floor { shape = box(width=300, height=10, depth=300) }
            part Head  { shape = box(width=20, height=30, depth=10) }
            part Pole  { shape = cylinder(height=80, radius=2) }
            part Angel { shape = sphere(radius=10) }
            relation {
                Head.bottom  on Floor.top facing=Angel
                Pole.bottom  on Floor.top shift=(100, 0) lean=(0, 40)
                Angel.center on Pole.top
            }
            pose Swing { Pole lean=(0, -40) }
        }
        print Stage";
        for pose in [None, Some("Swing")] {
            let f = frames_posed(src, pose);
            let to = f["Angel"].pos.sub(f["Head"].pos);
            let across = Vec3::new(to.x, 0.0, to.z).normalize().unwrap();
            assert!(near(north(&f["Head"]), across, 1e-9), "{pose:?}: {:?} vs {across:?}", north(&f["Head"]));
        }
    }

    /// Inside an instance, facing words are the thing's own and turn with it.
    #[test]
    fn facing_inside_an_instance_turns_with_it() {
        let spike = |q: &str| format!("thing Spike {{
            part Foot {{ shape = box(width=10, height=2, depth=10) }}
            part Fin  {{ shape = box(width=8, height=20, depth=1) }}
            relation {{ Fin.bottom on Foot.top {q} }}
        }}");
        for q in ["facing=east", "facing=[30, 5, 30]", "aim=north facing=up"] {
            let board = format!("{}
            thing Board {{
                part Wall {{ shape = box(width=100, height=100, depth=10) }}
                part S    {{ thing = Spike }}
                relation {{ S.bottom on Wall.north }}
            }}
            print Board", spike(q));
            let f = frames(&board);
            let g = frames(&format!("{}\nprint Spike", spike(q)));
            let local = f["S.Foot"].inverse().compose(&f["S.Fin"]);
            let alone = g["Foot"].inverse().compose(&g["Fin"]);
            assert!(same(&local, &alone), "{q}");
        }
    }

    /// A pose may set `facing` — substitution, like every qualifier.
    #[test]
    fn a_pose_may_set_facing_by_substitution() {
        let src = |fin: &str, pose: &str| format!("thing T {{
            part Plate {{ shape = box(width=200, height=10, depth=100) }}
            part Fin   {{ shape = box(width=20, height=40, depth=4) }}
            relation {{ Fin.bottom on Plate.top {fin} }}
            {pose}
        }}
        print T");
        for (rest, posed) in [("facing=north", "facing=west"), ("", "facing=east twist=10"), ("aim=north", "facing=up")] {
            let p = frames_posed(&src(rest, &format!("pose P {{ Fin {posed} }}")), Some("P"));
            let q = frames(&src(&format!("{rest} {posed}").replace("facing=north facing=west", "facing=west"), ""));
            assert!(same(&p["Fin"], &q["Fin"]), "pose {{ Fin {posed} }} over `{rest}`");
        }
    }

    #[test]
    fn facing_errors_name_the_fix() {
        let m = errors(&choir("facing=up"));
        assert!(m.contains("on part 'Up': faces up, which is its own axis; name a direction across it \
                            (north, east, …)"), "{m}");
        let m = errors(&choir("facing=nrth"));
        assert!(m.contains("facing of 'Up': 'nrth' is neither a direction (north, south, east, west, up, \
                            down) nor a part of 'Choir'"), "{m}");
        let m = errors("thing T {
            part Body  { shape = box(width=40, height=60, depth=30) }
            part ArmR  { shape = capsule(height=30, radius=3) }
            part HandR { shape = sphere(radius=5) }
            relation {
                ArmR.bottom on Body.east facing=HandR
                HandR.center on ArmR.top
            }
        }
        print T");
        assert!(m.contains("'ArmR' faces HandR.center, but HandR hangs off ArmR (ArmR → HandR)"), "{m}");
        let m = errors("thing T {
            part A { shape = sphere(radius=5) }
            part B { shape = sphere(radius=2) }
            relation { B.center on A.top facing=north }
        }
        print T");
        assert!(m.contains("`facing` needs an oriented anchor on both sides"), "{m}");
    }
}
