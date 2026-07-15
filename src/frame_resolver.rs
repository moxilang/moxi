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
use crate::ast::{AnchorRef, Placement, RelationKind, RelationStmt, ShapeExpr};
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
    let mut errors: Vec<PlacementError> = Vec::new();

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
            Some(Placement::Align { object, .. }) => vec![object.part.clone()],
            Some(Placement::Mirror { source, plane, .. }) => {
                let mut d = vec![source.clone()];
                if plane.part != *source {
                    d.push(plane.part.clone());
                }
                d
            }
        }
    };

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
    let mut solved_count = 0usize;

    while let Some(name) = queue.pop() {
        solved_count += 1;

        let frame = match placement_of.get(name) {
            None => Frame::IDENTITY, // Rule 2: root
            Some(p) => match solve_one(p, &shape_of, &frames) {
                Ok(f) => f,
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

    // Rule 3: leftovers form a cycle. Walk it for the error message.
    if solved_count < parts.len() {
        let mut in_cycle: Vec<&str> = parts.iter()
            .map(|(n, _)| n.as_str())
            .filter(|n| !frames.contains_key(**&n))
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

    if errors.is_empty() { Ok(frames) } else { Err(errors) }
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
fn solve_one(
    placement: &Placement,
    shape_of:  &HashMap<&str, &ShapeExpr>,
    frames:    &FrameMap,
) -> Result<Frame, PlacementError> {
    match placement {
        Placement::Align { subject, object, twist, pitch, gap, span } => {
            let a_subj = lookup_anchor(subject, shape_of, *span)?;
            let a_obj  = lookup_anchor(object, shape_of, *span)?;

            let t_obj = frames.get(&object.part).copied().unwrap_or(Frame::IDENTITY);
            let w     = t_obj.compose(&a_obj.frame); // socket in world space

            let free = a_subj.kind == AnchorKind::Free || a_obj.kind == AnchorKind::Free;

            if free {
                // Inherit object rotation; coincide anchor points.
                let rot = t_obj.rot;
                let target = w.pos.add(w.rot.col(1).scale(*gap));
                let pos = target.sub(rot.apply(a_subj.frame.pos));
                Ok(Frame::new(rot, pos))
            } else {
                let adjust = Frame::from_pos(Vec3::new(0.0, *gap, 0.0))
                    .compose(&Frame::from_rot(Mat3::rot_y(twist.to_radians())))
                    .compose(&Frame::from_rot(Mat3::rot_x(pitch.to_radians())))
                    .compose(&Frame::from_rot(Mat3::FLIP_X));
                Ok(w.compose(&adjust).compose(&a_subj.frame.inverse()))
            }
        }

        Placement::Mirror { source, plane, axis, span, .. } => {
            let src = frames.get(source).copied().ok_or_else(|| PlacementError::UnknownPart {
                part: source.clone(), span: *span,
            })?;
            let a_plane = lookup_anchor(plane, shape_of, *span)?;
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
            let n  = t_plane.rot.apply(axis_vec);
            let m  = reflection_matrix(n);
            let p  = m.apply(src.pos.sub(w.pos)).add(w.pos);
            // Sandwich: M·R·M is a proper rotation (det M² = +1).
            let r  = m.mul(&src.rot).mul(&m);
            Ok(Frame::new(r, p))
        }
    }
}

fn lookup_anchor(
    r:        &AnchorRef,
    shape_of: &HashMap<&str, &ShapeExpr>,
    span:     Span,
) -> Result<Anchor, PlacementError> {
    let shape = shape_of.get(r.part.as_str()).ok_or_else(|| PlacementError::UnknownPart {
        part: r.part.clone(), span,
    })?;
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
    use crate::ast::{Expr, NamedArg};

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

    fn above(subject: &str, object: &str) -> Placement {
        Placement::Align {
            subject: aref(subject, "bottom"),
            object:  aref(object, "top"),
            twist: 0.0, pitch: 0.0, gap: 0.0,
            span: Span::new(1, 1),
        }
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
                twist: 0.0, pitch: 0.0, gap: 0.0,
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
            twist: 0.0, pitch: 0.0, gap: 0.0,
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
