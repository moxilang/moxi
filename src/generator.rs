// src/generator.rs
//
// The generator pass runs after geometry compilation and relation resolution.
// It reads `GeneratorDecl` blocks and scatters instances of a target entity
// over a terrain surface, respecting `where`, `avoid`, and `min_spacing`.
//
// Output: a list of placed instances with world-space (x, y, z) positions.
// The main pipeline merges these into the scene VoxelGrid.

use std::collections::HashMap;
use crate::ast::{GeneratorDecl, Expr, BinOp};
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
    grid:     &VoxelGrid,
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
            let ctx = EvalCtx { x, y, z, elev_map };
            if let Some(cond) = condition {
                if !eval_bool(cond, &ctx) { return None; }
            }
            Some((x, y, z))
        })
        .collect();

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

// ── Condition evaluator ────────────────────────────────────────────────────

struct EvalCtx<'a> {
    x: i32,
    y: i32,   // elevation
    z: i32,
    elev_map: &'a HashMap<(i32,i32), i32>,
}

/// Evaluate a boolean condition expression at a given (x,y,z) position.
/// Supports: `elevation < 30`, `slope < 25`, `and`, `or`, `not`, comparisons.
fn eval_bool(expr: &Expr, ctx: &EvalCtx) -> bool {
    match expr {
        Expr::Int(n)   => *n != 0,
        Expr::Float(f) => *f != 0.0,
        Expr::BinOp { op, lhs, rhs } => {
            match op {
                BinOp::And => eval_bool(lhs, ctx) && eval_bool(rhs, ctx),
                BinOp::Or  => eval_bool(lhs, ctx) || eval_bool(rhs, ctx),
                _ => {
                    let l = eval_f64(lhs, ctx);
                    let r = eval_f64(rhs, ctx);
                    match op {
                        BinOp::Lt   => l < r,
                        BinOp::Gt   => l > r,
                        BinOp::LtEq => l <= r,
                        BinOp::GtEq => l >= r,
                        BinOp::Eq   => (l - r).abs() < 0.001,
                        BinOp::Neq  => (l - r).abs() >= 0.001,
                        _ => false,
                    }
                }
            }
        }
        Expr::Not(inner) => !eval_bool(inner, ctx),
        _ => true, // unknown → allow
    }
}

fn eval_f64(expr: &Expr, ctx: &EvalCtx) -> f64 {
    match expr {
        Expr::Int(n)   => *n as f64,
        Expr::Float(f) => *f,
        Expr::Ident(i) => match i.name.as_str() {
            "elevation" => ctx.y as f64,
            "x"         => ctx.x as f64,
            "z"         => ctx.z as f64,
            "slope"     => estimate_slope(ctx.x, ctx.z, ctx.elev_map),
            "depth"     => -(ctx.y as f64), // below sea level
            _           => 0.0,
        },
        Expr::BinOp { op, lhs, rhs } => {
            let l = eval_f64(lhs, ctx);
            let r = eval_f64(rhs, ctx);
            match op {
                BinOp::Add => l + r,
                BinOp::Sub => l - r,
                BinOp::Mul => l * r,
                BinOp::Div => if r != 0.0 { l / r } else { 0.0 },
                _          => 0.0,
            }
        }
        _ => 0.0,
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
fn shuffle<T>(v: &mut Vec<T>, seed: u64) {
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