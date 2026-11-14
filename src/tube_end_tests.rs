//! A tube mated by its end keeps its own sideways axes (Tighten, T5): the
//! acceptance pins of the issue in proposals/TIGHTEN.md.

use crate::anchors::resolve_anchor;
use crate::ast::{Expr, NamedArg};
use crate::frame::{Frame, Vec3};
use crate::pipeline::compile_to_scene;
use crate::scene::Scene;

fn scene(path: &str, mate: &str) -> Scene {
    let src = format!("```moxi\nmaterial M {{ color = \"#888888\" }}\n\
        thing T {{\n part G {{ shape = box(width=40, height=1, depth=40), material = M }}\n\
        part P {{ shape = tube(path={path}, radius=2), material = M }}\n\
        relation {{ {mate} }}\n resolve grain = 0.5 }}\nprint T\n```\n");
    compile_to_scene(&src).unwrap_or_else(|e| panic!("{e:?}"))
}

/// The placed tube's frame and its anchor `name(args)` in world space.
fn world(s: &Scene, name: &str, args: &[NamedArg]) -> (Frame, Frame) {
    let p = s.layers[0].parts.iter().find(|p| p.name == "P").expect("P");
    let f = p.frame.to_frame();
    let a = resolve_anchor(&p.shape.to_expr(), name, args).expect("anchor").frame;
    let w = Frame::new(f.rot.mul(&a.rot), f.apply_point(a.pos));
    (f, w)
}

fn near(a: Vec3, b: Vec3) -> bool { a.sub(b).length() < 1e-9 }
fn na(k: &str, v: f64) -> NamedArg { NamedArg { key: k.into(), value: Expr::Float(v) } }

const BENT: &str = "[[0, 0, 0], [0, 10, 0], [5, 20, 0]]";

#[test]
fn a_tube_stood_on_its_start_lands_unturned() {
    let s = scene(BENT, "P.start on G.top");
    let (f, _) = world(&s, "start", &[]);
    // the round start rests on the ground top (y = 0.5), so the path begins
    // one radius up and is otherwise where it was written
    assert!(near(f.apply_point(Vec3::new(0.0, 0.0, 0.0)), Vec3::new(0.0, 2.5, 0.0)), "{:?}", f.apply_point(Vec3::ZERO));
    assert!(near(f.apply_point(Vec3::new(5.0, 20.0, 0.0)), Vec3::new(5.0, 22.5, 0.0)), "{:?}", f.apply_point(Vec3::new(5.0, 20.0, 0.0)));
    // its angle-0 side points north, as a cylinder's does
    let (_, side) = world(&s, "side", &[na("t", 0.25), na("angle", 0.0)]);
    assert!(near(side.rot.col(1), Vec3::Z), "side(angle=0) points {:?}", side.rot.col(1));
}

#[test]
fn a_straight_tube_mates_by_its_ends_exactly_as_a_cylinder_does() {
    let tube = scene("[[0, 0, 0], [0, 8, 0]]", "P.start on G.top");
    let cyl = {
        let src = "```moxi\nmaterial M { color = \"#888888\" }\n\
            thing T {\n part G { shape = box(width=40, height=1, depth=40), material = M }\n\
            part P { shape = cylinder(radius=2, height=12), material = M }\n\
            relation { P.bottom on G.top }\n resolve grain = 0.5 }\nprint T\n```\n";
        compile_to_scene(src).unwrap_or_else(|e| panic!("{e:?}"))
    };
    let rot = |s: &Scene| s.layers[0].parts.iter().find(|p| p.name == "P").unwrap().frame.to_frame().rot;
    for k in 0..3 { assert!(near(rot(&tube).col(k), rot(&cyl).col(k)), "column {k}"); }
    // the anchors themselves, before any mate: start is a cylinder's bottom
    // and end its top, axis for axis
    let t = crate::ast::ShapeExpr::Tube { args: vec![
        NamedArg { key: "path".into(), value: list(&[[0.0, 0.0, 0.0], [0.0, 8.0, 0.0]]) }, na("radius", 2.0)] };
    let c = crate::ast::ShapeExpr::Cylinder { args: vec![na("radius", 2.0), na("height", 12.0)] };
    for (te, ce) in [("start", "bottom"), ("end", "top")] {
        let (a, b) = (resolve_anchor(&t, te, &[]).unwrap().frame, resolve_anchor(&c, ce, &[]).unwrap().frame);
        for k in 0..3 { assert!(near(a.rot.col(k), b.rot.col(k)), "{te} vs {ce}, column {k}"); }
    }
}

fn list(ps: &[[f64; 3]]) -> Expr {
    Expr::List(ps.iter().map(|p| Expr::List(p.iter().map(|v| Expr::Float(*v)).collect())).collect())
}

#[test]
fn every_tube_end_turns_by_the_shortest_rotation_from_the_cylinders() {
    // the end frame is the cylinder's top frame turned by the smallest
    // rotation taking +Y onto the end direction: X stays perpendicular to
    // the plane of the turn's axis, so X·(Y × d) = 0 is not required, but
    // the rotation axis Y × d is left fixed. Check that directly.
    let ends: [[f64; 3]; 4] = [[5.0, 20.0, 0.0], [0.0, 20.0, 7.0], [-3.0, 12.0, -4.0], [6.0, 10.0, 6.0]];
    for e in ends {
        let t = crate::ast::ShapeExpr::Tube { args: vec![
            NamedArg { key: "path".into(), value: list(&[[0.0, 0.0, 0.0], [0.0, 10.0, 0.0], e]) }, na("radius", 1.0)] };
        let tube = crate::tube::tube(match &t { crate::ast::ShapeExpr::Tube { args } => args, _ => unreachable!() });
        let tube = tube.as_ref().as_ref().unwrap();
        let d = tube.tan[tube.tan.len() - 1];
        let a = resolve_anchor(&t, "end", &[]).unwrap().frame;
        assert!(near(a.rot.col(1), d), "end points along the tube");
        // the rotation about k = Y × d by the angle between Y and d fixes k
        // and takes the cylinder's top frame (X, Y, Z) to (R X, d, R Z)
        let k = Vec3::Y.cross(d).normalize().unwrap();
        let c = Vec3::Y.dot(d);
        let s = Vec3::Y.cross(d).length();
        let rot = |v: Vec3| v.scale(c).add(k.cross(v).scale(s)).add(k.scale(k.dot(v) * (1.0 - c)));
        assert!(near(a.rot.col(0), rot(Vec3::X)), "{e:?}: x is {:?}, want {:?}", a.rot.col(0), rot(Vec3::X));
        assert!(near(a.rot.col(2), rot(Vec3::Z)), "{e:?}: z");
    }
}

#[test]
fn a_tube_drawn_downward_turns_half_about_its_own_x() {
    // the end points exactly down: the shortest rotation is not unique, and
    // the half turn about x is chosen: (X, Y, Z) → (X, −Y, −Z), which is a
    // cylinder's bottom frame
    let t = crate::ast::ShapeExpr::Tube { args: vec![
        NamedArg { key: "path".into(), value: list(&[[0.0, 0.0, 0.0], [0.0, -8.0, 0.0]]) }, na("radius", 1.0)] };
    let a = resolve_anchor(&t, "end", &[]).unwrap().frame;
    assert!(near(a.rot.col(0), Vec3::X) && near(a.rot.col(1), Vec3::Y.neg()) && near(a.rot.col(2), Vec3::Z.neg()),
        "{:?}", (a.rot.col(0), a.rot.col(1), a.rot.col(2)));
    // and its start, pointing up, is a cylinder's top frame turned the same way
    let a = resolve_anchor(&t, "start", &[]).unwrap().frame;
    assert!(near(a.rot.col(0), Vec3::X) && near(a.rot.col(1), Vec3::Y) && near(a.rot.col(2), Vec3::Z),
        "{:?}", (a.rot.col(0), a.rot.col(1), a.rot.col(2)));
}
