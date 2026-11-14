//! One mistake, one error (Tighten, T2): the acceptance tests of the issue
//! in proposals/TIGHTEN.md.

use crate::pipeline::compile_to_scene;

fn errors(src: &str) -> Vec<(String, Option<usize>)> {
    compile_to_scene(src).expect_err("an error").into_iter().map(|e| (e.message, e.line)).collect()
}

#[test]
fn a_loop_inside_relation_is_one_error_that_shows_the_fix() {
    // the grove's first draft: 31 errors on 0.8.0, 30 of them the parser
    // losing its place and the resolver reading what it lost
    let es = errors(include_str!("fixtures/GROVE_DRAFT.md"));
    assert_eq!(es.len(), 1, "{es:?}");
    assert!(es[0].0.contains("a loop cannot sit inside relation { }: write `for k in 0..n { relation { … } }`"), "{es:?}");
    assert_eq!(es[0].1, Some(81), "at the loop's line");
}

#[test]
fn two_mistakes_in_two_blocks_are_two_errors() {
    let src = "```moxi\nmaterial M { color = \"#888888\" }\n\
        thing T {\n part A { shape = box(width=2, height=2, depth=2), material = M }\n\
        part B { shape = sphere(radius=), material = M }\n\
        relation { B.bottom A.top }\n resolve grain = 0.5 }\nprint T\n```\n";
    let es = errors(src);
    assert_eq!(es.len(), 2, "{es:?}");
    assert_eq!(es[0].1, Some(5), "the missing radius in B: {es:?}");
    assert_eq!(es[1].1, Some(6), "the missing `on`: {es:?}");
}

#[test]
fn a_mistake_in_relation_does_not_lose_the_rest_of_the_thing() {
    // after the bad line the block is skipped to its brace, so the thing's
    // own closing brace and `resolve` are still read as the thing's
    let src = "```moxi\nmaterial M { color = \"#888888\" }\n\
        thing T {\n part A { shape = box(width=2, height=2, depth=2), material = M }\n\
        part B { shape = sphere(radius=1), material = M }\n\
        relation {\n B.bottom A.top\n B.top on A.bottom\n }\n resolve grain = 0.5 }\nprint T\n```\n";
    let es = errors(src);
    assert_eq!(es.len(), 1, "{es:?}");
}
