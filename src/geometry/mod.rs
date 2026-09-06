// src/geometry/mod.rs
//
// Phase B1: shapes are CONTAINMENT FUNCTIONS.
//
// The old pipeline stamped each shape into its own grid at origin, then
// rotated integer voxels — which is exact only for the 24 axis-aligned
// orientations, so the realizer refused everything else. This module
// inverts the sampling direction: for each world voxel center p, test
// contains(shape, F⁻¹·p). Any rigid frame F realizes exactly; the
// NonAxisAlignedRotation restriction is deleted, not patched.
//
// Determinism: blob and heightfield noise is keyed on SHAPE-LOCAL voxel
// indices (round(p_local/vs)), so a rotated blob is the same blob, and
// identity-placed terrain reproduces the Phase-A island exactly.
//
// Parity notes vs the stamp-then-offset path:
//   • vertical/box extents keep the old index-based bounds (ceil + the
//     inclusive cap layer), so part heights match the old output;
//   • radial tests are continuous (|p| ≤ r), the natural generalization;
//   • positions are rounded ONCE (at sampling) instead of twice
//     (grid-center + offset), so surfaces can shift ≤ 1 voxel vs Phase A.

use crate::anchors::analytic_extents;
use crate::ast::{Expr, NamedArg, ShapeExpr};
use crate::frame::{Frame, Mat3, Vec3};
use crate::resolver::{ResolvedEntity, ResolvedScene};
use crate::voxel::VoxelGrid;

// ── Public types ───────────────────────────────────────────────────────────
//
// Compilation no longer pre-stamps grids — geometry happens once, at
// rasterization, with solved frames in hand. CompiledPart is now just the
// resolved material binding the rasterizer needs.

#[derive(Debug)]
pub struct CompiledPart {
    pub name:       String,
    pub atom_id:    u16,
    pub voxel_size: f64,
}

#[derive(Debug)]
pub struct CompiledEntity {
    pub name:       String,
    pub parts:      Vec<CompiledPart>,
    pub voxel_size: f64,
}

// ── Compilation (material binding only) ────────────────────────────────────

pub fn compile(scene: &ResolvedScene, voxel_size: f64) -> Vec<CompiledEntity> {
    scene.entities.iter().map(|ent| {
        let vs = ent.resolve.as_ref().map(|r| r.voxel_size).unwrap_or(voxel_size);
        compile_entity(ent, scene, vs)
    }).collect()
}

fn compile_entity(ent: &ResolvedEntity, scene: &ResolvedScene, voxel_size: f64) -> CompiledEntity {
    let mut parts = Vec::new();
    for part in &ent.parts {
        let atom_id = part.material_index
            .and_then(|mi| scene.materials.get(mi))
            .map(|mat| (mat.atom_index as u16) + 1)
            .unwrap_or(1);
        if part.shape.is_some() {
            parts.push(CompiledPart { name: part.name.clone(), atom_id, voxel_size });
        }
    }
    CompiledEntity { name: ent.name.clone(), parts, voxel_size }
}

// ── Containment ────────────────────────────────────────────────────────────
//
// `p` is in SHAPE-LOCAL world units. `vs` is the voxel size — needed only
// where the old stampers were index-based (vertical extents, box bounds)
// or keyed noise on integer offsets (blob, heightfield), so those results
// are preserved exactly at identity placement.

pub fn contains(shape: &ShapeExpr, p: Vec3, vs: f64) -> bool {
    match shape {
        ShapeExpr::Sphere { args } => {
            let r = arg_f64(args, "radius", 1.0);
            p.x * p.x + p.y * p.y + p.z * p.z <= r * r
        }

        ShapeExpr::Ellipsoid { args } => {
            let rx = arg_f64(args, "rx", 1.0);
            let ry = arg_f64(args, "ry", 1.0);
            let rz = arg_f64(args, "rz", 1.0);
            let (fx, fy, fz) = (p.x / rx, p.y / ry, p.z / rz);
            fx * fx + fy * fy + fz * fz <= 1.0
        }

        ShapeExpr::Box_ { args } => {
            let hw = (arg_f64(args, "width",  2.0) / 2.0 / vs).ceil() as i32;
            let hh = (arg_f64(args, "height", 2.0) / 2.0 / vs).ceil() as i32;
            let hd = (arg_f64(args, "depth",  2.0) / 2.0 / vs).ceil() as i32;
            let (kx, ky, kz) = local_key(p, vs);
            kx.abs() <= hw && ky.abs() <= hh && kz.abs() <= hd
        }

        ShapeExpr::Cylinder { args } => {
            let h = arg_f64(args, "height", 1.0);
            let r = arg_f64(args, "radius", 0.5);
            let h_vox = (h / vs).ceil() as i32;
            let ky = (p.y / vs).round() as i32;
            ky >= 0 && ky <= h_vox && p.x * p.x + p.z * p.z <= r * r
        }

        ShapeExpr::Cone { args } => {
            let h = arg_f64(args, "height", 1.0);
            let r = arg_f64(args, "radius", 0.5);
            let h_vox = (h / vs).ceil().max(1.0) as i32;
            let ky = (p.y / vs).round() as i32;
            if ky < 0 || ky > h_vox { return false; }
            let t  = ky as f64 / h_vox as f64; // 0 at base, 1 at apex
            let rs = r * (1.0 - t);
            p.x * p.x + p.z * p.z <= rs * rs
        }

        ShapeExpr::Blob { args } => {
            // Noise keyed on LOCAL voxel indices — rotation-invariant lumps.
            let radius    = arg_f64(args, "radius",    1.0);
            let roughness = arg_f64(args, "roughness", 0.2);
            let (kx, ky, kz) = local_key(p, vs);
            let eff_r = radius * (1.0 + roughness * hash_noise(kx, ky, kz));
            p.x * p.x + p.y * p.y + p.z * p.z <= eff_r * eff_r
        }

        ShapeExpr::Heightfield { args } => {
            let radius     = arg_f64(args, "radius",     50.0);
            let max_height = arg_f64(args, "max_height", 20.0);
            let noise_amt  = arg_f64(args, "noise",      0.3);
            let seed       = arg_i64(args, "seed",       42) as u64;

            let (kx, ky, kz) = local_key(p, vs);
            let dist = (((kx * kx + kz * kz) as f64).sqrt()) * vs;
            if dist > radius || ky < 0 { return false; }
            let edge_fade = 1.0 - (dist / radius).powi(2);

            let mh_vox = (max_height / vs).ceil() as i32;
            let n = terrain_noise(kx, kz, seed, noise_amt);
            let elev_vox = ((n * edge_fade) * mh_vox as f64).round() as i32;
            ky <= elev_vox
        }

        // A shell is its outer shape minus the inset copy of itself.
        ShapeExpr::Shell { inner, args } => {
            let inner_offset = arg_f64(args, "inner_offset", 1.0);
            contains(inner, p, vs) && !contains(&inset_shape(inner, inner_offset), p, vs)
        }

        // Extrusion = union of the profile stamped at each vertical slice
        // (the old stamper's semantics, faithfully).
        ShapeExpr::Extrude { profile, args } => {
            let h_vox = (arg_f64(args, "height", 1.0) / vs).ceil() as i32;
            (0..=h_vox).any(|k| {
                contains(profile, Vec3::new(p.x, p.y - k as f64 * vs, p.z), vs)
            })
        }

        // ── CSG combinators (Phase B2) ────────────────────────────────────
        // Predicates compose: this is the entire CSG implementation.
        // A BLENDED union has no predicate form — the fillet is a
        // property of the distance field — so it defers to `distance`.
        ShapeExpr::Union { shapes, args } => {
            if arg_f64(args, "blend", 0.0) > 0.0 {
                distance(shape, p, vs) <= 0.0
            } else {
                shapes.iter().any(|s| contains(s, p, vs))
            }
        }
        ShapeExpr::Intersect { shapes } =>
            !shapes.is_empty() && shapes.iter().all(|s| contains(s, p, vs)),
        ShapeExpr::Difference { base, cuts } =>
            contains(base, p, vs) && !cuts.iter().any(|c| contains(c, p, vs)),

        // Local transform wrappers: test the child at the inverse-
        // transformed point.
        ShapeExpr::At { inner, args } => {
            let t = Vec3::new(
                arg_f64(args, "x", 0.0),
                arg_f64(args, "y", 0.0),
                arg_f64(args, "z", 0.0),
            );
            contains(inner, p.sub(t), vs)
        }
        ShapeExpr::Spin { inner, args } => {
            contains(inner, spin_rot(args).transpose().apply(p), vs)
        }
    }
}

// ── Signed distance ────────────────────────────────────────────────────────
//
// `distance(shape, p, vs)` is the SDF companion of `contains`: negative
// inside, positive outside, zero on the surface, world units. Primitives
// are exact (sphere, box, cylinder, cone) or conservative bounds
// (ellipsoid, blob, heightfield, extrude) — good enough for meshing and
// for raymarching with a step factor below 1. CSG is min / max.
//
// `union(…, blend=k)` is a polynomial smooth-min, and it is why this
// exists: two spheres become a shoulder instead of an intersection.
//
// Formulas follow Inigo Quilez's SDF catalogue. Shape-local origins match
// `contains` and `anchors.rs`: centered for sphere/ellipsoid/blob/box,
// base at origin for cylinder/cone/heightfield/extrude.

pub fn distance(shape: &ShapeExpr, p: Vec3, vs: f64) -> f64 {
    match shape {
        ShapeExpr::Sphere { args } => p.length() - arg_f64(args, "radius", 1.0),

        // Quilez's bound: k0·(k0−1)/k1. Not exact, but never overshoots.
        ShapeExpr::Ellipsoid { args } => {
            let (rx, ry, rz) = (arg_f64(args, "rx", 1.0), arg_f64(args, "ry", 1.0), arg_f64(args, "rz", 1.0));
            let k0 = Vec3::new(p.x / rx, p.y / ry, p.z / rz).length();
            let k1 = Vec3::new(p.x / (rx * rx), p.y / (ry * ry), p.z / (rz * rz)).length();
            if k1 < 1e-12 { -rx.min(ry).min(rz) } else { k0 * (k0 - 1.0) / k1 }
        }

        ShapeExpr::Box_ { args } => {
            let b = Vec3::new(
                arg_f64(args, "width",  2.0) / 2.0,
                arg_f64(args, "height", 2.0) / 2.0,
                arg_f64(args, "depth",  2.0) / 2.0,
            );
            let q = Vec3::new(p.x.abs() - b.x, p.y.abs() - b.y, p.z.abs() - b.z);
            let outside = Vec3::new(q.x.max(0.0), q.y.max(0.0), q.z.max(0.0)).length();
            let inside  = q.x.max(q.y).max(q.z).min(0.0);
            outside + inside
        }

        // Base at origin, axis +Y: recenter to ±h/2 then the capped-
        // cylinder formula.
        ShapeExpr::Cylinder { args } => {
            let h = arg_f64(args, "height", 1.0);
            let r = arg_f64(args, "radius", 0.5);
            let dxz = (p.x * p.x + p.z * p.z).sqrt() - r;
            let dy  = (p.y - h / 2.0).abs() - h / 2.0;
            let (ox, oy) = (dxz.max(0.0), dy.max(0.0));
            dxz.max(dy).min(0.0) + (ox * ox + oy * oy).sqrt()
        }

        // Capped cone with the top radius zero, recentered to ±h/2.
        ShapeExpr::Cone { args } => {
            let h  = arg_f64(args, "height", 1.0);
            let r1 = arg_f64(args, "radius", 0.5);
            let hh = h / 2.0;
            let q  = ((p.x * p.x + p.z * p.z).sqrt(), p.y - hh);
            let k1 = (0.0, hh);
            let k2 = (-r1, 2.0 * hh);
            let ca = (q.0 - q.0.min(if q.1 < 0.0 { r1 } else { 0.0 }), q.1.abs() - hh);
            let t  = (((k1.0 - q.0) * k2.0 + (k1.1 - q.1) * k2.1) / (k2.0 * k2.0 + k2.1 * k2.1)).clamp(0.0, 1.0);
            let cb = (q.0 - k1.0 + k2.0 * t, q.1 - k1.1 + k2.1 * t);
            let s  = if cb.0 < 0.0 && ca.1 < 0.0 { -1.0 } else { 1.0 };
            s * (ca.0 * ca.0 + ca.1 * ca.1).min(cb.0 * cb.0 + cb.1 * cb.1).sqrt()
        }

        // Nominal sphere with the same voxel-keyed noise `contains` uses,
        // so the two agree; not Lipschitz, which raymarchers must tolerate.
        ShapeExpr::Blob { args } => {
            let radius    = arg_f64(args, "radius",    1.0);
            let roughness = arg_f64(args, "roughness", 0.2);
            let (kx, ky, kz) = local_key(p, vs);
            p.length() - radius * (1.0 + roughness * hash_noise(kx, ky, kz))
        }

        // Height above the terrain, clipped to the disc: a bound, not an
        // SDF, but its zero set is the terrain surface.
        ShapeExpr::Heightfield { args } => {
            let radius     = arg_f64(args, "radius",     50.0);
            let max_height = arg_f64(args, "max_height", 20.0);
            let noise_amt  = arg_f64(args, "noise",      0.3);
            let seed       = arg_i64(args, "seed",       42) as u64;

            let (kx, _, kz) = local_key(p, vs);
            let rxz = (p.x * p.x + p.z * p.z).sqrt();
            let edge_fade = (1.0 - (rxz / radius).powi(2)).max(0.0);
            let mh_vox = (max_height / vs).ceil();
            let elev = (terrain_noise(kx, kz, seed, noise_amt) * edge_fade * mh_vox).round() * vs;
            (p.y - elev).max(rxz - radius).max(-p.y)
        }

        // The band between the surface (d = 0) and `inner_offset` inside
        // it (d = −offset): exactly what `contains` computes via inset.
        ShapeExpr::Shell { inner, args } => {
            let offset = arg_f64(args, "inner_offset", 1.0);
            let d = distance(inner, p, vs);
            d.max(-d - offset)
        }

        // Profile swept along +Y over [0, h]: evaluate the profile at the
        // nearest sweep parameter.
        ShapeExpr::Extrude { profile, args } => {
            let h = arg_f64(args, "height", 1.0);
            let py = p.y - p.y.clamp(0.0, h);
            distance(profile, Vec3::new(p.x, py, p.z), vs)
        }

        ShapeExpr::Union { shapes, args } => {
            let blend = arg_f64(args, "blend", 0.0);
            let mut it = shapes.iter().map(|s| distance(s, p, vs));
            let first = it.next().unwrap_or(f64::MAX);
            if blend > 0.0 {
                it.fold(first, |a, b| smooth_min(a, b, blend))
            } else {
                it.fold(first, f64::min)
            }
        }
        ShapeExpr::Intersect { shapes } =>
            shapes.iter().map(|s| distance(s, p, vs)).fold(f64::MIN, f64::max),
        ShapeExpr::Difference { base, cuts } =>
            cuts.iter().fold(distance(base, p, vs), |d, c| d.max(-distance(c, p, vs))),

        ShapeExpr::At { inner, args } => {
            let t = Vec3::new(
                arg_f64(args, "x", 0.0),
                arg_f64(args, "y", 0.0),
                arg_f64(args, "z", 0.0),
            );
            distance(inner, p.sub(t), vs)
        }
        ShapeExpr::Spin { inner, args } =>
            distance(inner, spin_rot(args).transpose().apply(p), vs),
    }
}

/// Polynomial smooth minimum. `k` is the blend radius in world units:
/// the surfaces are joined by a fillet roughly that wide.
pub fn smooth_min(a: f64, b: f64, k: f64) -> f64 {
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b * (1.0 - h) + a * h - k * h * (1.0 - h)
}

/// Outward surface normal by central differences on the distance field.
pub fn gradient(shape: &ShapeExpr, p: Vec3, vs: f64, eps: f64) -> Vec3 {
    let d = |q: Vec3| distance(shape, q, vs);
    let n = Vec3::new(
        d(Vec3::new(p.x + eps, p.y, p.z)) - d(Vec3::new(p.x - eps, p.y, p.z)),
        d(Vec3::new(p.x, p.y + eps, p.z)) - d(Vec3::new(p.x, p.y - eps, p.z)),
        d(Vec3::new(p.x, p.y, p.z + eps)) - d(Vec3::new(p.x, p.y, p.z - eps)),
    );
    n.normalize().unwrap_or(Vec3::Y)
}

#[inline]
fn local_key(p: Vec3, vs: f64) -> (i32, i32, i32) {
    (
        (p.x / vs).round() as i32,
        (p.y / vs).round() as i32,
        (p.z / vs).round() as i32,
    )
}

// ── Rasterization (the ONE voxel-producing step) ──────────────────────────
//
// Input: shaped parts with their SOLVED world frames and atom ids.
// Output: a merged grid whose (0,0,0) is the world minimum of the union
// AABB — the same origin convention merge_parts used, so main.rs layer
// centering is untouched. Later parts overwrite earlier (declaration
// order), matching the old merge behavior.

pub fn rasterize_entity(
    parts: &[(String, ShapeExpr, u16, Frame)],
    vs:    f64,
) -> VoxelGrid {
    // Per-part and global AABBs in voxel coordinates.
    let boxes: Vec<((i32, i32, i32), (i32, i32, i32))> = parts.iter()
        .map(|(_, shape, _, frame)| vox_aabb(shape, frame, vs))
        .collect();

    let mut gmin = (i32::MAX, i32::MAX, i32::MAX);
    let mut gmax = (i32::MIN, i32::MIN, i32::MIN);
    for (lo, hi) in &boxes {
        gmin = (gmin.0.min(lo.0), gmin.1.min(lo.1), gmin.2.min(lo.2));
        gmax = (gmax.0.max(hi.0), gmax.1.max(hi.1), gmax.2.max(hi.2));
    }
    if gmin.0 == i32::MAX {
        return VoxelGrid::new(1, 1, 1);
    }

    let w = (gmax.0 - gmin.0 + 1) as u32;
    let h = (gmax.1 - gmin.1 + 1) as u32;
    let d = (gmax.2 - gmin.2 + 1) as u32;
    let mut grid = VoxelGrid::new(w, h, d);

    for ((_, shape, atom_id, frame), (lo, hi)) in parts.iter().zip(&boxes) {
        let inv = frame.inverse();
        for vy in lo.1..=hi.1 {
            for vz in lo.2..=hi.2 {
                for vx in lo.0..=hi.0 {
                    let pw = Vec3::new(vx as f64 * vs, vy as f64 * vs, vz as f64 * vs);
                    if contains(shape, inv.apply_point(pw), vs) {
                        grid.set(vx - gmin.0, vy - gmin.1, vz - gmin.2, *atom_id);
                    }
                }
            }
        }
    }

    grid
}

/// World AABB of a placed shape in voxel coordinates: the analytic extents'
/// 8 corners through the frame, padded 2 voxels for index-based overhang.
fn vox_aabb(shape: &ShapeExpr, frame: &Frame, vs: f64) -> ((i32, i32, i32), (i32, i32, i32)) {
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
    (
        (
            (min.x / vs).floor() as i32 - 2,
            (min.y / vs).floor() as i32 - 2,
            (min.z / vs).floor() as i32 - 2,
        ),
        (
            (max.x / vs).ceil() as i32 + 2,
            (max.y / vs).ceil() as i32 + 2,
            (max.z / vs).ceil() as i32 + 2,
        ),
    )
}

// ── Shape helpers ──────────────────────────────────────────────────────────

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

// ── Noise (unchanged formulas — shared with anchors.rs) ───────────────────

/// Deterministic noise in [-1, 1] for a given integer key.
fn hash_noise(dx: i32, dy: i32, dz: i32) -> f64 {
    let mut h = (dx as u64).wrapping_mul(2654435761)
        ^ (dy as u64).wrapping_mul(2246822519)
        ^ (dz as u64).wrapping_mul(3266489917);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
    h ^= h >> 33;
    (h as i64 as f64) / (i64::MAX as f64)
}

/// 2D terrain noise: smooth pseudo-random in [0,1].
/// pub(crate): shared with anchors.rs so heightfield `surface(x, z)` anchors
/// sit exactly on the stamped surface — one elevation function, two consumers.
pub(crate) fn terrain_noise(dx: i32, dz: i32, seed: u64, scale: f64) -> f64 {
    let mut value  = 0.0f64;
    let mut amp    = 1.0f64;
    let mut freq   = scale;
    let mut max_v  = 0.0f64;

    for octave in 0..4u64 {
        let sx = (dx as f64 * freq) as i32;
        let sz = (dz as f64 * freq) as i32;
        let h = hash_noise_2d(sx, sz, seed ^ (octave * 1234567));
        value += (h * 0.5 + 0.5) * amp;
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

/// Read an identifier-or-string argument (e.g. `axis=z`).
pub fn arg_str(args: &[NamedArg], key: &str) -> Option<String> {
    args.iter().find(|a| a.key == key).and_then(|a| match &a.value {
        Expr::Ident(i) => Some(i.name.clone()),
        Expr::Str(s)   => Some(s.clone()),
        _              => None,
    })
}

/// The rotation of a `spin(shape, axis=…, degrees=…)` wrapper.
/// pub(crate): shared with anchors.rs so extents and anchors ride the
/// same rotation the containment predicate uses. Default axis: Y.
pub(crate) fn spin_rot(args: &[NamedArg]) -> Mat3 {
    let deg = arg_f64(args, "degrees", 0.0).to_radians();
    match arg_str(args, "axis").as_deref() {
        Some("x") | Some("X") => Mat3::rot_x(deg),
        Some("z") | Some("Z") => Mat3::rot_z(deg),
        _                     => Mat3::rot_y(deg),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Mat3;

    fn fnum(v: f64) -> Expr { Expr::Float(v) }
    fn na(k: &str, v: f64) -> NamedArg { NamedArg { key: k.into(), value: fnum(v) } }

    fn sphere(r: f64) -> ShapeExpr { ShapeExpr::Sphere { args: vec![na("radius", r)] } }
    fn cylinder(h: f64, r: f64) -> ShapeExpr {
        ShapeExpr::Cylinder { args: vec![na("height", h), na("radius", r)] }
    }

    fn part(shape: ShapeExpr, frame: Frame) -> (String, ShapeExpr, u16, Frame) {
        ("P".to_string(), shape, 1, frame)
    }

    /// The headline: a cylinder at 45° — impossible in Phase A — lands
    /// exactly where the frame says, and NOT on the unrotated axis.
    #[test]
    fn rotated_cylinder_lands_where_frame_says() {
        let deg45 = std::f64::consts::FRAC_PI_4;
        let frame = Frame::new(Mat3::rot_z(deg45), Vec3::ZERO);
        let parts = vec![part(cylinder(8.0, 1.0), frame)];
        let grid  = rasterize_entity(&parts, 1.0);

        // gmin from the AABB: recompute to index into the grid.
        let (lo, _) = super::vox_aabb(&parts[0].1, &parts[0].3, 1.0);

        // Local (0, 6, 0) → world rot_z(45°)·(0,6,0) = (−4.24, 4.24, 0).
        assert!(grid.get(-4 - lo.0, 4 - lo.1, 0 - lo.2) != 0,
                "point on the rotated axis must be filled");
        // Straight up (0, 6, 0) is far off the rotated axis → empty.
        assert!(grid.get(0 - lo.0, 6 - lo.1, 0 - lo.2) == 0,
                "the unrotated axis must be empty");
    }

    /// Local-space noise keying: a quarter-turned blob is the SAME blob —
    /// identical voxel count, just rotated.
    #[test]
    fn blob_is_deterministic_under_rotation() {
        let blob = ShapeExpr::Blob { args: vec![na("radius", 3.0), na("roughness", 0.35)] };
        let a = rasterize_entity(&[part(blob.clone(), Frame::IDENTITY)], 1.0);
        let b = rasterize_entity(
            &[part(blob, Frame::new(Mat3::rot_y(std::f64::consts::FRAC_PI_2), Vec3::ZERO))],
            1.0,
        );
        assert_eq!(a.filled_count(), b.filled_count());
    }

    /// Shell predicate: outer minus inset — hollow center, solid wall.
    #[test]
    fn shell_is_hollow() {
        let shell = ShapeExpr::Shell {
            inner: Box::new(sphere(4.0)),
            args:  vec![na("inner_offset", 1.5)],
        };
        assert!(!contains(&shell, Vec3::ZERO, 1.0), "center must be hollow");
        assert!(contains(&shell, Vec3::new(3.6, 0.0, 0.0), 1.0), "wall must be solid");
        assert!(!contains(&shell, Vec3::new(4.5, 0.0, 0.0), 1.0), "outside must be empty");
    }

    /// Identity placement reproduces the Phase-A sphere footprint exactly:
    /// same fill condition, same count.
    #[test]
    fn identity_sphere_matches_stamped_footprint() {
        let r = 2.0;
        let grid = rasterize_entity(&[part(sphere(r), Frame::IDENTITY)], 1.0);

        let mut expected = 0usize;
        let rv = r.ceil() as i32;
        for dy in -rv..=rv {
            for dz in -rv..=rv {
                for dx in -rv..=rv {
                    let (x, y, z) = (dx as f64, dy as f64, dz as f64);
                    if x * x + y * y + z * z <= r * r { expected += 1; }
                }
            }
        }
        assert_eq!(grid.filled_count(), expected);
    }

    // ── Phase B2: CSG ─────────────────────────────────────────────────────

    /// difference(box, vertical cylinder) — hollow bore through the
    /// middle, corners intact.
    #[test]
    fn difference_cuts_a_hole() {
        let holed = ShapeExpr::Difference {
            base: Box::new(ShapeExpr::Box_ {
                args: vec![na("width", 8.0), na("height", 6.0), na("depth", 8.0)],
            }),
            cuts: vec![ShapeExpr::At {
                inner: Box::new(cylinder(8.0, 1.5)),
                args:  vec![na("y", -4.0)],
            }],
        };
        assert!(!contains(&holed, Vec3::ZERO, 1.0), "bore must be empty");
        assert!(contains(&holed, Vec3::new(3.0, 0.0, 3.0), 1.0), "corner must be solid");
    }

    /// spin(cylinder, axis=z, degrees=90) aims the axis at −X: points on
    /// the spun axis are inside, points on the original +Y axis are not.
    #[test]
    fn spin_aims_the_axis()
    {
        let spun = ShapeExpr::Spin {
            inner: Box::new(cylinder(8.0, 1.0)),
            args:  vec![
                NamedArg { key: "axis".into(), value: Expr::Ident(crate::ast::Ident {
                    name: "z".into(), span: crate::error::Span::new(1, 1),
                }) },
                na("degrees", 90.0),
            ],
        };
        assert!(contains(&spun, Vec3::new(-4.0, 0.0, 0.0), 1.0), "spun axis at −X");
        assert!(!contains(&spun, Vec3::new(4.0, 0.0, 0.0), 1.0), "+X is outside");
        assert!(!contains(&spun, Vec3::new(0.0, 4.0, 0.0), 1.0), "original axis is outside");
    }

    /// union(sphere, at(sphere, x=6)) — both lobes filled, the gap empty.
    #[test]
    fn union_of_offset_spheres() {
        let pair = ShapeExpr::Union {
            shapes: vec![
                sphere(2.0),
                ShapeExpr::At { inner: Box::new(sphere(2.0)), args: vec![na("x", 6.0)] },
            ],
            args: vec![],
        };
        assert!(contains(&pair, Vec3::ZERO, 1.0));
        assert!(contains(&pair, Vec3::new(6.0, 0.0, 0.0), 1.0));
        assert!(!contains(&pair, Vec3::new(3.5, 0.0, 0.0), 1.0), "gap between lobes");
    }

    // ── Signed distance ───────────────────────────────────────────────

    /// A sphere's distance is exact: radius subtracted from the norm.
    #[test]
    fn sphere_distance_is_exact() {
        let s = sphere(2.0);
        assert!((distance(&s, Vec3::new(3.0, 0.0, 0.0), 1.0) - 1.0).abs() < 1e-12);
        assert!((distance(&s, Vec3::ZERO, 1.0) + 2.0).abs() < 1e-12);
        assert!(distance(&s, Vec3::new(0.0, 2.0, 0.0), 1.0).abs() < 1e-12, "zero on the surface");
    }

    /// `contains` and `distance` agree on sign for the primitives, away
    /// from the voxel-index cap layers where `contains` keeps its Phase-A
    /// ceil semantics.
    #[test]
    fn distance_sign_agrees_with_contains() {
        let shapes = vec![
            sphere(3.0),
            cylinder(8.0, 2.0),
            ShapeExpr::Box_ { args: vec![na("width", 6.0), na("height", 4.0), na("depth", 2.0)] },
            ShapeExpr::Ellipsoid { args: vec![na("rx", 4.0), na("ry", 2.0), na("rz", 3.0)] },
        ];
        let probes = [
            Vec3::ZERO, Vec3::new(1.0, 1.0, 0.5), Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(0.0, 20.0, 0.0), Vec3::new(-2.5, 0.5, 0.3), Vec3::new(0.0, -9.0, 0.0),
        ];
        for s in &shapes {
            for &p in &probes {
                let d = distance(s, p, 1.0);
                if d.abs() < 0.6 { continue; } // skip the cap-layer ambiguity band
                assert_eq!(contains(s, p, 1.0), d < 0.0, "{s:?} at {p:?}: d={d}");
            }
        }
    }

    /// The reason the SDF exists: two spheres 1 unit apart do not touch
    /// under a plain union, and DO under a blended one — the gap between
    /// them fills with a fillet. Voxel output shows it too, because
    /// `contains` defers to `distance` when blend > 0.
    #[test]
    fn blended_union_bridges_the_gap() {
        let lobes = |blend: f64| ShapeExpr::Union {
            shapes: vec![
                sphere(2.0),
                ShapeExpr::At { inner: Box::new(sphere(2.0)), args: vec![na("x", 5.0)] },
            ],
            args: if blend > 0.0 { vec![na("blend", blend)] } else { vec![] },
        };
        // Each surface is 0.5 from the midpoint, so smooth_min there is
        // 0.5 − k/4: the gap closes strictly above k = 2, not at it.
        let mid = Vec3::new(2.5, 0.0, 0.0);
        assert!(distance(&lobes(0.0), mid, 1.0) > 0.0, "plain union leaves a gap");
        assert!(!contains(&lobes(0.0), mid, 1.0));

        // Blend pulls the field down monotonically...
        let plain = distance(&lobes(0.0), mid, 1.0);
        let some  = distance(&lobes(1.0), mid, 1.0);
        let more  = distance(&lobes(3.0), mid, 1.0);
        assert!(some < plain, "blend must lower the field between the lobes");
        assert!(more < some,  "more blend, lower still");

        // ...and past the threshold the gap is closed, in the voxel
        // backend too, since `contains` defers to `distance` when blending.
        assert!(more < 0.0, "blend=3 fills the gap, got {more}");
        assert!(contains(&lobes(3.0), mid, 1.0), "and the voxel backend sees the fillet");

        // Away from the join, blending changes nothing: each lobe's own
        // surface is where it was.
        let far = Vec3::new(-2.0, 0.0, 0.0);
        assert!((distance(&lobes(3.0), far, 1.0) - distance(&lobes(0.0), far, 1.0)).abs() < 1e-9,
                "the fillet is local to the join");
    }

    /// A shell is the band between the surface and `inner_offset` inside it.
    #[test]
    fn shell_distance_is_hollow_inside_and_solid_in_the_wall() {
        let shell = ShapeExpr::Shell {
            inner: Box::new(sphere(4.0)),
            args:  vec![na("inner_offset", 1.5)],
        };
        assert!(distance(&shell, Vec3::ZERO, 1.0) > 0.0, "centre is outside the wall");
        assert!(distance(&shell, Vec3::new(3.5, 0.0, 0.0), 1.0) < 0.0, "inside the wall");
        assert!(distance(&shell, Vec3::new(5.0, 0.0, 0.0), 1.0) > 0.0, "outside");
    }
}