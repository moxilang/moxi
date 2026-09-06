//! `moxi scene` — the canonical intermediate representation.
//!
//! The solved scene is a list of (shape, parameters, frame, material) per
//! part. Today the rasterizer, the mesher, and the raymarcher all read
//! this list; a sphere leaves the compiler as a sphere.
//!
//! # Contract
//!
//! This is a PUBLIC schema. `schema` is bumped on any incompatible change.
//! Frames are rigid transforms in each thing's own solved space, world
//! units, +Y up. No layer-centering offset is applied — positions are
//! exactly where relations put them.
//!
//! Not yet in the scene: lists, strings (Phase D.2) — which is also why
//! lathe and sweep primitives (profile-as-list) are deferred rather than
//! shoehorned in early.
//!
//! # Round trip
//!
//! `Shape::to_expr` inverts `Shape::from_expr`, and the pipeline test
//! `scene_round_trips_to_identical_voxels` rasterizes the scene and
//! compares against the direct path. If a new shape variant does not
//! round-trip, that test is what fails.

use serde::{Deserialize, Serialize};

use crate::ast::{Expr, Ident, NamedArg, ShapeExpr};
use crate::error::Span;
use crate::frame::{Frame, Mat3, Vec3};
use crate::geometry::{arg_f64, arg_i64, arg_str};

pub const SCHEMA: u32 = 1;

// ── Types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub version: String,
    pub schema:  u32,
    /// Printed things, in print order. Templates and generator targets
    /// are components, not layers, and are not listed.
    pub layers:  Vec<Layer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub thing:      String,
    pub voxel_size: f64,
    pub parts:      Vec<Part>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    /// Flattened name, e.g. `RightArm.Humerus`.
    pub name:  String,
    pub shape: Shape,
    pub frame: FrameOut,
    /// Resolved hex color, e.g. `#fffff0`.
    pub color: String,
}

/// `p_world = rot · p_local + pos`. `rot` is row-major, matching `Mat3`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameOut {
    pub pos: [f64; 3],
    pub rot: [[f64; 3]; 3],
}

impl FrameOut {
    pub fn from_frame(f: &Frame) -> Self {
        FrameOut { pos: [f.pos.x, f.pos.y, f.pos.z], rot: f.rot.0 }
    }

    pub fn to_frame(&self) -> Frame {
        Frame::new(Mat3(self.rot), Vec3::new(self.pos[0], self.pos[1], self.pos[2]))
    }
}

/// Every `ShapeExpr` variant with its arguments FOLDED to numbers. The
/// defaults here are the same ones `anchors::analytic_extents` and
/// `geometry::contains`/`distance` use; `from_expr` reads them through
/// the same `arg_f64` helper so they cannot disagree.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Shape {
    Sphere      { radius: f64 },
    Cylinder    { height: f64, radius: f64 },
    Box         { width: f64, height: f64, depth: f64, round: f64 },
    Cone        { height: f64, radius: f64 },
    Ellipsoid   { rx: f64, ry: f64, rz: f64 },
    Blob        { radius: f64, roughness: f64 },
    Heightfield { radius: f64, max_height: f64, noise: f64, seed: i64 },
    Capsule     { height: f64, radius: f64 },
    Torus       { major_radius: f64, minor_radius: f64 },
    Shell       { inner: std::boxed::Box<Shape>, inner_offset: f64 },
    Extrude     { profile: std::boxed::Box<Shape>, height: f64 },
    Union       { shapes: Vec<Shape>, blend: f64 },
    Intersect   { shapes: Vec<Shape> },
    Difference  { base: std::boxed::Box<Shape>, cuts: Vec<Shape> },
    At          { inner: std::boxed::Box<Shape>, x: f64, y: f64, z: f64 },
    Spin        { inner: std::boxed::Box<Shape>, axis: String, degrees: f64 },
}

// ── AST ⇄ scene ────────────────────────────────────────────────────────

impl Shape {
    pub fn from_expr(s: &ShapeExpr) -> Shape {
        use ShapeExpr as E;
        match s {
            E::Sphere { args } => Shape::Sphere { radius: arg_f64(args, "radius", 1.0) },
            E::Cylinder { args } => Shape::Cylinder {
                height: arg_f64(args, "height", 1.0),
                radius: arg_f64(args, "radius", 0.5),
            },
            E::Box_ { args } => Shape::Box {
                width:  arg_f64(args, "width",  2.0),
                height: arg_f64(args, "height", 2.0),
                depth:  arg_f64(args, "depth",  2.0),
                round:  arg_f64(args, "round",  0.0),
            },
            E::Cone { args } => Shape::Cone {
                height: arg_f64(args, "height", 1.0),
                radius: arg_f64(args, "radius", 0.5),
            },
            E::Ellipsoid { args } => Shape::Ellipsoid {
                rx: arg_f64(args, "rx", 1.0),
                ry: arg_f64(args, "ry", 1.0),
                rz: arg_f64(args, "rz", 1.0),
            },
            E::Blob { args } => Shape::Blob {
                radius:    arg_f64(args, "radius",    1.0),
                roughness: arg_f64(args, "roughness", 0.2),
            },
            E::Heightfield { args } => Shape::Heightfield {
                radius:     arg_f64(args, "radius",     50.0),
                max_height: arg_f64(args, "max_height", 20.0),
                noise:      arg_f64(args, "noise",      0.3),
                seed:       arg_i64(args, "seed",       42),
            },
            E::Capsule { args } => Shape::Capsule {
                height: arg_f64(args, "height", 1.0),
                radius: arg_f64(args, "radius", 0.5),
            },
            E::Torus { args } => Shape::Torus {
                major_radius: arg_f64(args, "major_radius", 2.0),
                minor_radius: arg_f64(args, "minor_radius", 0.5),
            },
            E::Shell { inner, args } => Shape::Shell {
                inner:        Box::new(Shape::from_expr(inner)),
                inner_offset: arg_f64(args, "inner_offset", 1.0),
            },
            E::Extrude { profile, args } => Shape::Extrude {
                profile: Box::new(Shape::from_expr(profile)),
                height:  arg_f64(args, "height", 1.0),
            },
            E::Union { shapes, args } => Shape::Union {
                shapes: shapes.iter().map(Shape::from_expr).collect(),
                blend:  arg_f64(args, "blend", 0.0),
            },
            E::Intersect { shapes } => Shape::Intersect {
                shapes: shapes.iter().map(Shape::from_expr).collect(),
            },
            E::Difference { base, cuts } => Shape::Difference {
                base: Box::new(Shape::from_expr(base)),
                cuts: cuts.iter().map(Shape::from_expr).collect(),
            },
            E::At { inner, args } => Shape::At {
                inner: Box::new(Shape::from_expr(inner)),
                x: arg_f64(args, "x", 0.0),
                y: arg_f64(args, "y", 0.0),
                z: arg_f64(args, "z", 0.0),
            },
            E::Spin { inner, args } => Shape::Spin {
                inner:   Box::new(Shape::from_expr(inner)),
                axis:    arg_str(args, "axis").unwrap_or_else(|| "y".to_string()),
                degrees: arg_f64(args, "degrees", 0.0),
            },
        }
    }

    /// Inverse of `from_expr`. Every argument is emitted explicitly
    /// (except `round`/`blend`, which are only emitted when non-zero, to
    /// keep the common case free of no-op arguments), so the rebuilt
    /// expression does not depend on any default.
    pub fn to_expr(&self) -> ShapeExpr {
        use ShapeExpr as E;
        let f = |k: &str, v: f64| NamedArg { key: k.into(), value: Expr::Float(v) };
        let i = |k: &str, v: i64| NamedArg { key: k.into(), value: Expr::Int(v) };
        let id = |k: &str, v: &str| NamedArg {
            key:   k.into(),
            value: Expr::Ident(Ident { name: v.into(), span: Span::new(0, 0) }),
        };
        match self {
            Shape::Sphere { radius } => E::Sphere { args: vec![f("radius", *radius)] },
            Shape::Cylinder { height, radius } =>
                E::Cylinder { args: vec![f("height", *height), f("radius", *radius)] },
            Shape::Box { width, height, depth, round } => E::Box_ {
                args: {
                    let mut a = vec![f("width", *width), f("height", *height), f("depth", *depth)];
                    if *round > 0.0 { a.push(f("round", *round)); }
                    a
                },
            },
            Shape::Cone { height, radius } =>
                E::Cone { args: vec![f("height", *height), f("radius", *radius)] },
            Shape::Ellipsoid { rx, ry, rz } =>
                E::Ellipsoid { args: vec![f("rx", *rx), f("ry", *ry), f("rz", *rz)] },
            Shape::Blob { radius, roughness } =>
                E::Blob { args: vec![f("radius", *radius), f("roughness", *roughness)] },
            Shape::Heightfield { radius, max_height, noise, seed } => E::Heightfield {
                args: vec![
                    f("radius", *radius), f("max_height", *max_height),
                    f("noise", *noise), i("seed", *seed),
                ],
            },
            Shape::Capsule { height, radius } =>
                E::Capsule { args: vec![f("height", *height), f("radius", *radius)] },
            Shape::Torus { major_radius, minor_radius } => E::Torus {
                args: vec![f("major_radius", *major_radius), f("minor_radius", *minor_radius)],
            },
            Shape::Shell { inner, inner_offset } => E::Shell {
                inner: Box::new(inner.to_expr()),
                args:  vec![f("inner_offset", *inner_offset)],
            },
            Shape::Extrude { profile, height } => E::Extrude {
                profile: Box::new(profile.to_expr()),
                args:    vec![f("height", *height)],
            },
            Shape::Union { shapes, blend } => E::Union {
                shapes: shapes.iter().map(Shape::to_expr).collect(),
                args:   if *blend > 0.0 { vec![f("blend", *blend)] } else { vec![] },
            },
            Shape::Intersect { shapes } =>
                E::Intersect { shapes: shapes.iter().map(Shape::to_expr).collect() },
            Shape::Difference { base, cuts } => E::Difference {
                base: Box::new(base.to_expr()),
                cuts: cuts.iter().map(Shape::to_expr).collect(),
            },
            Shape::At { inner, x, y, z } => E::At {
                inner: Box::new(inner.to_expr()),
                args:  vec![f("x", *x), f("y", *y), f("z", *z)],
            },
            Shape::Spin { inner, axis, degrees } => E::Spin {
                inner: Box::new(inner.to_expr()),
                args:  vec![id("axis", axis), f("degrees", *degrees)],
            },
        }
    }
}

impl Scene {
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("scene is always serializable")
    }

    pub fn from_json(s: &str) -> Result<Scene, serde_json::Error> {
        serde_json::from_str(s)
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::contains;

    fn na(k: &str, v: f64) -> NamedArg { NamedArg { key: k.into(), value: Expr::Float(v) } }

    /// A mug body — difference of cylinders with an `at` wrapper — and a
    /// spun cylinder survive the trip to `Shape` and back: the rebuilt
    /// expression answers `contains` identically at probe points.
    #[test]
    fn shape_round_trips_through_expr() {
        let mug = ShapeExpr::Difference {
            base: Box::new(ShapeExpr::Cylinder { args: vec![na("height", 10.0), na("radius", 5.0)] }),
            cuts: vec![ShapeExpr::At {
                inner: Box::new(ShapeExpr::Cylinder { args: vec![na("height", 10.0), na("radius", 4.0)] }),
                args:  vec![na("y", 1.0)],
            }],
        };
        let spun = ShapeExpr::Spin {
            inner: Box::new(ShapeExpr::Cylinder { args: vec![na("height", 8.0), na("radius", 1.0)] }),
            args:  vec![
                NamedArg { key: "axis".into(), value: Expr::Ident(Ident {
                    name: "z".into(), span: Span::new(1, 1),
                }) },
                na("degrees", 90.0),
            ],
        };

        for shape in [mug, spun] {
            let rebuilt = Shape::from_expr(&shape).to_expr();
            for p in [
                Vec3::ZERO, Vec3::new(4.5, 5.0, 0.0), Vec3::new(0.0, 0.5, 0.0),
                Vec3::new(-4.0, 0.0, 0.0), Vec3::new(4.0, 0.0, 0.0), Vec3::new(0.0, 4.0, 0.0),
            ] {
                assert_eq!(contains(&shape, p, 1.0), contains(&rebuilt, p, 1.0),
                           "disagreement at {p:?}");
            }
        }
    }

    /// Defaults fill in on the way out and are emitted explicitly on the
    /// way back — a bare `sphere()` becomes `sphere(radius=1)`.
    #[test]
    fn defaults_are_made_explicit() {
        let bare = ShapeExpr::Sphere { args: vec![] };
        let Shape::Sphere { radius } = Shape::from_expr(&bare) else { panic!() };
        assert_eq!(radius, 1.0);
        let ShapeExpr::Sphere { args } = Shape::from_expr(&bare).to_expr() else { panic!() };
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn frame_round_trips_exactly() {
        let f = Frame::new(Mat3::rot_y(0.7), Vec3::new(3.0, -2.0, 5.0));
        let back = FrameOut::from_frame(&f).to_frame();
        assert_eq!(f, back);
    }

    #[test]
    fn json_round_trips() {
        let scene = Scene {
            version: "test".into(),
            schema:  SCHEMA,
            layers:  vec![Layer {
                thing:      "E".into(),
                voxel_size: 1.0,
                parts: vec![Part {
                    name:  "P".into(),
                    shape: Shape::Sphere { radius: 2.0 },
                    frame: FrameOut::from_frame(&Frame::IDENTITY),
                    color: "#fffff0".into(),
                }],
            }],
        };
        let back = Scene::from_json(&scene.to_json_pretty()).unwrap();
        assert_eq!(back.layers[0].parts[0].name, "P");
        assert!(matches!(back.layers[0].parts[0].shape, Shape::Sphere { radius } if radius == 2.0));
    }

    /// Capsule, torus, and a rounded box round-trip through the IR, and
    /// `round`/`blend` at zero are OMITTED on the way back out — a
    /// plain box does not grow a spurious `round=0.0` argument.
    #[test]
    fn sculptor_primitives_round_trip() {
        let cap = ShapeExpr::Capsule { args: vec![na("height", 5.0), na("radius", 1.2)] };
        let tor = ShapeExpr::Torus { args: vec![na("major_radius", 3.0), na("minor_radius", 0.7)] };
        let rb  = ShapeExpr::Box_ { args: vec![na("width", 4.0), na("height", 4.0), na("depth", 4.0), na("round", 0.5)] };
        let plain_box = ShapeExpr::Box_ { args: vec![na("width", 2.0), na("height", 2.0), na("depth", 2.0)] };

        for shape in [cap, tor, rb] {
            let rebuilt = Shape::from_expr(&shape).to_expr();
            for p in [Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0), Vec3::new(2.0, 0.0, 0.0), Vec3::new(0.0, 3.0, 0.0)] {
                assert_eq!(contains(&shape, p, 1.0), contains(&rebuilt, p, 1.0), "disagreement at {p:?}");
            }
        }

        let ShapeExpr::Box_ { args } = Shape::from_expr(&plain_box).to_expr() else { panic!() };
        assert!(!args.iter().any(|a| a.key == "round"), "round=0 must not round-trip as a literal arg");
    }
}