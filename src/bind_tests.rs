//! Arguments are computed once (Tighten, T1): the acceptance tests of the
//! issue in proposals/TIGHTEN.md.

use std::time::Instant;

use crate::pipeline::compile_to_scene;
use crate::scene::{Scene, Shape};

fn fence(body: &str) -> String {
    format!("```moxi\nmaterial M {{ color = \"#888888\" }}\n{body}\n```\n")
}
fn scene(body: &str) -> Scene {
    compile_to_scene(fence(body).as_str()).unwrap_or_else(|e| panic!("{e:?}"))
}
fn errors(body: &str) -> String {
    let es = compile_to_scene(fence(body).as_str()).expect_err("an error");
    es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n")
}
fn radius(s: &Scene, part: &str) -> f64 {
    let p = s.layers[0].parts.iter().find(|p| p.name == part).expect("part");
    match p.shape { Shape::Sphere { radius } => radius, ref other => panic!("{other:?}") }
}
fn one(shape: &str) -> String {
    format!("thing T {{ part A {{ shape = {shape}, material = M }}\n resolve grain = 0.25 }}\nprint T")
}

#[test]
fn an_argument_used_twice_at_every_level_is_computed_once() {
    // pasted, `d` 40 deep is 2^40 copies of its argument; bound, it is 40
    let calls = (0..40).fold("1.5".to_string(), |inner, _| format!("d({inner})"));
    let t = Instant::now();
    let s = scene(&format!("fn d(x) = x + x\nfn shrink(x) = x / 1099511627776\n{}",
        one(&format!("sphere(radius=shrink({calls}))"))));
    assert!((radius(&s, "A") - 1.5).abs() < 1e-12);
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
}

#[test]
fn a_shape_argument_is_computed_once_and_means_the_same() {
    // a point built by functions, used nine times by the shape: the bound
    // and the pasted forms build the same thing
    let lib = "fn dir(y) = [cos(y), 0, sin(y)]\n\
               fn add(p, d, s) = [p[0] + s * d[0], p[1] + s * d[1], p[2] + s * d[2]]\n\
               shape Pair(p) = union(at(sphere(radius=1), x=p[0], y=p[1], z=p[2]), \
                                     at(sphere(radius=1), x=add(p, dir(90), 3)[0], y=p[1], z=add(p, dir(90), 3)[2]))\n";
    let direct = scene(&format!("{lib}thing T {{ part A {{ shape = Pair(add(add([1, 2, 3], dir(30), 4), dir(60), 2)), material = M }}\n resolve grain = 0.25 }}\nprint T"));
    let named = scene(&format!("{lib}thing T {{ let q = add(add([1, 2, 3], dir(30), 4), dir(60), 2)\n part A {{ shape = Pair(q), material = M }}\n resolve grain = 0.25 }}\nprint T"));
    assert_eq!(serde_json::to_string(&direct).unwrap(), serde_json::to_string(&named).unwrap());
}

#[test]
fn an_untaken_branch_never_computes_its_argument() {
    let lib = "fn pick(c, a, b) = if c { a } else { b + b }\n";
    let s = scene(&format!("{lib}{}", one("sphere(radius=pick(2 > 1, 2, sqrt(-1)))")));
    assert_eq!(radius(&s, "A"), 2.0);
    // taken, it fails as it did when the argument was pasted in (0.8.0
    // reports this case as the `if` condition: a known, older message)
    let e = errors(&format!("{lib}{}", one("sphere(radius=pick(1 > 2, 2, sqrt(-1)))")));
    assert!(e.contains("`if` condition"), "{e}");
    assert!(!e.contains('%'), "the compiler's own names never show: {e}");
}

#[test]
fn a_failed_argument_used_deep_inside_says_why() {
    // the argument fails in the innermost call; the error is still its own
    let calls = (0..30).fold("sqrt(-1)".to_string(), |inner, _| format!("d({inner})"));
    let t = Instant::now();
    let e = errors(&format!("fn d(x) = x + x\n{}", one(&format!("sphere(radius={calls})"))));
    assert!(e.contains("sqrt") && !e.contains('%'), "{e}");
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
}

#[test]
fn an_undefined_name_lists_only_names_the_script_wrote() {
    let e = errors(&format!("fn f(a) = a * a + nope\n{}", one("sphere(radius=f(1 + 1))")));
    assert!(e.contains("'nope' is not defined") && !e.contains('%'), "{e}");
}

#[test]
fn shape_loop_bounds_may_still_use_a_computed_argument() {
    let s = scene("shape Row(n) = union(for k in 0..n { at(sphere(radius=0.5), x=2 * k) })\n\
                   thing T { part A { shape = Row(1 + 2), material = M }\n resolve grain = 0.25 }\nprint T");
    match &s.layers[0].parts[0].shape { Shape::Union { shapes, .. } => assert_eq!(shapes.len(), 3), o => panic!("{o:?}") }
}
