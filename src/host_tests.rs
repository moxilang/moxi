//! Hosted parts and `fuse` (Form, stage 4): the acceptance tests of the
//! issue in proposals/FORM.md.

use crate::ast::{Expr, NamedArg, ShapeExpr};
use crate::frame::{Frame, Mat3, Vec3};
use crate::geometry::distance;
use crate::pipeline::{compile_to_scene, compile_to_scene_posed};
use crate::scene::{Part, Scene};

fn fence(body: &str) -> String {
    format!("```moxi\nmaterial M {{ color = \"#888888\" }}\nmaterial N {{ color = \"#336699\" }}\n{body}\n```\n")
}
fn scene(body: &str) -> Scene {
    compile_to_scene(fence(body).as_str()).unwrap_or_else(|e| panic!("{e:?}"))
}
fn errors(body: &str) -> String {
    let es = compile_to_scene(fence(body).as_str()).expect_err("an error");
    es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n")
}
fn part<'a>(s: &'a Scene, name: &str) -> &'a Part {
    s.layers[0].parts.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no part {name}"))
}
/// A part's distance field at a WORLD point.
fn world_d(p: &Part, q: Vec3) -> f64 {
    distance(&p.shape.to_expr(), p.frame.to_frame().inverse().apply_point(q), 1.0)
}
fn lcg(seed: &mut u64) -> f64 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    (*seed >> 11) as f64 / (1u64 << 53) as f64
}

#[test]
fn placed_reproduces_any_frame_including_the_gimbal_case() {
    let shape = ShapeExpr::Box_ { args: vec![
        NamedArg { key: "width".into(), value: Expr::Float(3.0) },
        NamedArg { key: "height".into(), value: Expr::Float(1.0) },
        NamedArg { key: "depth".into(), value: Expr::Float(2.0) },
    ] };
    let mut seed = 9;
    let mut frames: Vec<Frame> = (0..200).map(|_| {
        let (a, b, c) = (lcg(&mut seed) * 6.3, lcg(&mut seed) * 3.1 - 1.55, lcg(&mut seed) * 6.3);
        Frame::new(Mat3::rot_y(a).mul(&Mat3::rot_x(b)).mul(&Mat3::rot_z(c)),
                   Vec3::new(lcg(&mut seed) * 10.0 - 5.0, lcg(&mut seed) * 10.0 - 5.0, 0.0))
    }).collect();
    // gimbal: pitch exactly ±90° about Y in the Rz·Ry·Rx order
    for s in [1.0f64, -1.0] {
        frames.push(Frame::new(Mat3::rot_z(0.7).mul(&Mat3::rot_y(s * std::f64::consts::FRAC_PI_2)).mul(&Mat3::rot_x(0.3)),
                               Vec3::new(1.0, 2.0, 3.0)));
    }
    frames.push(Frame::IDENTITY);
    for f in frames {
        let placed = crate::pipeline::placed(&shape, &f);
        for _ in 0..20 {
            let p = Vec3::new(lcg(&mut seed) * 16.0 - 8.0, lcg(&mut seed) * 16.0 - 8.0, lcg(&mut seed) * 16.0 - 8.0);
            let want = distance(&shape, f.inverse().apply_point(p), 1.0);
            assert!((distance(&placed, p, 1.0) - want).abs() < 1e-9, "{f:?} at {p:?}");
        }
    }
}

const WALL: &str = "thing Window {
    part Hole  { shape = box(width=4, height=5, depth=40), cuts = host }
    part Frame { shape = difference(box(width=5, height=6, depth=1), box(width=4, height=5, depth=2)), material = N }
    anchor seat = Hole.point(0, 0, 0, 0, 0, -1)
    relation { Frame.origin on Hole.origin }
    resolve grain = 0.5
}
thing T {
    part Wall { shape = box(width=30, height=20, depth=2), material = M }
    part Thick { shape = box(width=10, height=20, depth=9), material = M }
    part A { thing = Window }
    part B { thing = Window }
    relation {
        Thick.west on Wall.east
        A.seat on Wall.north shift=(2, -6)
        B.seat on Thick.north
    }
    resolve grain = 0.5
}
print T";

#[test]
fn a_window_cuts_its_wall_and_moves_nothing() {
    let s = scene(WALL);
    let wall = part(&s, "Wall");
    let hole = part(&s, "A.Hole");
    assert_eq!(hole.absorbed.as_ref().and_then(|a| a.into.as_deref()), Some("Wall"));
    // inside the cut: outside the assembled wall; far from it: unchanged
    let plain = ShapeExpr::Box_ { args: ["width", "height", "depth"].iter().zip([30.0, 20.0, 2.0])
        .map(|(k, v)| NamedArg { key: (*k).into(), value: Expr::Float(v) }).collect() };
    let hole_world = hole.frame.to_frame().pos;
    assert!(world_d(wall, hole_world) > 0.0, "the hole's center is no longer wall");
    let mut seed = 4;
    for _ in 0..2000 {
        let q = Vec3::new(lcg(&mut seed) * 34.0 - 17.0, lcg(&mut seed) * 24.0 - 12.0, lcg(&mut seed) * 6.0 - 3.0);
        let local = hole.frame.to_frame().inverse().apply_point(q);
        if local.x.abs() < 3.0 && local.y.abs() < 3.5 { continue; } // near the hole
        assert!((world_d(wall, q) - distance(&plain, q, 1.0)).abs() < 1e-12, "changed far from the hole at {q:?}");
    }
    // the same Window cuts a wall of another thickness, unchanged
    let (thick, bh) = (part(&s, "Thick"), part(&s, "B.Hole"));
    assert!(world_d(thick, bh.frame.to_frame().pos) > 0.0);
    let behind = bh.frame.to_frame().apply_point(Vec3::new(0.0, 0.0, 6.0)); // through the far face
    assert!(world_d(thick, behind) > 0.0, "the opening goes through the thick wall");
    // frames and anchors are those of the solve without the cut
    let uncut = scene(&WALL.replace(", cuts = host", ", material = M"));
    for p in &s.layers[0].parts {
        assert_eq!(format!("{:?}", p.frame), format!("{:?}", part(&uncut, &p.name).frame), "{} moved", p.name);
    }
    // the report lists the hole under the wall, not as a row
    let r = crate::report::from_source(fence(WALL).as_str(), None).unwrap();
    let text = r.to_text(false);
    assert!(text.contains("A.Hole (cut into Wall)") && text.contains("B.Hole (cut into Thick)"), "{text}");
    assert!(r.things[0].parts.iter().all(|p| !p.name.ends_with(".Hole")));
}

#[test]
fn a_fused_part_fillets_only_near_the_join_and_wears_the_host() {
    let src = |blend: f64| format!("thing T {{
        part Body   {{ shape = sphere(radius=5), material = M }}
        part Collar {{ shape = torus(major_radius=2, minor_radius=0.6), material = N, fuses = Body, blend = {blend} }}
        relation {{ Collar.center on Body.top }}
        resolve grain = 0.5 }}
        print T");
    let s = scene(&src(0.8));
    let body = part(&s, "Body");
    assert_eq!(part(&s, "Collar").absorbed.as_ref().and_then(|a| a.into.as_deref()), Some("Body"));
    assert_eq!(body.material.as_deref(), Some("M"), "the fused part wears the host's material");
    let collar = part(&s, "Collar");
    let sphere = ShapeExpr::Sphere { args: vec![NamedArg { key: "radius".into(), value: Expr::Float(5.0) }] };
    let mut seed = 8;
    let (mut far, mut near_changed) = (0, 0);
    for _ in 0..4000 {
        let q = Vec3::new(lcg(&mut seed) * 16.0 - 8.0, lcg(&mut seed) * 16.0 - 8.0, lcg(&mut seed) * 16.0 - 8.0);
        let (a, b) = (distance(&sphere, q, 1.0), world_d(collar, q));
        let d = world_d(body, q);
        if b > a + 2.0 * 0.8 {
            assert_eq!(d, a, "unchanged away from the join at {q:?}");
            far += 1;
        } else if (d - a.min(b)).abs() > 1e-9 { near_changed += 1; }
    }
    assert!(far > 1000 && near_changed > 0, "{far} far, {near_changed} filleted");
}

const FUSED: &str = "thing T {
    part Trunk { shape = cylinder(height=10, radius=1), material = M }
    part Bough { shape = capsule(height=6, radius=0.5), material = M }
    part Twig  { shape = capsule(height=3, radius=0.25), material = M }
    part Leaf  { shape = sphere(radius=0.8), material = N }
    relation {
        Bough.bottom on Trunk.side(t=0.6, angle=90) lean=(40, 0)
        Twig.bottom on Bough.side(t=0.7, angle=0)
        Leaf.center on Twig.top
    }
    fuse Trunk, Bough, Twig blend=0.4
    resolve grain = 0.25
}
print T";

#[test]
fn a_fuse_group_is_one_node_and_keeps_every_frame() {
    let s = scene(FUSED);
    for m in ["Bough", "Twig"] {
        assert_eq!(part(&s, m).absorbed.as_ref().and_then(|a| a.into.as_deref()), Some("Trunk"));
    }
    let unfused = scene(&FUSED.replace("    fuse Trunk, Bough, Twig blend=0.4\n", ""));
    for p in &s.layers[0].parts {
        assert_eq!(format!("{:?}", p.frame), format!("{:?}", part(&unfused, &p.name).frame), "{} moved", p.name);
    }
    let g = crate::gltf::scene_to_gltf(&s, None);
    let nodes = g.json["nodes"].as_array().unwrap();
    let names: Vec<&str> = nodes.iter().filter_map(|n| n["extras"]["moxi"]["part"].as_str()).collect();
    assert!(names.contains(&"Trunk") && !names.contains(&"Bough") && !names.contains(&"Twig"), "{names:?}");
    let leaf = nodes.iter().find(|n| n["extras"]["moxi"]["part"] == "Leaf").unwrap();
    assert_eq!(leaf["extras"]["moxi"]["parent"], "Trunk", "the leaf hangs from the group's node");
    let text = crate::report::from_source(fence(FUSED).as_str(), None).unwrap().to_text(false);
    assert!(text.contains("Bough (fused into Trunk)") && text.contains("Twig (fused into Trunk)"), "{text}");
}

#[test]
fn fuse_takes_families_and_instances_by_material() {
    let s = scene("thing Branch {
        part Wood { shape = capsule(height=4, radius=0.4), material = M }
        part Leaf { shape = sphere(radius=0.6), material = N }
        relation { Leaf.center on Wood.top }
        resolve grain = 0.25 }
    thing T {
        part Trunk { shape = cylinder(height=10, radius=1), material = M }
        for k in 0..3 {
            part Root[k] { shape = capsule(height=3, radius=0.3), material = M }
            relation { Root[k].top on Trunk.side(t=0.05, angle=k * 120) }
        }
        part Br { thing = Branch }
        relation { Br.Wood.bottom on Trunk.side(t=0.7, angle=90) }
        fuse Trunk, Root, Br blend=0.3
        resolve grain = 0.25 }
        print T");
    let into = |n: &str| part(&s, n).absorbed.as_ref().and_then(|a| a.into.clone());
    for n in ["Root[0]", "Root[1]", "Root[2]", "Br.Wood"] {
        assert_eq!(into(n).as_deref(), Some("Trunk"), "{n}");
    }
    assert_eq!(into("Br.Leaf"), None, "the leaf wears another material and stays its own part");
}

#[test]
fn a_rounded_cut_meshes_the_same_sparse_and_dense() {
    use crate::mesh::{surface_nets_with, Sampling};
    let wall = ShapeExpr::Box_ { args: ["width", "height", "depth"].iter().zip([20.0, 14.0, 3.0])
        .map(|(k, v)| NamedArg { key: (*k).into(), value: Expr::Float(v) }).collect() };
    let hole = ShapeExpr::Cylinder { args: vec![NamedArg { key: "height".into(), value: Expr::Float(10.0) },
                                                NamedArg { key: "radius".into(), value: Expr::Float(3.0) }] };
    let cut = ShapeExpr::Difference {
        base: Box::new(wall),
        cuts: vec![crate::pipeline::placed(&hole, &Frame::new(Mat3::rot_x(std::f64::consts::FRAC_PI_2), Vec3::new(2.0, 1.0, -5.0)))],
        blend: 0.8,
    };
    let (a, b) = (surface_nets_with(&cut, 0.25, 1.0, Sampling::Auto), surface_nets_with(&cut, 0.25, 1.0, Sampling::Dense));
    let bits = |m: &crate::mesh::TriMesh| m.positions.iter().flat_map(|v| v.map(f32::to_bits)).collect::<Vec<_>>();
    assert!(!a.indices.is_empty());
    assert_eq!((bits(&a), &a.indices), (bits(&b), &b.indices));
}

#[test]
fn a_pose_may_move_a_host_with_its_parts_but_not_apart() {
    let src = |pose: &str| format!("thing Plug {{
        part Bore {{ shape = cylinder(height=6, radius=0.5), cuts = host }}
        part Cap  {{ shape = sphere(radius=0.7), material = M }}
        anchor seat = Bore.bottom
        relation {{ Cap.center on Bore.bottom }}
        resolve grain = 0.25 }}
    thing T {{
        part Base {{ shape = box(width=10, height=1, depth=10), material = M }}
        part Lid  {{ shape = box(width=6, height=2, depth=6), material = M }}
        part P    {{ thing = Plug }}
        relation {{
            Lid.bottom on Base.top pitch=0
            P.seat on Lid.bottom gap=0
        }}
        {pose}
        resolve grain = 0.25 }}
        print T");
    // the lid tips with its bore: fine
    let s = compile_to_scene_posed(fence(&src("pose Tip { Lid pitch=30 }")).as_str(), Some("Tip"));
    assert!(s.is_ok(), "{:?}", s.err());
    // the bore slides off the lid: an error naming the pose
    let e = errors(&src("pose Slide { P gap=2 }"));
    assert!(e.contains("pose 'Slide' moves 'P.Bore' away from 'Lid'"), "{e}");
}

#[test]
fn hosted_and_fuse_errors_name_the_part_and_the_fix() {
    let thing = |body: &str| format!("thing T {{
        part A {{ shape = box(width=4, height=2, depth=4), material = M }}
        part B {{ shape = sphere(radius=1), material = M }}
        {body}
        resolve grain = 0.5 }}
        print T");
    let mirror = "thing W { part Hole { shape = box(width=1, height=1, depth=9), cuts = host }
                            part F { shape = sphere(radius=1), material = M }
                            relation { F.center on Hole.center }
                            resolve grain = 0.5 }
        thing T { part A { shape = box(width=20, height=2, depth=4), material = M }
                  part L { thing = W }  part R { thing = W }
                  relation { R.Hole.west on A.top
                             L symmetric_across A from=R }
                  resolve grain = 0.5 }
        print T";
    let cases: Vec<(String, &str)> = vec![
        (thing("part C { shape = sphere(radius=1), cuts = Wall }\n relation { B.center on A.top\n C.center on A.top }"),
         "part 'C': it cuts 'Wall', which is not a part of this thing"),
        (thing("part C { shape = sphere(radius=1), cuts = C }\n relation { B.center on A.top\n C.center on A.top }"),
         "part 'C': it cuts itself"),
        (thing("part C { shape = sphere(radius=1), cuts = A, fuses = A }\n relation { B.center on A.top\n C.center on A.top }"),
         "a part either cuts its target or fuses into it"),
        (thing("part C { shape = sphere(radius=1), material = M, blend = 2 }\n relation { B.center on A.top\n C.center on A.top }"),
         "which neither cuts nor fuses"),
        (thing("part C { shape = sphere(radius=1), fuses = A }\n part D { shape = sphere(radius=0.5), cuts = C }\n \
                relation { B.center on A.top\n C.center on A.top\n D.center on C.top }"),
         "part 'D': it cuts 'C', which itself cuts or fuses into another part"),
        (thing("relation { B.center on A.top }\n fuse A"), "`fuse` joins two parts or more"),
        (thing("part C { shape = sphere(radius=1), material = N }\n relation { B.center on A.top\n C.center on A.top }\n fuse A, C"),
         "`fuse` makes one surface in one material"),
        (thing("relation { B.center on A.top }\n fuse A, B, A"), "`fuse` names 'A' twice"),
        (thing("part C { shape = sphere(radius=1), material = M }\n relation { B.center on A.top\n C.center on A.top }\n fuse A, B\n fuse B, C"),
         "'B' is in two `fuse` groups"),
        (thing("part C { shape = sphere(radius=1), fuses = A }\n relation { B.center on A.top\n C.center on A.top }\n fuse B, C"),
         "'C' already cuts or fuses into a part"),
        (thing("relation { B.center on A.top }\n fuse A, Q"), "'Q' is not a part, a family or an instance"),
        (mirror.to_string(), "a mirror image does not cut or fuse into its host"),
    ];
    for (src, want) in cases {
        let e = errors(&src);
        assert!(e.contains(want), "expected {want:?} in:\n{e}");
    }
}

#[test]
fn the_facade_and_the_fused_mug_build() {
    let f = compile_to_scene(include_str!("../scripts/FACADE.md")).unwrap_or_else(|e| panic!("{e:?}"));
    let holes: Vec<&Part> = f.layers[0].parts.iter().filter(|p| p.name.ends_with(".Hole")).collect();
    assert_eq!(holes.len(), 5);
    assert!(holes.iter().all(|h| h.absorbed.as_ref().is_some_and(|a| a.cut && a.into.is_some())));
    for h in &holes {
        let into = h.absorbed.as_ref().unwrap().into.clone().unwrap();
        assert!(world_d(part(&f, &into), h.frame.to_frame().pos) > 0.0, "{} is cut out of {into}", h.name);
    }
    let m = compile_to_scene(include_str!("../scripts/MUG_FUSED.md")).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(part(&m, "Handle").absorbed.as_ref().and_then(|a| a.into.as_deref()), Some("Cup"));
}
