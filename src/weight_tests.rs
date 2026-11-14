//! Weight is visible before the build is done (Tighten, T6): the export
//! names its heaviest part with the cell it was meshed at, and any part
//! over the line with the grain that lightens it.

use crate::ast::{Expr, NamedArg, ShapeExpr};
use crate::mesh::{auto_cell, surface_nets, Weight, HEAVY_TRIANGLES};

fn sphere(r: f64) -> ShapeExpr {
    ShapeExpr::Sphere { args: vec![NamedArg { key: "radius".into(), value: Expr::Float(r) }] }
}
fn triangles(shape: &ShapeExpr, grain: f64) -> (usize, f64) {
    let c = auto_cell(shape, grain);
    (surface_nets(shape, c, grain).indices.len() / 3, c)
}

#[test]
fn the_heaviest_part_is_named_with_its_cell_and_why() {
    let small = sphere(1.0);
    let mut w = Weight::default();
    w.add("A", &small, 1200, 0.25, 0.5);
    w.add("B", &small, 9000, 0.25, 0.5);
    w.add("C", &small, 9000, 0.5, 0.5);
    let lines = w.lines();
    // B and C tie: the first exported wins; a light build has no notes
    assert_eq!(lines, ["  heaviest: 'B' — 9,000 triangles at cell 0.25 (half the grain 0.5)"]);
}

#[test]
fn a_heavy_part_gets_the_smallest_grain_that_lightens_it() {
    let s = sphere(12.0);
    let (tris, cell) = triangles(&s, 0.1);
    assert!(tris > HEAVY_TRIANGLES, "{tris}");
    let mut w = Weight::default();
    w.add("Ball", &s, tris, cell, 0.1);
    let lines = w.lines();
    assert_eq!(lines.len(), 2, "{lines:?}");
    let note = &lines[1];
    assert!(note.starts_with("  note: 'Ball' is 542,628 triangles; a grain of 0.11 or more (now 0.1) brings it under 500,000"), "{note}");
    // and it does, where the grain one step finer still does not
    assert!(triangles(&s, 0.11).0 < HEAVY_TRIANGLES);
    assert!(triangles(&s, 0.10).0 > HEAVY_TRIANGLES);
}

#[test]
fn many_heavy_parts_are_five_notes_and_a_count() {
    let small = sphere(1.0);
    let mut w = Weight::default();
    for k in 0..8 { w.add(&format!("Slab[{k}]"), &small, 600_000 + k, 3.0, 6.0); }
    let lines = w.lines();
    assert_eq!(lines.len(), 1 + 5 + 1, "{lines:?}");
    assert!(lines[0].contains("'Slab[7]'"), "heaviest first: {lines:?}");
    assert_eq!(lines[6], "  note: and 3 more parts over 500,000 triangles");
}
