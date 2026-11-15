//! Marks (0.9, proposals/MARKS.md): written, placed, checked and reported
//! (step 1), and drawn on their host's surface (step 2).

use crate::frame::Vec3;
use crate::pipeline::compile_to_scene;
use crate::scene::{Scene, Shape};

fn script(parts: &str, relations: &str) -> String {
    format!("```moxi\nmaterial M {{ color = \"#888888\" }}\nmaterial Eye {{ color = \"#1b141d\" }}\n\
        thing T {{\n part Head {{ shape = sphere(radius=5), material = M }}\n\
        part Body {{ shape = capsule(height=10, radius=4), material = M }}\n{parts}\n\
        relation {{\n Head.bottom on Body.top\n{relations}\n }}\n resolve grain = 0.5 }}\nprint T\n```\n")
}

fn scene(parts: &str, relations: &str) -> Scene {
    compile_to_scene(&script(parts, relations)).unwrap_or_else(|e| panic!("{e:?}"))
}

fn errors(parts: &str, relations: &str) -> Vec<String> {
    compile_to_scene(&script(parts, relations)).expect_err("an error").into_iter().map(|e| e.message).collect()
}

fn near(a: Vec3, b: Vec3) -> bool { a.sub(b).length() < 1e-9 }

#[test]
fn a_mark_lies_on_its_anchor_facing_out() {
    let s = scene("part EyeR { mark = oval(width=2, height=3), material = Eye }",
                  "EyeR on Head.surface(yaw=30, pitch=20)");
    let parts = &s.layers[0].parts;
    let (head, eye) = (parts.iter().find(|p| p.name == "Head").unwrap(), parts.iter().find(|p| p.name == "EyeR").unwrap());
    let (hf, ef) = (head.frame.to_frame(), eye.frame.to_frame());
    // the eye's middle is on the sphere, and its front (+Z) is the sphere's
    // outward normal there
    let out = ef.pos.sub(hf.pos);
    assert!((out.length() - 5.0).abs() < 1e-9, "{}", out.length());
    assert!(near(ef.rot.col(2), out.normalize().unwrap()), "front {:?}, normal {:?}", ef.rot.col(2), out);
    // laid exactly as a flat plate's `south` face would be
    let plate = scene("part EyeR { shape = box(width=2, height=3, depth=0), material = Eye }",
                      "EyeR.south on Head.surface(yaw=30, pitch=20)");
    let pf = plate.layers[0].parts.iter().find(|p| p.name == "EyeR").unwrap().frame.to_frame();
    for k in 0..3 { assert!(near(ef.rot.col(k), pf.rot.col(k)), "column {k}"); }
    assert!(near(ef.pos, pf.pos));
}

#[test]
fn a_mark_keeps_its_outline_and_image_in_the_scene() {
    let s = in_folder(&script("part Lady { mark = rect(width=6, height=4, round=0.5), image = \"paintings/lady.jpg\" }",
                              "Lady.center on Body.side(t=0.5, angle=0) twist=10 shift=(1, 0)")).unwrap_or_else(|e| panic!("{e:?}"));
    let lady = s.layers[0].parts.iter().find(|p| p.name == "Lady").unwrap();
    match &lady.shape {
        Shape::Mark { outline, width, height, round, image, .. } => {
            assert_eq!((outline.as_str(), *width, *height, *round), ("rect", 6.0, 4.0, 0.5));
            assert_eq!(image.as_deref(), Some("paintings/lady.jpg"));
        }
        other => panic!("{other:?}"),
    }
    // and it survives the scene's JSON
    let back = Scene::from_json(&s.to_json_pretty()).unwrap();
    assert_eq!(format!("{:?}", back.layers[0].parts.iter().find(|p| p.name == "Lady").unwrap().shape),
               format!("{:?}", lady.shape));
}

#[test]
fn the_report_lists_marks_on_one_line_and_not_as_parts() {
    let src = script("part EyeR { mark = oval(width=2, height=3), material = Eye }\n\
                      part EyeL { mark = oval(width=2, height=3), material = Eye }\n\
                      part BandA { mark = band(width=1.5), material = Eye }",
                     "EyeR on Head.surface(yaw=20, pitch=10)\n EyeL symmetric_across Head from=EyeR\n\
                      BandA on Body.side(t=0.6)");
    let r = crate::report::from_source(&src, None).expect("compiles");
    let t = &r.things[0];
    assert_eq!(t.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Head", "Body"]);
    assert!(!t.touching.iter().flatten().any(|n| n.starts_with("Eye") || n == "BandA"));
    let text = r.to_text(false);
    assert!(text.contains("\nmarks: EyeR, EyeL on Head · BandA on Body"), "{text}");
}

#[test]
fn every_misplaced_mark_is_one_error_that_says_what_to_do() {
    let one = |parts: &str, rel: &str, want: &str| {
        let es = errors(parts, rel);
        assert_eq!(es.len(), 1, "{es:?}");
        assert!(es[0].contains(want), "want {want:?} in {es:?}");
    };
    let eye = "part E { mark = oval(width=2, height=3), material = Eye }";
    one(eye, "E on Head.north gap=1", "it lies on the surface, so it takes no gap");
    one(eye, "E on Head.north pitch=10 lean=(5, 0)", "takes no pitch, lean");
    one(eye, "E.top on Head.north", "no anchor 'top' — valid anchors: center");
    one(&format!("{eye}\n part H {{ shape = sphere(radius=1), material = M }}"),
        "E on Head.north\n H.bottom on E.center", "only a mark lies on a mark");
    one(&format!("{eye}\n part B {{ mark = band(width=1), material = Eye }}"),
        "E on Head.north\n B on E.center", "a band, which wraps a part, not a mark");
    one("part B { mark = band(width=2), material = Eye }", "B on Head.top", "lay it on that part's `side(t=…)`");
    one("part H { shape = sphere(radius=1), material = M }", "H on Head.top",
        "Only a mark is laid without one");
    one(eye, "", "'E' has no placement; its root is 'Body'");
    one(&format!("{eye}\n part F {{ shape = sphere(radius=1), material = M }}"),
        "E on Head.north\n F symmetric_across Head from=E", "only one of them is a mark");
}

#[test]
fn a_wrong_mark_is_one_error_and_the_rest_is_still_read() {
    let one = |parts: &str, want: &str| {
        let es = errors(parts, "E on Head.north");
        assert_eq!(es.len(), 1, "{es:?}");
        assert!(es[0].contains(want), "want {want:?} in {es:?}");
    };
    one("part E { mark = circle(radius=2), material = Eye }", "a mark's outline: oval(width=, height=)");
    one("part E { mark = oval(width=2), material = Eye }", "`oval` needs `height`");
    one("part E { mark = oval(width=2, height=-1), material = Eye }", "height must be more than 0");
    one("part E { mark = oval(width=2, height=3, depth=1), material = Eye }", "takes width, height, not `depth`");
    one("part E { mark = poly(points=[[0, 1], [1, 0]]), material = Eye }", "at least 3 points");
    one("part E { mark = rect(width=2, height=2, round=3), material = Eye }", "round is at most half its smaller side (1)");
    one("part E { mark = oval(width=2, height=3), shape = sphere(radius=1), material = Eye }",
        "has a mark and a shape");
    let es = errors("part E { mark = oval(width=2, height=3), material = Eye }\n\
                     part H { shape = sphere(radius=1), image = \"a.png\", material = M }",
                    "E on Head.north\n H.bottom on Head.top");
    assert_eq!(es.len(), 1, "{es:?}");
    assert!(es[0].contains("`image` on part 'H', which is not a mark"), "{es:?}");
}

#[test]
fn a_mark_is_never_too_thin_and_takes_any_size() {
    // flat on purpose, and drawn as its own grid whatever the grain
    assert!(compile_to_scene(&script("part E { mark = oval(width=0.01, height=0.01), material = Eye }",
                                     "E on Head.north")).is_ok());
}

// ── Step 2: drawn on the surface ────────────────────────────────────────────

/// The world points and the host's lift of mark `name` drawn in `s`.
fn drawn(s: &Scene, name: &str) -> (Vec<Vec3>, f64, usize) {
    let layer = &s.layers[0];
    let part = layer.parts.iter().find(|p| p.name == name).unwrap();
    let m = crate::mark::draw(layer, part, None).unwrap_or_else(|e| panic!("{e}"));
    let f = part.frame.to_frame();
    let used: std::collections::BTreeSet<u32> = m.indices.iter().copied().collect();
    let pts = used.iter().map(|&i| { let p = m.positions[i as usize]; f.apply_point(Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64)) }).collect();
    let host = crate::mark::host_of(layer, part).unwrap();
    let cell = crate::mesh::auto_cell(&host.shape.to_expr(), host.grain(layer.voxel_size));
    (pts, cell * 0.1, m.indices.len() / 3)
}

#[test]
fn an_oval_lies_on_the_sphere_inside_its_outline() {
    let s = scene("part EyeR { mark = oval(width=2, height=3), material = Eye }", "EyeR on Head.surface(yaw=30, pitch=20)");
    let head = s.layers[0].parts.iter().find(|p| p.name == "Head").unwrap().frame.to_frame();
    let eye = s.layers[0].parts.iter().find(|p| p.name == "EyeR").unwrap().frame.to_frame();
    let (pts, lift, tris) = drawn(&s, "EyeR");
    assert!(tris > 100, "{tris}");
    // positions are stored as f32: a few millionths
    let on = |p: &Vec3| p.sub(head.pos).normalize().unwrap().scale(5.0).add(head.pos);
    for p in &pts {
        // on the sphere, lifted by the hair along its normal
        let r = p.sub(head.pos).length();
        assert!((r - 5.0 - lift).abs() < 1e-5, "{r}");
        // inside the oval, seen along the eye's facing direction
        let q = eye.inverse().apply_point(on(p));
        assert!(q.x.powi(2) + (q.y / 1.5).powi(2) <= 1.0 + 1e-5, "{q:?}");
    }
    // and it reaches the outline: some point lies on it
    let edge = pts.iter().map(|p| { let q = eye.inverse().apply_point(on(p)); q.x.powi(2) + (q.y / 1.5).powi(2) }).fold(0.0, f64::max);
    assert!(edge > 1.0 - 1e-5, "{edge}");
}

#[test]
fn a_band_wraps_the_capsule_at_its_radius_between_its_edges() {
    let s = scene("part BandA { mark = band(width=2), material = Eye }", "BandA on Body.side(t=0.5)");
    let body = s.layers[0].parts.iter().find(|p| p.name == "Body").unwrap().frame.to_frame();
    let (pts, lift, _) = drawn(&s, "BandA");
    let mut angles: Vec<f64> = Vec::new();
    for p in &pts {
        let q = body.inverse().apply_point(*p);      // capsule axis: local +Y from 0 to 10
        let r = (q.x * q.x + q.z * q.z).sqrt();
        assert!((r - 4.0 - lift).abs() < 1e-5, "radius {r}");
        assert!((q.y - 5.0).abs() <= 1.0 + 1e-5, "height {}", q.y);
        angles.push(q.z.atan2(q.x));
    }
    angles.sort_by(f64::total_cmp);
    let widest = angles.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max)
        .max(std::f64::consts::TAU - (angles[angles.len() - 1] - angles[0]));
    assert!(widest < 0.2, "a gap of {widest} rad around the band");
}

#[test]
fn a_mark_over_a_cut_has_the_hole() {
    // a hole cut through the head's front, and an eye laid across it
    let parts = "part Hole { shape = sphere(radius=0.8), cuts = Head, material = M }\n\
                 part EyeR { mark = oval(width=3, height=3), material = Eye }";
    let rel = "Hole.center on Head.north\n EyeR on Head.north";
    let with = scene(parts, rel);
    let (pts, _, tris_with) = drawn(&with, "EyeR");
    let without = scene("part EyeR { mark = oval(width=3, height=3), material = Eye }", "EyeR on Head.north");
    let (_, _, tris_without) = drawn(&without, "EyeR");
    assert!(tris_with < tris_without, "{tris_with} vs {tris_without}");
    // no drawn point inside the hole's rim
    let head = with.layers[0].parts.iter().find(|p| p.name == "Head").unwrap().frame.to_frame();
    let north = head.apply_point(Vec3::new(0.0, 0.0, 5.0));
    assert!(pts.iter().all(|p| p.sub(north).length() > 0.75), "a point inside the hole");
}

#[test]
fn a_mark_that_cannot_be_drawn_is_refused_with_why() {
    let one = |parts: &str, rel: &str, want: &str| {
        let es = errors(parts, rel);
        assert_eq!(es.len(), 1, "{es:?}");
        assert!(es[0].contains(want), "want {want:?} in {es:?}");
    };
    // most of it past the end of the capsule's flat part and off its side
    one("part P { mark = rect(width=20, height=20), material = Eye }", "P on Body.side(t=1, angle=0)",
        "hangs off 'Body'");
    // too big for the curve: drawn on a head of radius 5, an oval 9.8 wide
    // covers a cap of area 2π·5·(5 − √(25 − 4.9²)) ≈ 125.8 against its flat
    // π·4.9² ≈ 75.4: 1.67 times (9 wide is 1.39, and is drawn)
    one("part P { mark = oval(width=9.8, height=9.8), material = Eye }", "P on Head.north",
        "too tight for it");
    scene("part P { mark = oval(width=9, height=9), material = Eye }", "P on Head.north");
    one("part P { mark = poly(points=[[2, 0], [-2, 2], [0, 0.2], [-2, -2]]), material = Eye }", "P on Head.north",
        "must go once around its middle");
}

#[test]
fn a_mark_on_a_fused_part_lies_on_the_fused_body() {
    // Head fuses into Body: the eye's host is the one fused surface
    let src = script("part EyeR { mark = oval(width=2, height=2), material = Eye }", "EyeR on Head.north")
        .replace("part Head { shape = sphere(radius=5), material = M }",
                 "part Head { shape = sphere(radius=5), fuses = Body, blend = 1, material = M }");
    let s = compile_to_scene(&src).unwrap_or_else(|e| panic!("{e:?}"));
    let layer = &s.layers[0];
    let eye = layer.parts.iter().find(|p| p.name == "EyeR").unwrap();
    assert_eq!(crate::mark::host_of(layer, eye).map(|h| h.name.as_str()), Some("Body"));
    let (pts, _, tris) = drawn(&s, "EyeR");
    assert!(tris > 50 && !pts.is_empty());
}

#[test]
fn every_exporter_draws_marks() {
    let s = scene("part EyeR { mark = oval(width=2, height=3), material = Eye }", "EyeR on Head.north");
    let g = crate::gltf::scene_to_gltf(&s, None);
    let node = g.json["nodes"].as_array().unwrap().iter().find(|n| n["name"] == "EyeR").expect("a node");
    assert!(node.get("mesh").is_some(), "the eye's node has a mesh");
    let dir = std::env::temp_dir().join(format!("moxi-mark-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("w");
    crate::mesh::export_scene_obj(&s, None, path.to_str().unwrap()).unwrap();
    let obj = std::fs::read_to_string(dir.join("w.obj")).unwrap();
    assert!(obj.contains("\no EyeR\n"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Step 3: poses reshape marks ─────────────────────────────────────────────

fn blink(extra_pose: &str) -> String {
    script("part EyeR { mark = oval(width=2, height=3), material = Eye }\n\
            part EyeL { mark = oval(width=2, height=3), material = Eye }",
           "EyeR on Head.surface(yaw=25, pitch=10)\n EyeL symmetric_across Head from=EyeR")
        .replace(" resolve grain = 0.5 }", &format!(" pose Shut {{ EyeR height=0.2 {extra_pose}}}\n \
            animate Blink {{ 0 rest  0.1 Shut  0.2 rest }}\n resolve grain = 0.5 }}"))
}

#[test]
fn a_pose_reshapes_a_mark_and_its_mirror_image_follows() {
    let s = compile_to_scene(&blink("")).unwrap_or_else(|e| panic!("{e:?}"));
    for name in ["EyeR", "EyeL"] {
        let p = s.layers[0].parts.iter().find(|p| p.name == name).unwrap();
        let shut = p.mark_poses.get("Shut").unwrap_or_else(|| panic!("{name} has no Shut"));
        let (w, h) = match shut {
            Shape::Mark { width, height, .. } => (*width, *height),
            Shape::Mirror { inner, .. } => match &**inner { Shape::Mark { width, height, .. } => (*width, *height), o => panic!("{o:?}") },
            o => panic!("{o:?}"),
        };
        assert_eq!((w, h), (2.0, 0.2), "{name}");
    }
    // posed, the mark itself has the pose's size
    let posed = crate::pipeline::compile_to_scene_posed(blink("").as_str(), Some("Shut")).unwrap();
    let eye = posed.layers[0].parts.iter().find(|p| p.name == "EyeR").unwrap();
    assert!(matches!(eye.shape, Shape::Mark { height, .. } if height == 0.2));
}

#[test]
fn a_blink_exports_as_a_morph_with_the_same_points_on_the_surface() {
    let s = compile_to_scene(&blink("")).unwrap();
    let g = crate::gltf::scene_to_gltf(&s, None);
    let j = &g.json;
    let mesh = j["meshes"].as_array().unwrap().iter().find(|m| m["name"] == "EyeR").unwrap();
    assert_eq!(mesh["extras"]["targetNames"], serde_json::json!(["Shut"]));
    assert_eq!(mesh["weights"], serde_json::json!([0.0]));
    let prim = &mesh["primitives"][0];
    let count = |a: &serde_json::Value| j["accessors"][a.as_u64().unwrap() as usize]["count"].as_u64().unwrap();
    assert_eq!(count(&prim["attributes"]["POSITION"]), count(&prim["targets"][0]["POSITION"]));

    // the weights track: off, on at Shut, off
    let anim = &j["animations"].as_array().unwrap()[0];
    let node = j["nodes"].as_array().unwrap().iter().position(|n| n["name"] == "EyeR").unwrap();
    let ch = anim["channels"].as_array().unwrap().iter()
        .find(|c| c["target"]["node"] == node && c["target"]["path"] == "weights").expect("a weights channel");
    let out = anim["samplers"][ch["sampler"].as_u64().unwrap() as usize]["output"].as_u64().unwrap() as usize;
    let read = |acc: usize| -> Vec<f32> {
        let a = &j["accessors"][acc];
        let v = &j["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        let n = a["count"].as_u64().unwrap() as usize * if a["type"] == "VEC3" { 3 } else { 1 };
        (0..n).map(|k| f32::from_le_bytes(g.bin[off + 4 * k..off + 4 * k + 4].try_into().unwrap())).collect()
    };
    assert_eq!(read(out), vec![0.0, 1.0, 0.0]);

    // shut, every drawn point is still on the head, inside the thin oval,
    // and the eye keeps its width
    let rest = read(prim["attributes"]["POSITION"].as_u64().unwrap() as usize);
    let d = read(prim["targets"][0]["POSITION"].as_u64().unwrap() as usize);
    let idx: std::collections::BTreeSet<usize> = {
        let a = &j["accessors"][prim["indices"].as_u64().unwrap() as usize];
        let v = &j["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let off = v["byteOffset"].as_u64().unwrap_or(0) as usize;
        (0..a["count"].as_u64().unwrap() as usize)
            .map(|k| u32::from_le_bytes(g.bin[off + 4 * k..off + 4 * k + 4].try_into().unwrap()) as usize).collect()
    };
    let parts = &s.layers[0].parts;
    let head = parts.iter().find(|p| p.name == "Head").unwrap().frame.to_frame();
    let eye = parts.iter().find(|p| p.name == "EyeR").unwrap();
    let joint = eye.joint.as_ref().unwrap().frame.to_frame();   // the node's frame
    let ef = eye.frame.to_frame();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &i in &idx {
        let p = Vec3::new((rest[3 * i] + d[3 * i]) as f64, (rest[3 * i + 1] + d[3 * i + 1]) as f64, (rest[3 * i + 2] + d[3 * i + 2]) as f64);
        let w = joint.apply_point(p);
        let r = w.sub(head.pos).length();
        assert!((r - 5.0).abs() < 0.05, "off the head: {r}");
        let q = ef.inverse().apply_point(w.sub(head.pos).normalize().unwrap().scale(5.0).add(head.pos));
        assert!(q.x.powi(2) + (q.y / 0.1).powi(2) <= 1.0 + 1e-3, "outside the shut oval: {q:?}");
        lo = lo.min(q.x); hi = hi.max(q.x);
    }
    assert!((hi - lo - 2.0).abs() < 1e-3, "width {}", hi - lo);
}

#[test]
fn only_a_mark_takes_a_size_in_a_pose() {
    let bad = |extra: &str, want: &str| {
        let src = blink("").replace("EyeR height=0.2 ", &format!("EyeR height=0.2 {extra} "));
        let es: Vec<String> = compile_to_scene(&src).expect_err("an error").into_iter().map(|e| e.message).collect();
        assert_eq!(es.len(), 1, "{es:?}");
        assert!(es[0].contains(want), "want {want:?} in {es:?}");
    };
    bad("Head height=2", "'Head' is not a mark");
    let neg = blink("").replace("EyeR height=0.2", "EyeR height=0");
    let es: Vec<String> = compile_to_scene(&neg).expect_err("an error").into_iter().map(|e| e.message).collect();
    assert!(es.len() == 1 && es[0].contains("height must be more than 0"), "{es:?}");
    let band = script("part B { mark = band(width=2), material = Eye }", "B on Body.side(t=0.5)")
        .replace(" resolve grain = 0.5 }", " pose P { B height=1 }\n resolve grain = 0.5 }");
    let es: Vec<String> = compile_to_scene(&band).expect_err("an error").into_iter().map(|e| e.message).collect();
    assert!(es.len() == 1 && es[0].contains("is a `band`, which has no height"), "{es:?}");
}

// ── A mark on a mark: a shine on an eye ─────────────────────────────────────

fn shiny(pose: &str) -> String {
    script("part EyeR { mark = oval(width=2, height=3), material = Eye }\n\
            part Shine { mark = oval(width=0.6, height=0.8), material = M }",
           "EyeR on Head.surface(yaw=25, pitch=10)\n Shine on EyeR.center shift=(0.4, -0.6)")
        .replace(" resolve grain = 0.5 }", &format!(" {pose}\n resolve grain = 0.5 }}"))
}

#[test]
fn a_shine_lies_on_the_eye_one_lift_higher() {
    let s = compile_to_scene(&shiny("")).unwrap_or_else(|e| panic!("{e:?}"));
    let layer = &s.layers[0];
    let shine = layer.parts.iter().find(|p| p.name == "Shine").unwrap();
    assert_eq!(crate::mark::host_of(layer, shine).map(|h| h.name.as_str()), Some("Head"));
    let head = layer.parts.iter().find(|p| p.name == "Head").unwrap().frame.to_frame();
    let (pts, lift, _) = drawn(&s, "Shine");
    for p in &pts {
        // on the head, two lifts up: above the eye's one
        let r = p.sub(head.pos).length();
        assert!((r - 5.0 - 2.0 * lift).abs() < 1e-5, "{r}");
    }
    let r = crate::report::from_source(&shiny(""), None).unwrap();
    assert!(r.to_text(false).contains("marks: EyeR on Head · Shine on EyeR"), "{}", r.to_text(false));
}

#[test]
fn a_shine_squashes_with_its_eye() {
    let s = compile_to_scene(&shiny("pose Shut { EyeR height=0.3 }")).unwrap_or_else(|e| panic!("{e:?}"));
    let layer = &s.layers[0];
    let shine = layer.parts.iter().find(|p| p.name == "Shine").unwrap();
    assert!(shine.mark_poses.contains_key("Shut"), "{:?}", shine.mark_poses.keys().collect::<Vec<_>>());
    // drawn as in the pose: inside the shut eye, squashed by 0.3 / 3 about its middle
    let mut posed = layer.clone();
    for q in posed.parts.iter_mut() { if let Some(sh) = q.mark_poses.get("Shut") { q.shape = sh.clone(); } }
    let p = posed.parts.iter().find(|p| p.name == "Shine").unwrap();
    let m = crate::mark::draw(&posed, p, None).unwrap();
    let f = p.frame.to_frame();
    let eye = posed.parts.iter().find(|p| p.name == "EyeR").unwrap().frame.to_frame();
    let head = posed.parts.iter().find(|p| p.name == "Head").unwrap().frame.to_frame();
    let used: std::collections::BTreeSet<u32> = m.indices.iter().copied().collect();
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &i in &used {
        let q = m.positions[i as usize];
        let w = f.apply_point(Vec3::new(q[0] as f64, q[1] as f64, q[2] as f64));
        let e = eye.inverse().apply_point(w.sub(head.pos).normalize().unwrap().scale(5.0).add(head.pos));
        assert!(e.x.powi(2) + (e.y / 0.15).powi(2) <= 1.0 + 1e-3, "outside the shut eye: {e:?}");
        lo = lo.min(e.y); hi = hi.max(e.y);
    }
    // its 0.8 height squashed by a tenth
    assert!((hi - lo - 0.08).abs() < 2e-3, "height {}", hi - lo);
}

// ---- step 4: pictures ------------------------------------------------

const QUAD: &[u8] = include_bytes!("fixtures/marks/quad.png");   // 16 × 8: red, green / blue, clear
const LADY: &[u8] = include_bytes!("fixtures/marks/lady.jpg");   // 30 × 20

/// The script compiled from a folder of its own, beside `quad.png` and
/// `paintings/lady.jpg`.
fn in_folder(src: &str) -> Result<Scene, Vec<crate::pipeline::CompileError>> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!("moxi-marks-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    std::fs::create_dir_all(dir.join("paintings")).unwrap();
    std::fs::write(dir.join("quad.png"), QUAD).unwrap();
    std::fs::write(dir.join("paintings/lady.jpg"), LADY).unwrap();
    std::fs::write(dir.join("main.md"), src).unwrap();
    let fs = crate::imports::FsSources { root: dir };
    compile_to_scene(crate::imports::Program::Files { main: "main.md", sources: &fs })
}

#[test]
fn a_picture_is_read_for_its_size_and_see_through() {
    let q = crate::mark::image_info(QUAD).unwrap();
    assert_eq!((q.width, q.height, q.mime, q.alpha), (16, 8, "image/png", true));
    let l = crate::mark::image_info(LADY).unwrap();
    assert_eq!((l.width, l.height, l.mime, l.alpha), (30, 20, "image/jpeg", false));
    assert!(crate::mark::image_info(b"hello").is_err());
}

#[test]
fn a_picture_of_the_wrong_shape_says_the_height_that_fits() {
    let msg = |parts: &str| -> String {
        in_folder(&script(parts, "Q on Body.side(t=0.5, angle=0)")).expect_err("an error")
            .into_iter().map(|e| e.message).collect::<Vec<_>>().join("\n")
    };
    let m = msg("part Q { mark = rect(width=4, height=3), image = \"quad.png\" }");
    assert!(m.contains("16 × 8") && m.contains("`height = 2` fits `width = 4`"), "{m}");
    let m = msg("part Q { mark = rect(width=4, height=2), image = \"nope.png\" }");
    assert!(m.contains("cannot read its image 'nope.png'") && m.contains("no such file"), "{m}");
    // within 1% is the picture's shape
    assert!(in_folder(&script("part Q { mark = rect(width=4, height=2.01), image = \"quad.png\" }",
                              "Q on Body.side(t=0.5, angle=0)")).is_ok());
}

#[test]
fn a_picture_travels_in_the_gltf_with_its_corners_on_the_marks() {
    let s = in_folder(&script("part Q { mark = rect(width=4, height=2), image = \"quad.png\", material = Eye }",
                              "Q on Body.side(t=0.5, angle=0)")).unwrap_or_else(|e| panic!("{e:?}"));
    let g = crate::gltf::scene_to_gltf(&s, None);
    let j = &g.json;
    assert_eq!(j["images"].as_array().unwrap().len(), 1);
    assert_eq!(j["images"][0]["mimeType"], "image/png");
    // the picture's bytes, whole, in the binary
    let v = &j["bufferViews"][j["images"][0]["bufferView"].as_u64().unwrap() as usize];
    let (o, n) = (v["byteOffset"].as_u64().unwrap() as usize, v["byteLength"].as_u64().unwrap() as usize);
    assert_eq!(&g.bin[o..o + n], QUAD);
    let node = j["nodes"].as_array().unwrap().iter().find(|n| n["name"] == "Q").unwrap();
    let prim = &j["meshes"][node["mesh"].as_u64().unwrap() as usize]["primitives"][0];
    let mat = &j["materials"][prim["material"].as_u64().unwrap() as usize];
    assert_eq!(mat["pbrMetallicRoughness"]["baseColorTexture"]["index"], 0);
    assert_eq!(mat["pbrMetallicRoughness"]["baseColorFactor"], serde_json::json!([1.0, 1.0, 1.0, 1.0]));
    assert_eq!(mat["alphaMode"], "BLEND");
    // the body keeps its own, untextured material
    let body = j["nodes"].as_array().unwrap().iter().find(|n| n["name"] == "Body").unwrap();
    let bm = j["meshes"][body["mesh"].as_u64().unwrap() as usize]["primitives"][0]["material"].as_u64().unwrap() as usize;
    assert!(j["materials"][bm]["pbrMetallicRoughness"].get("baseColorTexture").is_none());
    // texture coordinates: facing the mark, the picture's top left on the
    // mark's top left and its bottom right opposite (a mark faces its own
    // +Z, so +X is the viewer's right and +Y up)
    let read = |a: u64| -> Vec<f32> {
        let acc = &j["accessors"][a as usize];
        let v = &j["bufferViews"][acc["bufferView"].as_u64().unwrap() as usize];
        let o = v["byteOffset"].as_u64().unwrap() as usize;
        let n = acc["count"].as_u64().unwrap() as usize * if acc["type"] == "VEC2" { 2 } else { 3 };
        (0..n).map(|i| f32::from_le_bytes(g.bin[o + 4 * i..o + 4 * i + 4].try_into().unwrap())).collect()
    };
    let uv = read(prim["attributes"]["TEXCOORD_0"].as_u64().unwrap());
    let pos = read(prim["attributes"]["POSITION"].as_u64().unwrap());
    assert_eq!(uv.len() / 2, pos.len() / 3);
    assert!(uv.iter().all(|t| (-1e-6..=1.0 + 1e-6).contains(t)));
    // in the mark's own plane: x across, y up
    let q = s.layers[0].parts.iter().find(|p| p.name == "Q").unwrap();
    let qf = q.frame.to_frame();
    let nf = q.joint.as_ref().map(|j| j.frame.to_frame()).unwrap_or(qf);
    let to_plane = qf.inverse().compose(&nf);
    for i in 0..uv.len() / 2 {
        let p = to_plane.apply_point(Vec3::new(pos[3 * i] as f64, pos[3 * i + 1] as f64, pos[3 * i + 2] as f64));
        let (u, v) = (uv[2 * i] as f64, uv[2 * i + 1] as f64);
        assert!((u - (p.x + 2.0) / 4.0).abs() < 0.02 && (v - (1.0 - (p.y + 1.0) / 2.0)).abs() < 0.02,
                "point {i}: ({:.3}, {:.3}) has uv ({u:.3}, {v:.3})", p.x, p.y);
    }
}

#[test]
fn a_picture_travels_with_the_obj_beside_it() {
    let s = in_folder(&script("part Q { mark = rect(width=3, height=2), image = \"paintings/lady.jpg\", material = Eye }",
                              "Q on Body.side(t=0.5, angle=0)")).unwrap_or_else(|e| panic!("{e:?}"));
    let dir = std::env::temp_dir().join(format!("moxi-marks-obj-{}", std::process::id()));
    let path = dir.join("world");
    crate::mesh::export_scene_obj(&s, None, path.to_str().unwrap()).unwrap();
    assert_eq!(std::fs::read(dir.join("lady.jpg")).unwrap(), LADY);
    let mtl = std::fs::read_to_string(dir.join("world.mtl")).unwrap();
    assert!(mtl.contains("map_Kd lady.jpg") && !mtl.contains("map_d"), "{mtl}");
    let obj = std::fs::read_to_string(dir.join("world.obj")).unwrap();
    let q = obj.split("o Q\n").nth(1).unwrap();
    let vt = q.lines().filter(|l| l.starts_with("vt ")).count();
    let v = q.lines().filter(|l| l.starts_with("v ")).count();
    assert_eq!(vt, v);
    assert!(q.lines().filter(|l| l.starts_with("f ")).all(|l| l.split(' ').skip(1).all(|c| c.split('/').nth(1).is_some_and(|t| !t.is_empty()))));
}
