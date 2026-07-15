// src/anchors.rs
//
// The anchor vocabulary: every shape exposes named frames ("sockets"),
// derived ANALYTICALLY from its parameters — never from voxel data.
//
// Convention (enforced by frame_from_normal, defined in one place):
//   +Y = outward normal (the direction "away from the shape")
//   +X = tangent reference — for radial/surface anchors, the meridian
//        (up the shape's axis); for caps and faces, local +X
//   +Z = X × Y, right-handed
//
// Anchors are SEMANTIC, not geometric-exact: blob's `top` is the top of
// its nominal sphere. Determinism and meaning beat millimeter fidelity.
//
// Shape-local origins match the stampers in geometry/mod.rs:
//   centered at origin : sphere, ellipsoid, blob, box
//   base at origin     : cylinder, cone, heightfield, extrude
//   shell              : same as its inner shape

use crate::ast::{NamedArg, ShapeExpr};
use crate::frame::{frame_from_normal, Frame, Vec3};
use crate::geometry::{arg_f64, arg_i64};

// ── Public types ───────────────────────────────────────────────────────────

/// Oriented anchors participate fully in the mate (normals oppose, tangents
/// align). Free anchors (`center`) carry position only: the mate skips the
/// flip and the subject INHERITS the object part's rotation — rotate the
/// spine and the ribcage surrounding it rotates with it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnchorKind {
    Oriented,
    Free,
}

#[derive(Debug, Clone, Copy)]
pub struct Anchor {
    pub frame: Frame, // shape-local
    pub kind:  AnchorKind,
}

#[derive(Debug, Clone)]
pub enum AnchorError {
    /// Unknown anchor name for this shape. Carries the valid names so the
    /// resolver can print a suggestion.
    Undefined { anchor: String, shape: &'static str, valid: Vec<&'static str> },
    /// Anchor exists but a required argument is missing or out of range.
    BadArgs { anchor: String, message: String },
}

// ── Analytic extents ───────────────────────────────────────────────────────
//
// Shape-local axis-aligned bounds, from parameters. Used for:
//   • compass anchors (north/south/east/west) on any shape
//   • constraint checking on solved world frames
//   • sizing the stamp region at realization time
//
// This is the analytic replacement for BBox::from_part — placement no
// longer reads voxel grids.

#[derive(Debug, Clone, Copy)]
pub struct Extents {
    pub min: Vec3,
    pub max: Vec3,
}

impl Extents {
    pub fn center(&self) -> Vec3 {
        self.min.add(self.max).scale(0.5)
    }
}

pub fn analytic_extents(shape: &ShapeExpr) -> Extents {
    match shape {
        ShapeExpr::Sphere { args } => {
            let r = arg_f64(args, "radius", 1.0);
            centered(r, r, r)
        }
        ShapeExpr::Ellipsoid { args } => {
            let rx = arg_f64(args, "rx", 1.0);
            let ry = arg_f64(args, "ry", 1.0);
            let rz = arg_f64(args, "rz", 1.0);
            centered(rx, ry, rz)
        }
        ShapeExpr::Blob { args } => {
            // Nominal sphere — noise perturbs geometry, never anchors/extents.
            let r = arg_f64(args, "radius", 1.0);
            centered(r, r, r)
        }
        ShapeExpr::Box_ { args } => {
            let w = arg_f64(args, "width", 2.0);
            let h = arg_f64(args, "height", 2.0);
            let d = arg_f64(args, "depth", 2.0);
            centered(w / 2.0, h / 2.0, d / 2.0)
        }
        ShapeExpr::Cylinder { args } | ShapeExpr::Cone { args } => {
            let h = arg_f64(args, "height", 1.0);
            let r = arg_f64(args, "radius", 0.5);
            Extents { min: Vec3::new(-r, 0.0, -r), max: Vec3::new(r, h, r) }
        }
        ShapeExpr::Heightfield { args } => {
            let r  = arg_f64(args, "radius", 50.0);
            let mh = arg_f64(args, "max_height", 20.0);
            Extents { min: Vec3::new(-r, 0.0, -r), max: Vec3::new(r, mh, r) }
        }
        ShapeExpr::Shell { inner, .. } => analytic_extents(inner),
        ShapeExpr::Extrude { profile, args } => {
            let p = analytic_extents(profile);
            let h = arg_f64(args, "height", 1.0);
            Extents {
                min: Vec3::new(p.min.x, 0.0, p.min.z),
                max: Vec3::new(p.max.x, h, p.max.z),
            }
        }
    }
}

fn centered(hx: f64, hy: f64, hz: f64) -> Extents {
    Extents { min: Vec3::new(-hx, -hy, -hz), max: Vec3::new(hx, hy, hz) }
}

// ── Anchor resolution ──────────────────────────────────────────────────────

/// Resolve a named anchor (with optional args) on a shape.
///
/// Universal anchors, available on EVERY shape via its analytic extents:
///   center                          — orientation-free
///   top, bottom                     — ±Y face centers, normals ±Y
///   north(+Z), south(−Z),
///   east(+X), west(−X)              — lateral face centers, radial normals
///
/// Shape-specific anchors refine these with true surface geometry
/// (a sphere's `east` sits ON the sphere with a radial normal; a box's
/// `east` is its face center). Parametric anchors take named args.
pub fn resolve_anchor(
    shape: &ShapeExpr,
    name:  &str,
    args:  &[NamedArg],
) -> Result<Anchor, AnchorError> {
    // Shape-specific vocabulary first — it shadows the universal fallback.
    match shape {
        ShapeExpr::Sphere { args: sargs } | ShapeExpr::Blob { args: sargs } => {
            let r = arg_f64(sargs, "radius", 1.0);
            if let Some(a) = sphere_anchor(r, name, args)? {
                return Ok(a);
            }
        }
        ShapeExpr::Ellipsoid { args: sargs } => {
            let rx = arg_f64(sargs, "rx", 1.0);
            let ry = arg_f64(sargs, "ry", 1.0);
            let rz = arg_f64(sargs, "rz", 1.0);
            if let Some(a) = ellipsoid_anchor(rx, ry, rz, name, args)? {
                return Ok(a);
            }
        }
        ShapeExpr::Cylinder { args: sargs } => {
            let h = arg_f64(sargs, "height", 1.0);
            let r = arg_f64(sargs, "radius", 0.5);
            if let Some(a) = cylinder_anchor(h, r, name, args)? {
                return Ok(a);
            }
        }
        ShapeExpr::Cone { args: sargs } => {
            let h = arg_f64(sargs, "height", 1.0);
            let r = arg_f64(sargs, "radius", 0.5);
            if let Some(a) = cone_anchor(h, r, name, args)? {
                return Ok(a);
            }
        }
        ShapeExpr::Heightfield { args: sargs } => {
            if let Some(a) = heightfield_anchor(sargs, name, args)? {
                return Ok(a);
            }
        }
        // Combinator rule: the first operand's anchors pass through.
        ShapeExpr::Shell { inner, .. } => {
            if let Ok(a) = resolve_anchor(inner, name, args) {
                return Ok(a);
            }
        }
        ShapeExpr::Extrude { .. } | ShapeExpr::Box_ { .. } => {}
    }

    // Universal fallback from analytic extents.
    extents_anchor(&analytic_extents(shape), name).ok_or_else(|| AnchorError::Undefined {
        anchor: name.to_string(),
        shape:  shape_name(shape),
        valid:  valid_anchor_names(shape),
    })
}

// ── Universal compass anchors (from extents) ──────────────────────────────

fn extents_anchor(e: &Extents, name: &str) -> Option<Anchor> {
    let c = e.center();
    let (pos, normal) = match name {
        "center" => {
            return Some(Anchor {
                frame: Frame::from_pos(c),
                kind:  AnchorKind::Free,
            });
        }
        "top"    => (Vec3::new(c.x, e.max.y, c.z), Vec3::Y),
        "bottom" => (Vec3::new(c.x, e.min.y, c.z), Vec3::Y.neg()),
        "north"  => (Vec3::new(c.x, c.y, e.max.z), Vec3::Z),
        "south"  => (Vec3::new(c.x, c.y, e.min.z), Vec3::Z.neg()),
        "east"   => (Vec3::new(e.max.x, c.y, c.z), Vec3::X),
        "west"   => (Vec3::new(e.min.x, c.y, c.z), Vec3::X.neg()),
        _ => return None,
    };
    Some(oriented(pos, normal))
}

fn oriented(pos: Vec3, normal: Vec3) -> Anchor {
    // Meridian convention: X points "up the shape" where that's meaningful.
    Anchor {
        frame: frame_from_normal(pos, normal, Vec3::Y),
        kind:  AnchorKind::Oriented,
    }
}

// ── Sphere ─────────────────────────────────────────────────────────────────

fn sphere_anchor(r: f64, name: &str, args: &[NamedArg]) -> Result<Option<Anchor>, AnchorError> {
    let dir = match name {
        "top"    => Some(Vec3::Y),
        "bottom" => Some(Vec3::Y.neg()),
        "north"  => Some(Vec3::Z),
        "south"  => Some(Vec3::Z.neg()),
        "east"   => Some(Vec3::X),
        "west"   => Some(Vec3::X.neg()),
        "surface" => {
            // surface(yaw, pitch) in degrees. yaw about +Y from +Z;
            // pitch from the equator toward +Y.
            let yaw   = arg_f64(args, "yaw",   0.0).to_radians();
            let pitch = arg_f64(args, "pitch", 0.0).to_radians();
            Some(Vec3::new(
                pitch.cos() * yaw.sin(),
                pitch.sin(),
                pitch.cos() * yaw.cos(),
            ))
        }
        _ => None,
    };

    Ok(dir.map(|d| oriented(d.scale(r), d)))
}

// ── Ellipsoid ──────────────────────────────────────────────────────────────
//
// Like the sphere, but the surface normal of x²/rx² + y²/ry² + z²/rz² = 1
// at point p is (p.x/rx², p.y/ry², p.z/rz²) — NOT the radial direction.

fn ellipsoid_anchor(
    rx: f64, ry: f64, rz: f64,
    name: &str, args: &[NamedArg],
) -> Result<Option<Anchor>, AnchorError> {
    let dir = match name {
        "top"    => Some(Vec3::Y),
        "bottom" => Some(Vec3::Y.neg()),
        "north"  => Some(Vec3::Z),
        "south"  => Some(Vec3::Z.neg()),
        "east"   => Some(Vec3::X),
        "west"   => Some(Vec3::X.neg()),
        "surface" => {
            let yaw   = arg_f64(args, "yaw",   0.0).to_radians();
            let pitch = arg_f64(args, "pitch", 0.0).to_radians();
            Some(Vec3::new(
                pitch.cos() * yaw.sin(),
                pitch.sin(),
                pitch.cos() * yaw.cos(),
            ))
        }
        _ => None,
    };

    Ok(dir.map(|d| {
        let pos = Vec3::new(d.x * rx, d.y * ry, d.z * rz);
        let normal = Vec3::new(pos.x / (rx * rx), pos.y / (ry * ry), pos.z / (rz * rz));
        oriented(pos, normal)
    }))
}

// ── Cylinder ───────────────────────────────────────────────────────────────
//
// Base at origin, axis +Y, height h, radius r.

fn cylinder_anchor(h: f64, r: f64, name: &str, args: &[NamedArg]) -> Result<Option<Anchor>, AnchorError> {
    Ok(match name {
        "top"    => Some(oriented(Vec3::new(0.0, h, 0.0), Vec3::Y)),
        "bottom" => Some(oriented(Vec3::ZERO, Vec3::Y.neg())),

        // side(t, angle): t ∈ [0,1] along the axis, angle in degrees about
        // +Y from +Z. Radial outward normal; meridian X points up the axis.
        // At twist 0 a mated part sticks straight out, orthogonal to the
        // axis, with its own "up" facing up the trunk.
        "side" => {
            let t     = arg_f64(args, "t", 0.5);
            let angle = arg_f64(args, "angle", 0.0).to_radians();
            if !(0.0..=1.0).contains(&t) {
                return Err(AnchorError::BadArgs {
                    anchor:  "side".to_string(),
                    message: format!("t must be in [0, 1], got {t}"),
                });
            }
            let radial = Vec3::new(angle.sin(), 0.0, angle.cos());
            Some(oriented(radial.scale(r).add(Vec3::new(0.0, t * h, 0.0)), radial))
        }

        // rim_top / rim_bottom(angle): the cap edge. Normal along the cap
        // (±Y) so mated parts stand on the rim; position on the circle.
        "rim_top" | "rim_bottom" => {
            let angle  = arg_f64(args, "angle", 0.0).to_radians();
            let radial = Vec3::new(angle.sin(), 0.0, angle.cos());
            let (y, n) = if name == "rim_top" { (h, Vec3::Y) } else { (0.0, Vec3::Y.neg()) };
            Some(Anchor {
                // X hint = radial, so twist 0 faces outward from the rim.
                frame: frame_from_normal(radial.scale(r).add(Vec3::new(0.0, y, 0.0)), n, radial),
                kind:  AnchorKind::Oriented,
            })
        }

        _ => None,
    })
}

// ── Cone ───────────────────────────────────────────────────────────────────
//
// Base at origin, apex at (0, h, 0). The slant surface at parameter t has
// radius r·(1−t) and an outward normal tilted by the half-angle.

fn cone_anchor(h: f64, r: f64, name: &str, args: &[NamedArg]) -> Result<Option<Anchor>, AnchorError> {
    Ok(match name {
        "apex" | "top" => Some(oriented(Vec3::new(0.0, h, 0.0), Vec3::Y)),
        "base" | "bottom" => Some(oriented(Vec3::ZERO, Vec3::Y.neg())),

        "side" => {
            let t     = arg_f64(args, "t", 0.5);
            let angle = arg_f64(args, "angle", 0.0).to_radians();
            if !(0.0..=1.0).contains(&t) {
                return Err(AnchorError::BadArgs {
                    anchor:  "side".to_string(),
                    message: format!("t must be in [0, 1], got {t}"),
                });
            }
            let radial = Vec3::new(angle.sin(), 0.0, angle.cos());
            let pos    = radial.scale(r * (1.0 - t)).add(Vec3::new(0.0, t * h, 0.0));
            // Slant normal: perpendicular to the slant line, in the
            // (radial, Y) plane. Slant direction = (−r, h) normalized in
            // that plane ⇒ normal = (h, r) normalized.
            let len    = (h * h + r * r).sqrt();
            let normal = radial.scale(h / len).add(Vec3::new(0.0, r / len, 0.0));
            Some(oriented(pos, normal))
        }

        _ => None,
    })
}

// ── Heightfield ────────────────────────────────────────────────────────────
//
// surface(x, z): the terrain point at that column, with the TERRAIN NORMAL
// from central differences of the same analytic elevation function the
// stamper uses. This is the anchor that makes generators non-special:
// a generator is just repeated `instance.base on Terrain.surface(x, z)`.
//
// NOTE: requires `terrain_noise` in geometry/mod.rs to become pub(crate)
// and to be shared verbatim between this function and stamp_heightfield —
// one elevation function, two consumers. See MIGRATION.md.

fn heightfield_anchor(
    sargs: &[NamedArg],
    name:  &str,
    args:  &[NamedArg],
) -> Result<Option<Anchor>, AnchorError> {
    if name != "surface" {
        return Ok(None);
    }

    let x = arg_f64(args, "x", 0.0);
    let z = arg_f64(args, "z", 0.0);

    let radius = arg_f64(sargs, "radius", 50.0);
    if (x * x + z * z).sqrt() > radius {
        return Err(AnchorError::BadArgs {
            anchor:  "surface".to_string(),
            message: format!("({x}, {z}) is outside the heightfield radius {radius}"),
        });
    }

    let elev = |px: f64, pz: f64| heightfield_elevation(sargs, px, pz);

    let y = elev(x, z);

    // Central differences, 1 world-unit step → surface normal.
    let step   = 1.0;
    let normal = Vec3::new(
        elev(x - step, z) - elev(x + step, z),
        2.0 * step,
        elev(x, z - step) - elev(x, z + step),
    );

    Ok(Some(oriented(Vec3::new(x, y, z), normal)))
}

/// Analytic elevation — MUST be the same formula stamp_heightfield uses
/// (edge fade × multi-octave noise × max_height), so anchors sit exactly
/// on the stamped surface. Shared via geometry::terrain_noise.
fn heightfield_elevation(sargs: &[NamedArg], x: f64, z: f64) -> f64 {
    let radius     = arg_f64(sargs, "radius", 50.0);
    let max_height = arg_f64(sargs, "max_height", 20.0);
    let noise_amt  = arg_f64(sargs, "noise", 0.3);
    let seed       = arg_i64(sargs, "seed", 42) as u64;

    let dist = (x * x + z * z).sqrt();
    if dist > radius {
        return 0.0;
    }
    let edge_fade = 1.0 - (dist / radius).powi(2);

    let n = crate::geometry::terrain_noise(x.round() as i32, z.round() as i32, seed, noise_amt);
    ((n * edge_fade) * max_height).round()
}

// ── Vocabulary tables (for errors and SKILL.md generation) ────────────────

pub fn shape_name(shape: &ShapeExpr) -> &'static str {
    match shape {
        ShapeExpr::Sphere { .. }      => "sphere",
        ShapeExpr::Cylinder { .. }    => "cylinder",
        ShapeExpr::Box_ { .. }        => "box",
        ShapeExpr::Cone { .. }        => "cone",
        ShapeExpr::Ellipsoid { .. }   => "ellipsoid",
        ShapeExpr::Blob { .. }        => "blob",
        ShapeExpr::Heightfield { .. } => "heightfield",
        ShapeExpr::Shell { .. }       => "shell",
        ShapeExpr::Extrude { .. }     => "extrude",
    }
}

const UNIVERSAL: &[&str] = &["center", "top", "bottom", "north", "south", "east", "west"];

/// The complete, enumerable anchor vocabulary for a shape. The resolver
/// uses this for `UndefinedAnchor` suggestions; a doc generator can emit
/// the SKILL.md attachment table from it directly.
pub fn valid_anchor_names(shape: &ShapeExpr) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = UNIVERSAL.to_vec();
    match shape {
        ShapeExpr::Sphere { .. } | ShapeExpr::Ellipsoid { .. } | ShapeExpr::Blob { .. } => {
            v.push("surface(yaw, pitch)");
        }
        ShapeExpr::Cylinder { .. } => {
            v.extend(["side(t, angle)", "rim_top(angle)", "rim_bottom(angle)"]);
        }
        ShapeExpr::Cone { .. } => {
            v.extend(["apex", "base", "side(t, angle)"]);
        }
        ShapeExpr::Heightfield { .. } => {
            v.push("surface(x, z)");
        }
        ShapeExpr::Shell { inner, .. } => {
            return valid_anchor_names(inner); // pass-through + universal
        }
        ShapeExpr::Box_ { .. } | ShapeExpr::Extrude { .. } => {}
    }
    v
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::ShapeExpr;

    fn sphere(r: f64) -> ShapeExpr {
        ShapeExpr::Sphere {
            args: vec![NamedArg { key: "radius".into(), value: crate::ast::Expr::Float(r) }],
        }
    }

    fn cylinder(h: f64, r: f64) -> ShapeExpr {
        ShapeExpr::Cylinder {
            args: vec![
                NamedArg { key: "height".into(), value: crate::ast::Expr::Float(h) },
                NamedArg { key: "radius".into(), value: crate::ast::Expr::Float(r) },
            ],
        }
    }

    #[test]
    fn sphere_bottom_points_down() {
        let a = resolve_anchor(&sphere(4.0), "bottom", &[]).unwrap();
        assert_eq!(a.kind, AnchorKind::Oriented);
        assert!((a.frame.pos.y + 4.0).abs() < 1e-9);
        let n = a.frame.rot.col(1); // +Y column = outward normal
        assert!((n.y + 1.0).abs() < 1e-9);
    }

    #[test]
    fn cylinder_side_is_radial_with_meridian_x() {
        let a = resolve_anchor(&cylinder(6.0, 0.6), "side", &[
            NamedArg { key: "t".into(),     value: crate::ast::Expr::Float(0.5) },
            NamedArg { key: "angle".into(), value: crate::ast::Expr::Float(0.0) },
        ]).unwrap();
        let n = a.frame.rot.col(1);
        let x = a.frame.rot.col(0);
        assert!((n.z - 1.0).abs() < 1e-9); // radial +Z at angle 0
        assert!((x.y - 1.0).abs() < 1e-9); // meridian: X points up the axis
    }

    #[test]
    fn center_is_orientation_free() {
        let a = resolve_anchor(&sphere(2.0), "center", &[]).unwrap();
        assert_eq!(a.kind, AnchorKind::Free);
    }

    #[test]
    fn unknown_anchor_lists_vocabulary() {
        match resolve_anchor(&sphere(2.0), "tp", &[]) {
            Err(AnchorError::Undefined { valid, .. }) => assert!(valid.contains(&"top")),
            other => panic!("expected Undefined, got {other:?}"),
        }
    }
}
