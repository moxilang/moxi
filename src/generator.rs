// src/generator.rs
//
// The generator pass runs after geometry compilation and relation resolution.
// It reads `GeneratorDecl` blocks and scatters instances of a target entity
// over a terrain surface, respecting `where`, `avoid`, and `min_spacing`.
//
// Output: a list of placed instances with world-space (x, y, z) positions.
// The main pipeline merges these into the scene VoxelGrid.

use std::collections::HashMap;
use crate::ast::{GeneratorDecl, Expr};
use crate::value::{eval, Env, Value};
use crate::voxel::VoxelGrid;

// ── Public types ───────────────────────────────────────────────────────────

/// One placed instance from a generator.
#[derive(Debug, Clone)]
pub struct PlacedInstance {
    pub generator_name: String,
    pub target_name:    String,
    pub x: i32,
    pub y: i32,   // surface elevation at this column
    pub z: i32,
}

/// The full output of running all generators over a terrain grid.
pub type GeneratorOutput = Vec<PlacedInstance>;

// ── Entry point ────────────────────────────────────────────────────────────

/// Run all generators against the compiled terrain grid.
///
/// `terrain_grid` — the voxel grid of the terrain (from heightfield stamp)
/// `generators`   — parsed generator declarations
///
/// Returns all placed instances across all generators.
pub fn run_generators(
    terrain_grid: &VoxelGrid,
    generators:   &[GeneratorDecl],
) -> GeneratorOutput {
    let mut all = Vec::new();

    // Build elevation map: (x, z) → highest filled y
    let elev_map = build_elevation_map(terrain_grid);

    for gen in generators {
        let instances = run_one_generator(gen, &elev_map, terrain_grid);
        all.extend(instances);
    }

    all
}

// ── Per-generator execution ────────────────────────────────────────────────

fn run_one_generator(
    gen:      &GeneratorDecl,
    elev_map: &HashMap<(i32,i32), i32>,
    _grid:     &VoxelGrid,
) -> Vec<PlacedInstance> {
    // Extract generator properties
    let count       = prop_i64(gen, "count",       50)  as usize;
    let min_spacing = prop_f64(gen, "min_spacing",  3.0);
    let seed        = prop_i64(gen, "seed",         42)  as u64;

    // `where` condition AST node (if any)
    let condition = gen.props.iter().find(|p| p.key == "where").map(|p| &p.value);

    // `avoid` name (if any) — we skip cells where avoid-named atom is present
    let _avoid = gen.props.iter().find(|p| p.key == "avoid").map(|p| prop_str_val(&p.value));

    // Candidate cells: all (x,z) positions in the elevation map
    let mut candidates: Vec<(i32, i32, i32)> = elev_map
        .iter()
        .filter_map(|(&(x, z), &y)| {
            if let Some(cond) = condition {
                if !cell_passes(cond, &cell_env(x, y, z, elev_map)) { return None; }
            }
            Some((x, y, z))
        })
        .collect();

    // HashMap iteration order is seeded per PROCESS, so without this sort the
    // shuffle permutes a differently-ordered list on every run and `seed` does
    // not actually pin the layout. Sort first: one canonical order in, one
    // deterministic permutation out.
    candidates.sort_unstable();

    // Shuffle candidates deterministically using our hash
    shuffle(&mut candidates, seed);

    // Place up to `count` instances with minimum spacing enforced
    let mut placed: Vec<PlacedInstance> = Vec::new();

    'outer: for (x, y, z) in candidates {
        if placed.len() >= count { break; }

        // Check min_spacing against all already-placed instances
        for p in &placed {
            let dx = (x - p.x) as f64;
            let dz = (z - p.z) as f64;
            if (dx*dx + dz*dz).sqrt() < min_spacing {
                continue 'outer;
            }
        }

        placed.push(PlacedInstance {
            generator_name: gen.name.name.clone(),
            target_name:    gen.scatter_target.name.clone(),
            x, y, z,
        });
    }

    placed
}

// ── Elevation map ──────────────────────────────────────────────────────────

/// For each (x,z) column, find the highest filled voxel y.
fn build_elevation_map(grid: &VoxelGrid) -> HashMap<(i32,i32), i32> {
    let mut map: HashMap<(i32,i32), i32> = HashMap::new();
    for (x, y, z, _) in grid.iter_filled() {
        let (x, y, z) = (x as i32, y as i32, z as i32);
        let entry = map.entry((x, z)).or_insert(i32::MIN);
        if y > *entry { *entry = y; }
    }
    map
}

// ── Condition evaluation ───────────────────────────────────────────────────
//
// Phase D: the generator no longer has its own interpreter. `where` goes
// through `value::eval` with a per-cell environment. The resolver checks
// statically that a `where` only names these variables, so an Err here
// cannot be an undefined name — it would be a type error, which is also
// reported statically. Either way the cell is excluded, never silently
// allowed as the old evaluator did.

/// The variables a generator `where` may name. Kept here, next to the
/// code that binds them; the resolver imports it for its static check.
pub const WHERE_VARS: &[&str] = &["elevation", "slope", "x", "z", "depth"];

fn cell_env(x: i32, y: i32, z: i32, elev_map: &HashMap<(i32,i32), i32>) -> Env {
    let mut env = Env::new();
    env.insert("elevation".into(), Value::Num(y as f64));
    env.insert("x".into(),         Value::Num(x as f64));
    env.insert("z".into(),         Value::Num(z as f64));
    env.insert("slope".into(),     Value::Num(estimate_slope(x, z, elev_map)));
    env.insert("depth".into(),     Value::Num(-(y as f64))); // below sea level
    env
}

fn cell_passes(cond: &Expr, env: &Env) -> bool {
    match eval(cond, env) {
        Ok(Value::Bool(b)) => b,
        Ok(Value::Num(n))  => n != 0.0,
        Err(_)             => false,
    }
}

/// Estimate slope at (x,z) as the max elevation difference to 4 neighbors.
fn estimate_slope(x: i32, z: i32, map: &HashMap<(i32,i32), i32>) -> f64 {
    let center = *map.get(&(x, z)).unwrap_or(&0) as f64;
    let neighbors = [(x+1,z),(x-1,z),(x,z+1),(x,z-1)];
    neighbors.iter()
        .map(|&(nx,nz)| (*map.get(&(nx,nz)).unwrap_or(&0) as f64 - center).abs())
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

/// Fisher-Yates shuffle using deterministic hash.
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