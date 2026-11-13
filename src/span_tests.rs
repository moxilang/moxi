//! Spans (Form, stage 3): the acceptance tests of the issue in
//! proposals/FORM.md. The solver math is checked here, by coordinates.

use crate::anchors::resolve_anchor;
use crate::ast::{Expr, NamedArg, ShapeExpr};
use crate::frame::{Frame, Vec3};
use crate::pipeline::{compile_to_scene, compile_to_scene_posed, solve_notes};
use crate::scene::{Part, Scene, Shape};
use crate::tube::Tube;

fn fence(body: &str) -> String {
    format!("```moxi\nmaterial M {{ color = \"#888888\" }}\n{body}\n```\n")
}
fn scene(body: &str) -> Scene {
    compile_to_scene(fence(body).as_str()).unwrap_or_else(|e| panic!("{e:?}"))
}
fn posed(body: &str, pose: &str) -> Scene {
    compile_to_scene_posed(fence(body).as_str(), Some(pose)).unwrap_or_else(|e| panic!("{e:?}"))
}
fn errors(body: &str) -> String {
    let es = compile_to_scene(fence(body).as_str()).expect_err("an error");
    es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n")
}
fn part<'a>(s: &'a Scene, name: &str) -> &'a Part {
    s.layers[0].parts.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no part {name}"))
}
fn path(p: &Part) -> Vec<Vec3> {
    let Shape::Tube { path, .. } = &p.shape else { panic!("{} is not a tube", p.name) };
    path.iter().map(|q| Vec3::new(q[0], q[1], q[2])).collect()
}
fn tube(p: &Part) -> Tube {
    let ShapeExpr::Tube { args } = p.shape.to_expr() else { panic!() };
    Tube::new(&args).unwrap()
}
fn close(a: Vec3, b: Vec3, tol: f64) -> bool { a.sub(b).length() <= tol }
/// An anchor of a scene part, in the world.
fn anchor(s: &Scene, name: &str, a: &str, args: &[(&str, f64)]) -> Frame {
    let p = part(s, name);
    let args: Vec<NamedArg> = args.iter().map(|(k, v)| NamedArg { key: (*k).into(), value: Expr::Float(*v) }).collect();
    p.frame.to_frame().compose(&resolve_anchor(&p.shape.to_expr(), a, &args).unwrap().frame)
}

const TWO_BOXES: &str = "thing T {
    part A    { shape = box(width=4, height=2, depth=4), material = M }
    part B    { shape = at(box(width=2, height=2, depth=2), x=10), material = M }
    part Arch { shape = tube(radius=0.5), material = M }
    relation {
        B.origin on A.origin
        Arch from A.top to B.top rise=2
    }
    resolve voxel_size = 0.5
}
print T";

#[test]
fn the_path_is_pinned() {
    // A.top = (0, 1, 0) and B.top = (10, 1, 0), both facing +Y; D = 10, h = 3.3;
    // Qa = (0, 4.3, 0), Qb = (10, 4.3, 0); the rise point is their midpoint
    // plus 2 up. The span's frame is A.top's: at (0, 1, 0), unrotated.
    let s = scene(TWO_BOXES);
    let arch = part(&s, "Arch");
    let f = arch.frame.to_frame();
    assert!(close(f.pos, Vec3::new(0.0, 1.0, 0.0), 1e-12), "{:?}", f.pos);
    let want = [[0.0, 0.0, 0.0], [0.0, 3.3, 0.0], [5.0, 5.3, 0.0], [10.0, 3.3, 0.0], [10.0, 0.0, 0.0]];
    let got = path(arch);
    assert_eq!(got.len(), want.len(), "{got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!(close(*g, Vec3::new(w[0], w[1], w[2]), 1e-9), "{got:?}");
    }
    // the report reads A→B in the on column
    let r = crate::report::from_source(fence(TWO_BOXES).as_str(), None).unwrap().to_text(false);
    assert!(r.contains("A→B"), "{r}");
}

/// Ends on a tipped part and on a curved side: the tube starts and ends
/// exactly on the anchors and leaves and arrives square to them.
const TILTED: &str = "thing T {
    part Base { shape = box(width=20, height=1, depth=20), material = M }
    part Post { shape = cylinder(height=8, radius=1), material = M }
    part Ball { shape = sphere(radius=3), material = M }
    part Arm  { shape = tube(radius=[0.6, 0.3]), material = M }
    relation {
        Post.bottom on Base.top shift=(-6, 2) lean=(25, 40)
        Ball.bottom on Base.top shift=(7, -3)
        Arm from Post.side(t=0.7, angle=30) to Ball.surface(yaw=200, pitch=20) reach=0.4
    }
    resolve voxel_size = 0.5
}
print T";

#[test]
fn a_span_starts_and_ends_on_its_anchors_square_to_them() {
    let s = scene(TILTED);
    let arm = part(&s, "Arm");
    let (f, t) = (arm.frame.to_frame(), tube(arm));
    let a = anchor(&s, "Post", "side", &[("t", 0.7), ("angle", 30.0)]);
    let b = anchor(&s, "Ball", "surface", &[("yaw", 200.0), ("pitch", 20.0)]);
    let (start, end) = (f.apply_point(t.pts[0]), f.apply_point(*t.pts.last().unwrap()));
    assert!(close(start, a.pos, 1e-9), "start {start:?} vs {:?}", a.pos);
    assert!(close(end, b.pos, 1e-9), "end {end:?} vs {:?}", b.pos);
    let (t0, t1) = (f.apply_dir(t.tan[0]), f.apply_dir(*t.tan.last().unwrap()));
    assert!(close(t0, a.rot.col(1), 1e-9), "leaves along the normal: {t0:?} vs {:?}", a.rot.col(1));
    assert!(close(t1, b.rot.col(1).neg(), 1e-9), "arrives against the normal: {t1:?} vs {:?}", b.rot.col(1));
}

#[test]
fn via_rise_and_reach_shape_the_path() {
    let base = |rel: &str| scene(&format!("thing T {{
        part A    {{ shape = box(width=4, height=2, depth=4), material = M }}
        part B    {{ shape = at(box(width=2, height=2, depth=2), x=10, z=4), material = M }}
        part Arch {{ shape = tube(radius=0.5), material = M }}
        relation {{ B.origin on A.origin
                   {rel} }}
        resolve voxel_size = 0.5 }}
        print T"));
    // via: the point is on the path (every given point is a sample)
    let s = base("Arch from A.top to B.top via=[4, 9, -2]");
    let (arch, f) = (part(&s, "Arch"), part(&s, "Arch").frame.to_frame());
    assert!(tube(arch).pts.iter().any(|p| close(f.apply_point(*p), Vec3::new(4.0, 9.0, -2.0), 1e-9)));
    // reach=0 and no middle: a straight two-point tube between the anchors
    let s = base("Arch from A.top to B.top reach=0");
    assert_eq!(path(part(&s, "Arch")).len(), 2);
    // `to` a point, and a `let`-bound point: no normal there, so no
    // straight-out point at that end
    let s = base("Arch from A.top to [3, 6, 1]");
    let p = path(part(&s, "Arch"));
    assert_eq!(p.len(), 3, "{p:?}");
    assert!(close(part(&s, "Arch").frame.to_frame().apply_point(p[2]), Vec3::new(3.0, 6.0, 1.0), 1e-9));
    // a bare part as an end is its center, which has no normal
    let s = base("Arch from A.top to B");
    let p = path(part(&s, "Arch"));
    assert_eq!(p.len(), 3);
    assert!(close(part(&s, "Arch").frame.to_frame().apply_point(p[2]), Vec3::new(10.0, 0.0, 4.0), 1e-9));
    // sag hangs the middle below the line between the straight-out points
    let s = base("Arch from A.top to B.top reach=0 sag=1.5");
    let p = path(part(&s, "Arch"));
    let f = part(&s, "Arch").frame.to_frame();
    assert!(close(f.apply_point(p[1]), Vec3::new(5.0, 1.0 - 1.5, 2.0), 1e-9), "{:?}", f.apply_point(p[1]));
    let s = scene("thing T {
        part A { shape = box(width=4, height=2, depth=4), material = M }
        part Arch { shape = tube(radius=0.5), material = M }
        let p = [6, 5, 0]
        relation { Arch from A.top to p }
        resolve voxel_size = 0.5 }
        print T");
    let a = part(&s, "Arch");
    assert!(close(a.frame.to_frame().apply_point(*path(a).last().unwrap()), Vec3::new(6.0, 5.0, 0.0), 1e-9));
}

#[test]
fn a_span_never_moves_the_parts_it_joins() {
    let with = scene(TILTED);
    let without = scene(&TILTED.replace("    part Arm  { shape = tube(radius=[0.6, 0.3]), material = M }\n", "")
        .replace("        Arm from Post.side(t=0.7, angle=30) to Ball.surface(yaw=200, pitch=20) reach=0.4\n", ""));
    for p in &without.layers[0].parts {
        let q = part(&with, &p.name);
        assert_eq!(format!("{:?}", p.frame), format!("{:?}", q.frame), "{} moved", p.name);
    }
}

#[test]
fn parts_mated_on_a_span_land_on_its_surface_in_any_line_order() {
    let src = |first: &str, second: &str| format!("thing T {{
        part A    {{ shape = box(width=4, height=2, depth=4), material = M }}
        part B    {{ shape = at(box(width=2, height=2, depth=2), x=10), material = M }}
        part Arch {{ shape = tube(radius=[0.8, 0.4]), material = M }}
        part Bud  {{ shape = sphere(radius=0.3), material = M }}
        relation {{
            B.origin on A.origin
            {first}
            {second}
        }}
        resolve voxel_size = 0.5 }}
        print T");
    let (span, bud) = ("Arch from A.top to B.top rise=2", "Bud.center on Arch.side(t=0.4, angle=60)");
    let a = scene(&src(span, bud));
    let b = scene(&src(bud, span));
    assert_eq!(format!("{:?}", part(&a, "Bud").frame), format!("{:?}", part(&b, "Bud").frame));
    let arch = part(&a, "Arch");
    let local = arch.frame.to_frame().inverse().apply_point(part(&a, "Bud").frame.to_frame().pos);
    let d = tube(arch).distance(local);
    assert!(d <= 1e-9 && d > -0.05 * 0.4, "the bud's center is on the tube: {d}");
}

#[test]
fn a_span_follows_its_ends_through_poses() {
    // both ends on one part: its local path is the same bits in every pose
    let handle = "thing T {
        part Base { shape = box(width=10, height=1, depth=10), material = M }
        part Cup  { shape = cylinder(height=6, radius=2), material = M }
        part Grip { shape = tube(radius=0.3), material = M }
        relation {
            Cup.bottom on Base.top pitch=0
            Grip from Cup.side(t=0.8, angle=90) to Cup.side(t=0.25, angle=90) reach=0.6
        }
        pose Tip { Cup pitch=35 }
        resolve voxel_size = 0.5
    }
    print T";
    let (rest, tip) = (scene(handle), posed(handle, "Tip"));
    let bits = |s: &Scene| path(part(s, "Grip")).iter().map(|p| [p.x.to_bits(), p.y.to_bits(), p.z.to_bits()]).collect::<Vec<_>>();
    assert_eq!(bits(&rest), bits(&tip), "the handle keeps its shape");
    assert_ne!(format!("{:?}", part(&rest, "Grip").frame), format!("{:?}", part(&tip, "Grip").frame), "and moves with the cup");
    assert!(!solve_notes(fence(handle).as_str(), None).unwrap().iter().any(|(_, n)| n.contains("changes shape")));
    // the joint parent is the `from` part, so a glTF node hangs off it
    assert_eq!(part(&rest, "Grip").joint.as_ref().unwrap().parent.as_deref(), Some("Cup"));

    // ends on two parts: the span's world end follows the moved one, and
    // `moxi check` notes that an animation keeps the rest shape
    let rope = "thing T {
        part Ground { shape = box(width=40, height=1, depth=6), material = M }
        part PostA  { shape = cylinder(height=10, radius=0.5), material = M }
        part PostB  { shape = cylinder(height=10, radius=0.5), material = M }
        part Rope   { shape = tube(radius=0.2), material = M }
        relation {
            PostA.bottom on Ground.top shift=(-10, 0)
            PostB.bottom on Ground.top shift=(10, 0)
            Rope from PostA.top to PostB.top reach=0 sag=2
        }
        pose Wide { PostB shift=(16, 0) }
        resolve voxel_size = 0.5
    }
    print T";
    let wide = posed(rope, "Wide");
    let r = part(&wide, "Rope");
    let end = r.frame.to_frame().apply_point(*path(r).last().unwrap());
    assert!(close(end, anchor(&wide, "PostB", "top", &[]).pos, 1e-9), "{end:?}");
    let notes = solve_notes(fence(rope).as_str(), None).unwrap();
    assert!(notes.iter().any(|(_, n)| n.contains("span 'Rope' changes shape in pose 'Wide'")), "{notes:?}");
    // glTF exports the posed shape: the posed scene builds a different mesh
    let g = crate::gltf::scene_to_gltf(&wide, None);
    assert!(g.json["nodes"].as_array().unwrap().iter().any(|n| n["extras"]["moxi"]["part"] == "Rope"));
}

#[test]
fn span_errors_name_the_part_and_the_fix() {
    let thing = |parts: &str, rels: &str| format!("thing T {{
        part A {{ shape = box(width=4, height=2, depth=4), material = M }}
        part B {{ shape = at(box(width=2, height=2, depth=2), x=10), material = M }}
        {parts}
        relation {{ B.origin on A.origin
                   {rels} }}
        resolve voxel_size = 0.5 }}
        print T");
    let tube = "part Arch { shape = tube(radius=0.5), material = M }";
    let cases: Vec<(String, &str)> = vec![
        (thing("part Arch { shape = sphere(radius=1), material = M }", "Arch from A.top to B.top"),
         "a span's part is a tube with no path"),
        (thing("part Arch { shape = tube(path=[[0,0,0],[1,1,1]], radius=0.5), material = M }", "Arch from A.top to B.top"),
         "drop its `path`"),
        (thing(tube, "Arch.bottom on A.top"), "or join two anchors with a span"),
        (thing("part Arch { shape = union(tube(radius=0.5), sphere(radius=1)), material = M }", "Arch.bottom on A.top"),
         "must be the part's whole shape"),
        (thing(tube, "Arch from A.top to B.top sag=1 rise=1"), "one middle per span"),
        (thing(tube, "Arch from A.top to B.top reach=1.5"), "a fraction of the distance between the ends"),
        (thing(tube, "Arch from A.top to B.top bend=2"), "a span qualifier: reach, sag, rise or via"),
        (thing(tube, "Arch from A.top to A.top"), "its two ends are the same point"),
        (thing(tube, "Arch from A.east to B.west reach=0.9"), "lower `reach`"),
        (thing(tube, "Arch from [0, 0, 0] to B.top"), "a span starts on a part or an anchor"),
        (thing(tube, "Arch from A.top to up"), "a part, an anchor or a point"),
        (thing(tube, "Arch from A.top to B.top\n Arch.bottom on A.top"), "Arch"),
        (thing(&format!("{tube}\n part Arch2 {{ shape = tube(radius=0.5), material = M }}"),
               "Arch from A.top to B.top\n Arch2 symmetric_across A from=Arch"), "a span is not mirrored"),
        (thing(tube, "Arch from A.top to B.top\n }\n pose P { Arch from A.top to B.top"), "spans in the thing's own"),
    ];
    for (src, want) in cases {
        let e = errors(&src);
        assert!(e.contains(want), "expected {want:?} in:\n{e}");
    }
}

#[test]
fn the_lantern_hangs_from_its_rope() {
    let src = include_str!("../scripts/LANTERN.md");
    let s = compile_to_scene(src).unwrap_or_else(|e| panic!("{e:?}"));
    for (name, from, to) in [("Rope", "PostA", Some("PostB")), ("Lamp.Handle", "Lamp.Body", Some("Lamp.Body"))] {
        let j = part(&s, name).joint.as_ref().unwrap();
        assert_eq!((j.parent.as_deref(), j.to.as_deref()), (Some(from), to));
    }
    // no coordinate of any end is written in the script
    assert!(!src.contains("path="));
}
