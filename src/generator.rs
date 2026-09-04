// src/generator.rs
//
// The generator pass: scatter instances of a target thing over the
// printed world's top surface, respecting `where`, `min_spacing`, and a
// seed. Output is a list of placements in WORLD voxel coordinates; the
// pipeline expands each into the target's parts under a translation, so
// scattered instances are ordinary parts of the layer — in the voxel
// output, in the scene IR, and in the viewer alike.
//
// Elevation is computed from the solved parts without allocating a grid:
// for each (x, z) column, the highest voxel-centre that any part contains.
// This is the same `contains` predicate the rasterizer uses at the same
// sample points, so the top surface here is exactly the top surface of
// the voxel output — generators no longer read a grid, but they agree
// with it to the voxel.
//
// Elevation, x and z are WORLD coordinates in voxel units.

use std::collections::HashMap;

use crate::anchors::analytic_extents;
use crate::ast::{Expr, GeneratorDecl, ShapeExpr};
use crate::frame::{Frame, Vec3};
use crate::geometry::contains;
use crate::value::{eval, Env, Value};

// ── Public types ───────────────────────────────────────────────────────────

/// One placed instance from a generator.
#[derive(Debug, Clone)]
pub struct PlacedInstance {
    pub generator_name: String,
    pub target_name:    String,
    /// Position within its generator, for stable part names
    /// (`ForestGen.3.Trunk`).
    pub index: usize,
    pub x: i32,
    pub y: i32,   // surface elevation at this column, world voxel units
    pub z: i32,
}

pub type GeneratorOutput = Vec<PlacedInstance>;

/// (x, z) → highest filled voxel y, world voxel units.
pub type ElevationMap = HashMap<(i32, i32), i32>;

// ── Elevation from solved parts ────────────────────────────────────────────

/// The top surface of a set of placed parts, column by column. Grid-free:
/// each column is probed from the top of the part's world AABB downward
/// and stops at the first voxel-centre the part contains. Padding and
/// sampling match `geometry::rasterize_entity`, so this surface is the
/// voxel output's surface.
pub fn analytic_elevation_map(
    parts: &[(String, ShapeExpr, u16, Frame)],
    vs:    f64,
) -> ElevationMap {
    let mut map: ElevationMap = HashMap::new();

    for (_, shape, _, frame) in parts {
        let e = analytic_extents(shape);
        let mut min = Vec3::new(f64::MAX, f64::MAX, f64::MAX);
        let mut max = Vec3::new(f64::MIN, f64::MIN, f64::MIN);
        for &cx in &[e.min.x, e.max.x] {
            for &cy in &[e.min.y, e.max.y] {
                for &cz in &[e.min.z, e.max.z] {
                    let p = frame.apply_point(Vec3::new(cx, cy, cz));
                    min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
                    max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
                }
            }
        }

        // Two voxels of padding, as vox_aabb does.
        let (x0, x1) = ((min.x / vs).floor() as i32 - 2, (max.x / vs).ceil() as i32 + 2);
        let (y0, y1) = ((min.y / vs).floor() as i32 - 2, (max.y / vs).ceil() as i32 + 2);
        let (z0, z1) = ((min.z / vs).floor() as i32 - 2, (max.z / vs).ceil() as i32 + 2);

        let inv = frame.inverse();
        for x in x0..=x1 {
            for z in z0..=z1 {
                for y in (y0..=y1).rev() {
                    let pw = Vec3::new(x as f64 * vs, y as f64 * vs, z as f64 * vs);
                    if contains(shape, inv.apply_point(pw), vs) {
                        let top = map.entry((x, z)).or_insert(i32::MIN);
                        if y > *top { *top = y; }
                        break;
                    }
                }
            }
        }
    }

    map
}

// ── Entry point ────────────────────────────────────────────────────────────

pub fn run_generators(elev_map: &ElevationMap, generators: &[GeneratorDecl]) -> GeneratorOutput {
    let mut all = Vec::new();
    for gen in generators {
        all.extend(run_one_generator(gen, elev_map));
    }
    all
}

// ── Per-generator execution ────────────────────────────────────────────────

fn run_one_generator(gen: &GeneratorDecl, elev_map: &ElevationMap) -> Vec<PlacedInstance> {
    let count       = prop_i64(gen, "count",       50)  as usize;
    let min_spacing = prop_f64(gen, "min_spacing",  3.0);
    let seed        = prop_i64(gen, "seed",         42)  as u64;

    let condition = gen.props.iter().find(|p| p.key == "where").map(|p| &p.value);

    // `avoid` is parsed but not yet honoured.
    let _avoid = gen.props.iter().find(|p| p.key == "avoid").map(|p| prop_str_val(&p.value));

    let mut candidates: Vec<(i32, i32, i32)> = elev_map
        .iter()
        .filter_map(|(&(x, z), &y)| {
            if let Some(cond) = condition {
                if !cell_passes(cond, &cell_env(x, y, z, elev_map)) { return None; }
            }
            Some((x, y, z))
        })
        .collect();

    // HashMap iteration order is seeded per PROCESS; sort first so the
    // shuffle permutes one canonical order and `seed` actually pins the
    // layout.
    candidates.sort_unstable();
    shuffle(&mut candidates, seed);

    let mut placed: Vec<PlacedInstance> = Vec::new();

    'outer: for (x, y, z) in candidates {
        if placed.len() >= count { break; }

        for p in &placed {
            let dx = (x - p.x) as f64;
            let dz = (z - p.z) as f64;
            if (dx * dx + dz * dz).sqrt() < min_spacing {
                continue 'outer;
            }
        }

        placed.push(PlacedInstance {
            generator_name: gen.name.name.clone(),
            target_name:    gen.scatter_target.name.clone(),
            index:          placed.len(),
            x, y, z,
        });
    }

    placed
}

// ── Condition evaluation ───────────────────────────────────────────────────
//
// Phase D: `where` goes through `value::eval` with a per-cell environment.
// The resolver checks statically that a `where` only names these
// variables, so an Err here cannot be an undefined name; either way the
// cell is excluded, never silently allowed.

/// The variables a generator `where` may name. The resolver imports this
/// for its static check.
pub const WHERE_VARS: &[&str] = &["elevation", "slope", "x", "z", "depth"];

fn cell_env(x: i32, y: i32, z: i32, elev_map: &ElevationMap) -> Env {
    let mut env = Env::new();
    env.insert("elevation".into(), Value::Num(y as f64));
    env.insert("x".into(),         Value::Num(x as f64));
    env.insert("z".into(),         Value::Num(z as f64));
    env.insert("slope".into(),     Value::Num(estimate_slope(x, z, elev_map)));
    env.insert("depth".into(),     Value::Num(-(y as f64)));
    env
}

fn cell_passes(cond: &Expr, env: &Env) -> bool {
    match eval(cond, env) {
        Ok(Value::Bool(b)) => b,
        Ok(Value::Num(n))  => n != 0.0,
        Err(_)             => false,
    }
}

/// Max elevation difference to the 4 neighbours.
fn estimate_slope(x: i32, z: i32, map: &ElevationMap) -> f64 {
    let center = *map.get(&(x, z)).unwrap_or(&0) as f64;
    [(x + 1, z), (x - 1, z), (x, z + 1), (x, z - 1)].iter()
        .map(|&(nx, nz)| (*map.get(&(nx, nz)).unwrap_or(&0) as f64 - center).abs())
        .fold(0.0_f64, f64::max)
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn prop_i64(gen: &GeneratorDecl, key: &str, default: i64) -> i64 {
    gen.props.iter().find(|p| p.key == key).map(|p| match &p.value {
        Expr::Int(n)   => *n,
        Expr::Float(f) => *f as i64,
        _              => default,
    }).unwrap_or(default)
}

fn prop_f64(gen: &GeneratorDecl, key: &str, default: f64) -> f64 {
    gen.props.iter().find(|p| p.key == key).map(|p| match &p.value {
        Expr::Float(f) => *f,
        Expr::Int(n)   => *n as f64,
        _              => default,
    }).unwrap_or(default)
}

fn prop_str_val(expr: &Expr) -> String {
    match expr {
        Expr::Ident(i) => i.name.clone(),
        Expr::Str(s)   => s.clone(),
        _              => String::new(),
    }
}

/// Fisher-Yates with a deterministic hash.
fn shuffle<T>(v: &mut [T], seed: u64) {
    let n = v.len();
    for i in (1..n).rev() {
        let j = (hash(i as u64 ^ seed) % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
}

fn hash(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^= x >> 33;
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::NamedArg;

    fn na(k: &str, v: f64) -> NamedArg { NamedArg { key: k.into(), value: Expr::Float(v) } }

    /// A unit-height cylinder standing at y=5 has its top voxel-centre at
    /// y=6 (`ky <= h_vox` is inclusive), and that is what the map reports.
    /// Cells outside its footprint are absent, not zero.
    #[test]
    fn elevation_is_the_highest_contained_voxel_in_world_units() {
        let cyl = ShapeExpr::Cylinder { args: vec![na("height", 1.0), na("radius", 2.0)] };
        let parts = vec![("P".to_string(), cyl, 1u16, Frame::from_pos(Vec3::new(0.0, 5.0, 0.0)))];
        let map = analytic_elevation_map(&parts, 1.0);
        assert_eq!(map.get(&(0, 0)), Some(&6));
        assert_eq!(map.get(&(10, 10)), None, "outside the footprint is not a candidate");
    }

    /// Two overlapping parts: the higher one wins per column.
    #[test]
    fn elevation_takes_the_max_across_parts() {
        let low  = ShapeExpr::Box_ { args: vec![na("width", 10.0), na("height", 2.0), na("depth", 10.0)] };
        let high = ShapeExpr::Box_ { args: vec![na("width", 2.0),  na("height", 8.0), na("depth", 2.0)] };
        let parts = vec![
            ("L".to_string(), low,  1u16, Frame::IDENTITY),
            ("H".to_string(), high, 1u16, Frame::IDENTITY),
        ];
        let map = analytic_elevation_map(&parts, 1.0);
        assert!(map[&(0, 0)] > map[&(4, 4)], "the tall box's column must be higher");
    }
}