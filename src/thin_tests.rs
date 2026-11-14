//! A part that is not drawn is refused (Tighten, T3): the acceptance tests
//! of the issue in proposals/TIGHTEN.md.

use crate::ast::{Expr, NamedArg, ShapeExpr};
use crate::mesh::{auto_cell, surface_nets, surface_nets_with, vanishes, Sampling};
use crate::pipeline::compile_to_scene;

fn cylinder_at_grain_4(radius: f64) -> Result<(), String> {
    let src = format!("```moxi\nmaterial M {{ color = \"#888888\" }}\n\
        thing T {{ part Root {{ shape = cylinder(radius={radius}, height=10), material = M }}\n\
        resolve grain = 4 }}\nprint T\n```\n");
    compile_to_scene(&src).map(|_| ())
        .map_err(|es| es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n"))
}

#[test]
fn a_cylinder_thinner_than_the_mesh_grid_is_refused_and_says_how_to_fix_it() {
    // grain 4 meshes this small part at cell 2; diameter 1.8 slips between
    // the grid's sample points, so the exported mesh would be empty
    let why = cylinder_at_grain_4(0.9).expect_err("refused");
    assert!(why.contains("'Root' is too thin to be drawn"), "{why}");
    assert!(why.contains("thinnest side is 1.8"), "{why}");
    assert!(why.contains("mesh cell here is 2 (half the grain 4)"), "{why}");
    assert!(why.contains("at least 4 thick"), "{why}");
    // diameter 2.6 catches a sample point at cell 2, and 5 always does
    cylinder_at_grain_4(1.3).expect("drawn");
    cylinder_at_grain_4(2.5).expect("drawn");
}

fn na(k: &str, v: f64) -> NamedArg { NamedArg { key: k.into(), value: Expr::Float(v) } }

/// A small deterministic generator, so the test needs no crate and a
/// failure is the same failure every run.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 { a + (b - a) * self.next() }
}

/// A random thin part: its thin side is around one mesh cell (0.5 at
/// grain 1), the size where it may or may not catch a sample point.
fn thin_part(r: &mut Lcg) -> ShapeExpr {
    let t = |r: &mut Lcg| r.range(0.1, 1.3);
    let big = |r: &mut Lcg| r.range(1.0, 6.0);
    let shape = match (r.next() * 7.0) as usize {
        0 => ShapeExpr::Box_ { args: vec![na("width", t(r)), na("height", big(r)), na("depth", big(r))] },
        1 => ShapeExpr::Sphere { args: vec![na("radius", t(r) / 2.0)] },
        2 => ShapeExpr::Cylinder { args: vec![na("radius", t(r) / 2.0), na("height", big(r))] },
        3 => ShapeExpr::Torus { args: vec![na("major_radius", big(r)), na("minor_radius", t(r) / 2.0)] },
        4 => ShapeExpr::Capsule { args: vec![na("height", big(r)), na("radius", t(r) / 2.0)] },
        // two slivers apart: the box around them is wide, the parts are not
        5 => ShapeExpr::Union { shapes: vec![
                ShapeExpr::Box_ { args: vec![na("width", t(r)), na("height", big(r)), na("depth", big(r))] },
                ShapeExpr::At { inner: Box::new(ShapeExpr::Sphere { args: vec![na("radius", t(r) / 2.0)] }),
                                args: vec![na("x", big(r)), na("y", r.range(-1.0, 1.0))] },
             ], args: vec![] },
        // a hollowed box whose wall is thin: its box is fat, the part is not
        _ => {
            let (w, h, d, wall) = (big(r) + 1.0, big(r) + 1.0, big(r) + 1.0, t(r) / 2.0);
            ShapeExpr::Difference {
                base: Box::new(ShapeExpr::Box_ { args: vec![na("width", w), na("height", h), na("depth", d)] }),
                cuts: vec![ShapeExpr::Box_ { args: vec![na("width", w - 2.0 * wall), na("height", h - 2.0 * wall), na("depth", d - 2.0 * wall)] }],
                blend: 0.0,
            }
        }
    };
    // an off-grid place: the mesher's grid starts at the part's own box,
    // so moving a part must not change the answer
    ShapeExpr::At { inner: Box::new(shape),
        args: vec![na("x", r.range(-3.0, 3.0)), na("y", r.range(-3.0, 3.0)), na("z", r.range(-3.0, 3.0))] }
}

#[test]
fn the_check_refuses_exactly_the_parts_whose_mesh_is_empty() {
    let mut r = Lcg(0x5EED_0073);
    let (mut empty, mut drawn) = (0, 0);
    for i in 0..200 {
        let shape = thin_part(&mut r);
        let cell = auto_cell(&shape, 1.0);
        let mesh_empty = surface_nets(&shape, cell, 1.0).indices.is_empty();
        assert_eq!(vanishes(&shape, cell, 1.0), mesh_empty, "part {i}: {shape:?}");
        // the sparse mesher skips blocks it proves empty; dense must agree
        assert_eq!(surface_nets_with(&shape, cell, 1.0, Sampling::Dense).indices.is_empty(), mesh_empty, "part {i}");
        if mesh_empty { empty += 1 } else { drawn += 1 }
    }
    // both answers are exercised, or the test proves nothing
    assert!(empty >= 20 && drawn >= 20, "empty {empty}, drawn {drawn}");
}
