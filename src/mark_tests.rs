//! Marks, step 1 (0.9, proposals/MARKS.md): written, placed, checked and
//! reported. Drawing them is step 2.

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
    let s = scene("part Lady { mark = rect(width=6, height=4, round=0.5), image = \"paintings/lady.jpg\" }",
                  "Lady.center on Body.side(t=0.5, angle=0) twist=10 shift=(1, 0)");
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
        "E on Head.north\n H.bottom on E.center", "nothing is placed on a mark");
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
fn marks_are_left_out_of_every_drawing_until_they_are_drawn() {
    let s = scene("part E { mark = poly(points=[[0, 2], [2, -1], [-2, -1]]), material = Eye }", "E on Head.north");
    assert_eq!(s.undrawn_marks().as_deref(), Some("E"));
    assert!(!s.at_rest().layers[0].parts.iter().any(|p| p.name == "E"));
    assert!(!s.without_guides().layers[0].parts.iter().any(|p| p.name == "E"));
    // and a mark is never "too thin to be drawn": it is flat on purpose
    assert!(compile_to_scene(&script("part E { mark = oval(width=0.01, height=0.01), material = Eye }",
                                     "E on Head.north")).is_ok());
}
