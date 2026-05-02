use crate::ast::{ShapeExpr, NamedArg, Expr};
use crate::resolver::{ResolvedScene, ResolvedEntity};
use crate::voxel::VoxelGrid;

// ── Public types ───────────────────────────────────────────────────────────

/// A single compiled part — its own grid stamped at origin.
/// The relation resolver will compute offsets between these.
#[derive(Debug)]
pub struct CompiledPart {
    pub name:       String,
    pub grid:       VoxelGrid,
    pub atom_id:    u16,
    pub voxel_size: f64,
}

/// A fully compiled entity: individual part grids + the merged final grid.
#[derive(Debug)]
pub struct CompiledEntity {
    pub name:       String,
    pub parts:      Vec<CompiledPart>,  // individual grids at origin
    pub grid:       VoxelGrid,          // merged grid (with offsets applied)
    pub voxel_size: f64,
}

// ── Public entry point ─────────────────────────────────────────────────────

/// Compile a resolved scene into voxel grids.
/// Each entity gets a CompiledEntity with per-part grids ready for
/// the relation resolver to offset.
pub fn compile(scene: &ResolvedScene, voxel_size: f64) -> Vec<CompiledEntity> {
    scene.entities.iter().map(|ent| {
        let vs = ent.resolve.as_ref().map(|r| r.voxel_size).unwrap_or(voxel_size);
        compile_entity(ent, scene, vs)
    }).collect()
}

// ── Entity compilation ─────────────────────────────────────────────────────

fn compile_entity(ent: &ResolvedEntity, scene: &ResolvedScene, voxel_size: f64) -> CompiledEntity {
    // Step 1: compile each part into its own grid at origin
    let mut compiled_parts: Vec<CompiledPart> = Vec::new();

    for part in &ent.parts {
        let atom_id = part.material_index
            .and_then(|mi| scene.materials.get(mi))
            .map(|mat| (mat.atom_index as u16) + 1)
            .unwrap_or(1);

        if let Some(shape) = &part.shape {
            let radius = bounding_radius(shape);
            let half   = (radius / voxel_size).ceil() as i32 + 4;
            let size   = (half * 2 + 1) as u32;
            let mut grid = VoxelGrid::new(size, size, size);
            stamp(shape, half, half, half, atom_id, &mut grid, voxel_size);
            compiled_parts.push(CompiledPart {
                name: part.name.clone(),
                grid,
                atom_id,
                voxel_size,
            });
        }
    }

    // Step 2: merge all parts into a combined grid at origin
    // (offsets are (0,0,0) until the relation resolver runs)
    let merged = merge_parts(&compiled_parts, &[]);

    CompiledEntity {
        name: ent.name.clone(),
        parts: compiled_parts,
        grid: merged,
        voxel_size,
    }
}

/// Merge compiled parts into a single VoxelGrid, applying (dx,dy,dz) offsets.
/// `offsets` maps part name → (dx, dy, dz).  Missing entries default to (0,0,0).
pub fn merge_parts(
    parts:   &[CompiledPart],
    offsets: &[(String, (i32, i32, i32))],
) -> VoxelGrid {
    use std::collections::HashMap;
    let offset_map: HashMap<&str, (i32,i32,i32)> = offsets
        .iter()
        .map(|(name, off)| (name.as_str(), *off))
        .collect();

    // Find total bounding box
    let mut world_min_x = i32::MAX; let mut world_max_x = i32::MIN;
    let mut world_min_y = i32::MAX; let mut world_max_y = i32::MIN;
    let mut world_min_z = i32::MAX; let mut world_max_z = i32::MIN;

    for part in parts {
        let (dx, dy, dz) = offset_map.get(part.name.as_str()).copied().unwrap_or((0,0,0));
        for (x, y, z, _) in part.grid.iter_filled() {
            let wx = x as i32 + dx;
            let wy = y as i32 + dy;
            let wz = z as i32 + dz;
            if wx < world_min_x { world_min_x = wx; }
            if wx > world_max_x { world_max_x = wx; }
            if wy < world_min_y { world_min_y = wy; }
            if wy > world_max_y { world_max_y = wy; }
            if wz < world_min_z { world_min_z = wz; }
            if wz > world_max_z { world_max_z = wz; }
        }
    }

    if world_min_x == i32::MAX {
        return VoxelGrid::new(1, 1, 1);
    }

    let w = (world_max_x - world_min_x + 1) as u32;
    let h = (world_max_y - world_min_y + 1) as u32;
    let d = (world_max_z - world_min_z + 1) as u32;
    let mut merged = VoxelGrid::new(w, h, d);

    for part in parts {
        let (dx, dy, dz) = offset_map.get(part.name.as_str()).copied().unwrap_or((0,0,0));
        for (x, y, z, atom_id) in part.grid.iter_filled() {
            let wx = x as i32 + dx - world_min_x;
            let wy = y as i32 + dy - world_min_y;
            let wz = z as i32 + dz - world_min_z;
            merged.set(wx, wy, wz, atom_id);
        }
    }

    merged
}

// ── Shape dispatcher ───────────────────────────────────────────────────────

fn stamp(
    shape:      &ShapeExpr,
    cx: i32, cy: i32, cz: i32,
    atom_id:    u16,
    grid:       &mut VoxelGrid,
    voxel_size: f64,
) {
    match shape {
        ShapeExpr::Sphere    { args } => stamp_sphere   (args, cx, cy, cz, atom_id, grid, voxel_size),
        ShapeExpr::Cylinder  { args } => stamp_cylinder (args, cx, cy, cz, atom_id, grid, voxel_size),
        ShapeExpr::Box_      { args } => stamp_box      (args, cx, cy, cz, atom_id, grid, voxel_size),
        ShapeExpr::Ellipsoid { args } => stamp_ellipsoid(args, cx, cy, cz, atom_id, grid, voxel_size),
        ShapeExpr::Blob      { args } => stamp_blob     (args, cx, cy, cz, atom_id, grid, voxel_size),
        ShapeExpr::Cone      { args } => stamp_cone     (args, cx, cy, cz, atom_id, grid, voxel_size),
        ShapeExpr::Heightfield{args } => stamp_heightfield(args, cx, cy, cz, atom_id, grid, voxel_size),
        ShapeExpr::Shell     { inner, args } => stamp_shell(inner, args, cx, cy, cz, atom_id, grid, voxel_size),
        ShapeExpr::Extrude   { profile, args } => stamp_extrude(profile, args, cx, cy, cz, atom_id, grid, voxel_size),
    }
}

// ── Sphere ─────────────────────────────────────────────────────────────────
//
// Fill every voxel whose centre is within `radius` world-units of (cx,cy,cz).
//
//   (dx*vs)² + (dy*vs)² + (dz*vs)²  ≤  radius²
//
// where dx,dy,dz are integer offsets from the centre voxel.

fn stamp_sphere(
    args: &[NamedArg],
    cx: i32, cy: i32, cz: i32,
    atom_id: u16, grid: &mut VoxelGrid, vs: f64,
) {
    let radius = arg_f64(args, "radius", 1.0);
    let r_vox  = (radius / vs).ceil() as i32;
    let r2     = radius * radius;

    for dy in -r_vox..=r_vox {
        for dz in -r_vox..=r_vox {
            for dx in -r_vox..=r_vox {
                let wx = dx as f64 * vs;
                let wy = dy as f64 * vs;
                let wz = dz as f64 * vs;
                if wx*wx + wy*wy + wz*wz <= r2 {
                    grid.set(cx+dx, cy+dy, cz+dz, atom_id);
                }
            }
        }
    }
}

// ── Cylinder ───────────────────────────────────────────────────────────────
//
// Vertical cylinder (axis = Y).
// A voxel is inside if:
//   • horizontal distance from axis ≤ radius
//   • dy is within [0, height]  (base at cy, cap at cy + height_vox)

fn stamp_cylinder(
    args: &[NamedArg],
    cx: i32, cy: i32, cz: i32,
    atom_id: u16, grid: &mut VoxelGrid, vs: f64,
) {
    let height = arg_f64(args, "height", 1.0);
    let radius = arg_f64(args, "radius", 0.5);
    let r_vox  = (radius / vs).ceil() as i32;
    let h_vox  = (height / vs).ceil() as i32;
    let r2     = radius * radius;

    for dy in 0..=h_vox {
        for dz in -r_vox..=r_vox {
            for dx in -r_vox..=r_vox {
                let wx = dx as f64 * vs;
                let wz = dz as f64 * vs;
                if wx*wx + wz*wz <= r2 {
                    grid.set(cx+dx, cy+dy, cz+dz, atom_id);
                }
            }
        }
    }
}

// ── Box ────────────────────────────────────────────────────────────────────

fn stamp_box(
    args: &[NamedArg],
    cx: i32, cy: i32, cz: i32,
    atom_id: u16, grid: &mut VoxelGrid, vs: f64,
) {
    let w = arg_f64(args, "width",  2.0);
    let h = arg_f64(args, "height", 2.0);
    let d = arg_f64(args, "depth",  2.0);

    let hw = (w / 2.0 / vs).ceil() as i32;
    let hh = (h / 2.0 / vs).ceil() as i32;
    let hd = (d / 2.0 / vs).ceil() as i32;

    for dy in -hh..=hh {
        for dz in -hd..=hd {
            for dx in -hw..=hw {
                grid.set(cx+dx, cy+dy, cz+dz, atom_id);
            }
        }
    }
}

// ── Ellipsoid ──────────────────────────────────────────────────────────────
//
// A voxel is inside if:
//   (dx*vs/rx)² + (dy*vs/ry)² + (dz*vs/rz)²  ≤  1

fn stamp_ellipsoid(
    args: &[NamedArg],
    cx: i32, cy: i32, cz: i32,
    atom_id: u16, grid: &mut VoxelGrid, vs: f64,
) {
    let rx = arg_f64(args, "rx", 1.0);
    let ry = arg_f64(args, "ry", 1.0);
    let rz = arg_f64(args, "rz", 1.0);

    let xv = (rx / vs).ceil() as i32;
    let yv = (ry / vs).ceil() as i32;
    let zv = (rz / vs).ceil() as i32;

    for dy in -yv..=yv {
        for dz in -zv..=zv {
            for dx in -xv..=xv {
                let fx = (dx as f64 * vs) / rx;
                let fy = (dy as f64 * vs) / ry;
                let fz = (dz as f64 * vs) / rz;
                if fx*fx + fy*fy + fz*fz <= 1.0 {
                    grid.set(cx+dx, cy+dy, cz+dz, atom_id);
                }
            }
        }
    }
}

// ── Blob ───────────────────────────────────────────────────────────────────
//
// An organic approximation: sphere with radius perturbed by low-frequency noise.
// We use a simple deterministic hash to simulate noise without dependencies.
//
//   effective_radius(dx,dz) = radius * (1 + roughness * hash_noise(dx, dz))
//
// This gives a lumpy but reproducible shape.

fn stamp_blob(
    args: &[NamedArg],
    cx: i32, cy: i32, cz: i32,
    atom_id: u16, grid: &mut VoxelGrid, vs: f64,
) {
    let radius    = arg_f64(args, "radius",    1.0);
    let roughness = arg_f64(args, "roughness", 0.2);
    let r_vox     = ((radius * (1.0 + roughness)) / vs).ceil() as i32;

    for dy in -r_vox..=r_vox {
        for dz in -r_vox..=r_vox {
            for dx in -r_vox..=r_vox {
                // Perturb radius based on direction
                let noise = hash_noise(dx, dy, dz);
                let eff_r = radius * (1.0 + roughness * noise);
                let wx = dx as f64 * vs;
                let wy = dy as f64 * vs;
                let wz = dz as f64 * vs;
                if wx*wx + wy*wy + wz*wz <= eff_r * eff_r {
                    grid.set(cx+dx, cy+dy, cz+dz, atom_id);
                }
            }
        }
    }
}

/// Deterministic noise in [-1, 1] for a given (dx,dy,dz) offset.
/// Uses integer bit-mixing — no floating point, fully reproducible.
fn hash_noise(dx: i32, dy: i32, dz: i32) -> f64 {
    let mut h = (dx as u64).wrapping_mul(2654435761)
        ^ (dy as u64).wrapping_mul(2246822519)
        ^ (dz as u64).wrapping_mul(3266489917);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
    h ^= h >> 33;
    // Map to [-1.0, 1.0]
    (h as i64 as f64) / (i64::MAX as f64)
}

// ── Cone ───────────────────────────────────────────────────────────────────
//
// Vertical cone, apex at top (cy + height), base at cy.
// At height y from base, the radius shrinks linearly: r(y) = radius * (1 - y/height)

fn stamp_cone(
    args: &[NamedArg],
    cx: i32, cy: i32, cz: i32,
    atom_id: u16, grid: &mut VoxelGrid, vs: f64,
) {
    let height = arg_f64(args, "height", 1.0);
    let radius = arg_f64(args, "radius", 0.5);
    let r_vox  = (radius / vs).ceil() as i32;
    let h_vox  = (height / vs).ceil() as i32;

    for dy in 0..=h_vox {
        let t       = dy as f64 / h_vox as f64; // 0 at base, 1 at apex
        let r_slice = radius * (1.0 - t);
        let r2      = r_slice * r_slice;
        for dz in -r_vox..=r_vox {
            for dx in -r_vox..=r_vox {
                let wx = dx as f64 * vs;
                let wz = dz as f64 * vs;
                if wx*wx + wz*wz <= r2 {
                    grid.set(cx+dx, cy+dy, cz+dz, atom_id);
                }
            }
        }
    }
}

// ── Heightfield ────────────────────────────────────────────────────────────
//
// Procedural terrain surface.  We use 2D value noise to generate elevation,
// then fill every voxel from y=0 up to the elevation at (x,z).

fn stamp_heightfield(
    args: &[NamedArg],
    cx: i32, cy: i32, cz: i32,
    atom_id: u16, grid: &mut VoxelGrid, vs: f64,
) {
    let radius     = arg_f64(args, "radius",     50.0);
    let max_height = arg_f64(args, "max_height", 20.0);
    let noise_amt  = arg_f64(args, "noise",      0.3);
    let seed       = arg_i64(args, "seed",       42) as u64;

    let r_vox  = (radius / vs).ceil() as i32;
    let mh_vox = (max_height / vs).ceil() as i32;

    for dz in -r_vox..=r_vox {
        for dx in -r_vox..=r_vox {
            // Circular island: fade elevation to zero near the edge
            let dist = ((dx*dx + dz*dz) as f64).sqrt() * vs;
            if dist > radius { continue; }
            let edge_fade = 1.0 - (dist / radius).powi(2);

            // 2D value noise at this (dx, dz) column
            let n = terrain_noise(dx, dz, seed, noise_amt);
            let elev_vox = ((n * edge_fade) * mh_vox as f64).round() as i32;

            for dy in 0..=elev_vox {
                grid.set(cx+dx, cy+dy, cz+dz, atom_id);
            }
        }
    }
}

/// 2D terrain noise: smooth pseudo-random in [0,1].
fn terrain_noise(dx: i32, dz: i32, seed: u64, scale: f64) -> f64 {
    // Sample at multiple octaves for natural-looking terrain
    let mut value  = 0.0f64;
    let mut amp    = 1.0f64;
    let mut freq   = scale;
    let mut max_v  = 0.0f64;

    for octave in 0..4u64 {
        let sx = (dx as f64 * freq) as i32;
        let sz = (dz as f64 * freq) as i32;
        let h = hash_noise_2d(sx, sz, seed ^ (octave * 1234567));
        value += (h * 0.5 + 0.5) * amp; // remap [-1,1] → [0,1]
        max_v += amp;
        amp  *= 0.5;
        freq *= 2.0;
    }

    value / max_v
}

fn hash_noise_2d(dx: i32, dz: i32, seed: u64) -> f64 {
    let mut h = (dx as u64).wrapping_mul(2654435761)
        ^ (dz as u64).wrapping_mul(3266489917)
        ^ seed.wrapping_mul(2246822519);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    (h as i64 as f64) / (i64::MAX as f64)
}

// ── Shell ──────────────────────────────────────────────────────────────────
//
// Fill the outer shape, then hollow out by over-writing with air (0)
// a smaller version of the same shape inset by `inner_offset` voxels.

fn stamp_shell(
    inner_shape: &ShapeExpr,
    args:        &[NamedArg],
    cx: i32, cy: i32, cz: i32,
    atom_id: u16, grid: &mut VoxelGrid, vs: f64,
) {
    let inner_offset = arg_f64(args, "inner_offset", 1.0);

    // 1. Stamp the outer shape solid
    stamp(inner_shape, cx, cy, cz, atom_id, grid, vs);

    // 2. Hollow it out by shrinking the shape and stamping with air
    let inset_shape = inset_shape(inner_shape, inner_offset);
    stamp(&inset_shape, cx, cy, cz, 0, grid, vs);
}

/// Return a copy of a shape with all radii/dimensions reduced by `offset`.
fn inset_shape(shape: &ShapeExpr, offset: f64) -> ShapeExpr {
    match shape {
        ShapeExpr::Sphere { args } =>
            ShapeExpr::Sphere { args: scale_arg(args, "radius", -offset) },
        ShapeExpr::Ellipsoid { args } => ShapeExpr::Ellipsoid {
            args: scale_args(args, &["rx","ry","rz"], -offset)
        },
        ShapeExpr::Cylinder { args } =>
            ShapeExpr::Cylinder { args: scale_args(args, &["radius"], -offset) },
        ShapeExpr::Box_ { args } =>
            ShapeExpr::Box_ { args: scale_args(args, &["width","height","depth"], -offset) },
        other => other.clone(),
    }
}

fn scale_arg(args: &[NamedArg], key: &str, delta: f64) -> Vec<NamedArg> {
    args.iter().map(|a| {
        if a.key == key {
            NamedArg { key: a.key.clone(), value: Expr::Float(arg_f64(args, key, 1.0) + delta) }
        } else {
            a.clone()
        }
    }).collect()
}

fn scale_args(args: &[NamedArg], keys: &[&str], delta: f64) -> Vec<NamedArg> {
    args.iter().map(|a| {
        if keys.contains(&a.key.as_str()) {
            NamedArg { key: a.key.clone(), value: Expr::Float(arg_f64(args, &a.key, 1.0) + delta) }
        } else {
            a.clone()
        }
    }).collect()
}

// ── Extrude ────────────────────────────────────────────────────────────────
//
// Extrude a 2D profile shape upward by `height` voxels.
// We take an XZ cross-section of the profile at y=0 and repeat it vertically.

fn stamp_extrude(
    profile:    &ShapeExpr,
    args:       &[NamedArg],
    cx: i32, cy: i32, cz: i32,
    atom_id: u16, grid: &mut VoxelGrid, vs: f64,
) {
    let height = arg_f64(args, "height", 1.0);
    let h_vox  = (height / vs).ceil() as i32;

    // Stamp the profile at each vertical slice
    for dy in 0..=h_vox {
        stamp(profile, cx, cy+dy, cz, atom_id, grid, vs);
    }
}

// ── Bounding radius helper ─────────────────────────────────────────────────

pub fn bounding_radius(shape: &ShapeExpr) -> f64 {
    match shape {
        ShapeExpr::Sphere    { args } => arg_f64(args, "radius", 1.0),
        ShapeExpr::Cylinder  { args } => {
            let r = arg_f64(args, "radius", 0.5);
            let h = arg_f64(args, "height", 1.0);
            r.max(h)
        }
        ShapeExpr::Box_      { args } => {
            let w = arg_f64(args, "width",  2.0);
            let h = arg_f64(args, "height", 2.0);
            let d = arg_f64(args, "depth",  2.0);
            (w*w + h*h + d*d).sqrt() / 2.0
        }
        ShapeExpr::Ellipsoid { args } => {
            let rx = arg_f64(args, "rx", 1.0);
            let ry = arg_f64(args, "ry", 1.0);
            let rz = arg_f64(args, "rz", 1.0);
            rx.max(ry).max(rz)
        }
        ShapeExpr::Blob      { args } => {
            let r = arg_f64(args, "radius",    1.0);
            let n = arg_f64(args, "roughness", 0.2);
            r * (1.0 + n)
        }
        ShapeExpr::Cone      { args } => {
            arg_f64(args, "radius", 0.5).max(arg_f64(args, "height", 1.0))
        }
        ShapeExpr::Heightfield { args } => arg_f64(args, "radius", 50.0),
        ShapeExpr::Shell { inner, .. } => bounding_radius(inner),
        ShapeExpr::Extrude { profile, args } => {
            bounding_radius(profile).max(arg_f64(args, "height", 1.0))
        }
    }
}

// ── Named argument helpers ─────────────────────────────────────────────────

pub fn arg_f64(args: &[NamedArg], key: &str, default: f64) -> f64 {
    args.iter().find(|a| a.key == key).map(|a| match &a.value {
        Expr::Float(f) => *f,
        Expr::Int(n)   => *n as f64,
        _              => default,
    }).unwrap_or(default)
}

pub fn arg_i64(args: &[NamedArg], key: &str, default: i64) -> i64 {
    args.iter().find(|a| a.key == key).map(|a| match &a.value {
        Expr::Int(n)   => *n,
        Expr::Float(f) => *f as i64,
        _              => default,
    }).unwrap_or(default)
}