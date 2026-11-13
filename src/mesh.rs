//! Surface extraction from signed distance fields — naive surface nets.
//!
//! For every grid cell whose corners straddle the surface, one vertex is
//! placed at the mean of the edge crossings; for every grid edge that
//! crosses the surface, the four cells around it are joined into a quad.
//! No case tables, and vertices sit ON the surface rather than on grid
//! edges, so the result is smooth on curves and reasonably crisp on
//! creases. Dual contouring with a QEF is the upgrade if exact corners
//! are ever needed.
//!
//! Meshes are SHAPE-LOCAL; place them with the part's frame.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use crate::anchors::analytic_extents;
use crate::ast::ShapeExpr;
use crate::frame::{Frame, Vec3};
use crate::geometry::distance;
use crate::scene::Scene;

#[derive(Debug, Clone, Default)]
pub struct TriMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals:   Vec<[f32; 3]>,
    pub indices:   Vec<u32>,
}

/// The most field samples the automatic cell asks for on one part (8 bytes
/// each plus 4 for the vertex index: ~400 MB). Only a part far larger than
/// its scene's voxel size reaches it: a 260 m ground in a scene resolved at
/// 5 cm for its door hinges.
pub const MAX_MESH_SAMPLES: f64 = 32_000_000.0;

/// Choose a cell size: half the voxel size, unless that would mean more
/// than ~2M samples for this shape, in which case the voxel size itself —
/// and if even that is more than `MAX_MESH_SAMPLES`, the cell that gives
/// that many. Below the cap this is exactly the old rule.
pub fn auto_cell(shape: &ShapeExpr, vs: f64) -> f64 {
    let e = analytic_extents(shape);
    let span = (e.max.x - e.min.x) * (e.max.y - e.min.y) * (e.max.z - e.min.z);
    let samples_at_half = span / (vs * 0.5).powi(3);
    let cell = if samples_at_half > 2_000_000.0 { vs } else { vs * 0.5 };
    // padded grid, as surface_nets builds it: two cells on every side
    let samples = |c: f64| ((e.max.x - e.min.x) / c + 5.0) * ((e.max.y - e.min.y) / c + 5.0) * ((e.max.z - e.min.z) / c + 5.0);
    if samples(cell) <= MAX_MESH_SAMPLES { return cell; }
    let mut c = (span / MAX_MESH_SAMPLES).cbrt().max(cell);
    while samples(c) > MAX_MESH_SAMPLES { c *= 1.1; }
    c
}

pub fn surface_nets(shape: &ShapeExpr, cell: f64, vs: f64) -> TriMesh {
    surface_nets_with(shape, cell, vs, Sampling::Auto)
}

/// How the field is sampled. `Auto` is sparse when that is provably
/// identical to dense (`is_lipschitz`), dense otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sampling { Auto, Dense }

pub fn surface_nets_with(shape: &ShapeExpr, cell: f64, vs: f64, mode: Sampling) -> TriMesh {
    let e   = analytic_extents(shape);
    let pad = 2.0 * cell;
    let lo  = Vec3::new(e.min.x - pad, e.min.y - pad, e.min.z - pad);
    let hi  = Vec3::new(e.max.x + pad, e.max.y + pad, e.max.z + pad);

    let nx = ((hi.x - lo.x) / cell).ceil().max(1.0) as usize;
    let ny = ((hi.y - lo.y) / cell).ceil().max(1.0) as usize;
    let nz = ((hi.z - lo.z) / cell).ceil().max(1.0) as usize;
    let (cx, cy, cz) = (nx + 1, ny + 1, nz + 1);

    let corner = |i: usize, j: usize, k: usize| Vec3::new(
        lo.x + i as f64 * cell, lo.y + j as f64 * cell, lo.z + k as f64 * cell,
    );
    let cidx = |i: usize, j: usize, k: usize| (k * cy + j) * cx + i;

    // Sample the field: everywhere (dense), or only where the surface can be
    // (sparse). Both give the same mesh, byte for byte — see `sample_grid`.
    let field = Field::new(shape, vs);
    let (d, _) = sample_grid(&field, lo, cell, [cx, cy, cz], mode);
    let inside = |v: f64| v <= 0.0;

    // One vertex per cell that straddles the surface.
    const EDGES: [((usize, usize, usize), (usize, usize, usize)); 12] = [
        ((0,0,0),(1,0,0)), ((0,1,0),(1,1,0)), ((0,0,1),(1,0,1)), ((0,1,1),(1,1,1)),
        ((0,0,0),(0,1,0)), ((1,0,0),(1,1,0)), ((0,0,1),(0,1,1)), ((1,0,1),(1,1,1)),
        ((0,0,0),(0,0,1)), ((1,0,0),(1,0,1)), ((0,1,0),(0,1,1)), ((1,1,0),(1,1,1)),
    ];

    let mut mesh = TriMesh::default();
    let mut cell_vertex = vec![u32::MAX; nx * ny * nz];
    let vidx = |i: usize, j: usize, k: usize| (k * ny + j) * nx + i;
    let eps = cell * 0.25;

    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                let mut sum = Vec3::ZERO;
                let mut n = 0usize;
                for &((ax, ay, az), (bx, by, bz)) in &EDGES {
                    let da = d[cidx(i + ax, j + ay, k + az)];
                    let db = d[cidx(i + bx, j + by, k + bz)];
                    if inside(da) != inside(db) {
                        let t  = (da / (da - db)).clamp(0.0, 1.0);
                        let pa = corner(i + ax, j + ay, k + az);
                        let pb = corner(i + bx, j + by, k + bz);
                        sum = sum.add(pa.add(pb.sub(pa).scale(t)));
                        n += 1;
                    }
                }
                if n == 0 { continue; }
                let p = sum.scale(1.0 / n as f64);
                let nrm = field.gradient(p, eps);
                cell_vertex[vidx(i, j, k)] = mesh.positions.len() as u32;
                mesh.positions.push([p.x as f32, p.y as f32, p.z as f32]);
                mesh.normals.push([nrm.x as f32, nrm.y as f32, nrm.z as f32]);
            }
        }
    }

    // One quad per surface-crossing edge, joining the four cells around it.
    // Winding is decided by comparing the face normal with the vertex
    // normals, so no orientation bookkeeping can be silently wrong.
    // Winding is decided PER TRIANGLE, after the split. A dual-contouring
    // quad is not planar — its two triangles can have geometric normals
    // that disagree — so reordering the quad as a unit fixes one and can
    // break the other. Each triangle is compared against its own vertex
    // normals and flipped alone.
    let mut emit_quad = |q: [u32; 4]| {
        if q.contains(&u32::MAX) { return; }

        let push_tri = |a: u32, b: u32, c: u32, mesh: &mut TriMesh| {
            let p = |v: u32| {
                let x = mesh.positions[v as usize];
                Vec3::new(x[0] as f64, x[1] as f64, x[2] as f64)
            };
            let nv = |v: u32| {
                let x = mesh.normals[v as usize];
                Vec3::new(x[0] as f64, x[1] as f64, x[2] as f64)
            };
            let face = p(b).sub(p(a)).cross(p(c).sub(p(a)));
            let want = nv(a).add(nv(b)).add(nv(c));
            if face.dot(want) >= 0.0 {
                mesh.indices.extend_from_slice(&[a, b, c]);
            } else {
                mesh.indices.extend_from_slice(&[a, c, b]);
            }
        };

        push_tri(q[0], q[1], q[2], &mut mesh);
        push_tri(q[0], q[2], q[3], &mut mesh);
    };

    for k in 0..cz {
        for j in 0..cy {
            for i in 0..cx {
                let da = d[cidx(i, j, k)];
                // x-edge
                if i + 1 < cx && j >= 1 && k >= 1 && j < ny && k < nz
                    && inside(da) != inside(d[cidx(i + 1, j, k)]) && i < nx
                {
                    emit_quad([
                        cell_vertex[vidx(i, j - 1, k - 1)], cell_vertex[vidx(i, j, k - 1)],
                        cell_vertex[vidx(i, j, k)],         cell_vertex[vidx(i, j - 1, k)],
                    ]);
                }
                // y-edge
                if j + 1 < cy && i >= 1 && k >= 1 && i < nx && k < nz
                    && inside(da) != inside(d[cidx(i, j + 1, k)]) && j < ny
                {
                    emit_quad([
                        cell_vertex[vidx(i - 1, j, k - 1)], cell_vertex[vidx(i, j, k - 1)],
                        cell_vertex[vidx(i, j, k)],         cell_vertex[vidx(i - 1, j, k)],
                    ]);
                }
                // z-edge
                if k + 1 < cz && i >= 1 && j >= 1 && i < nx && j < ny
                    && inside(da) != inside(d[cidx(i, j, k + 1)]) && k < nz
                {
                    emit_quad([
                        cell_vertex[vidx(i - 1, j - 1, k)], cell_vertex[vidx(i, j - 1, k)],
                        cell_vertex[vidx(i, j, k)],         cell_vertex[vidx(i - 1, j, k)],
                    ]);
                }
            }
        }
    }

    mesh
}

// ── Sparse sampling (Form, stage 0) ─────────────────────────────────────
//
// Dense sampling evaluates the field at every corner of the padded box, so a
// thin structure spread over a large box costs the whole box. Two exact
// shortcuts, both chosen so the mesh does not change by a single bit:
//
// 1. Skipping empty space. If the field is 1-Lipschitz (it never changes
//    faster than the point moves), then |d(c)| > R means no point within R of
//    c has a different sign. A block of corners whose every touching cell
//    lies within R of its center can then have no surface cell and no
//    crossing edge, and surface nets reads only signs there. The block's
//    corners are filled with d(c), which has the right sign; their exact
//    values are never read. Shapes that are not 1-Lipschitz (ellipsoid,
//    blob, heightfield) are sampled densely, exactly as before.
//
// 2. Skipping far operands of a union. An operand whose distance is exact
//    (`exact_outside`) is never below the distance to its bounding box; one
//    whose box is farther than the running minimum (plus twice the blend,
//    where the smooth minimum then returns the running value unchanged) cannot
//    change the fold, so it is not evaluated. The fold keeps the operands'
//    order: a smooth minimum is not associative.

/// The distance field of one shape, with union-operand culling when the
/// root is a union of many operands.
pub(crate) struct Field<'a> {
    shape: &'a ShapeExpr,
    vs:    f64,
    cull:  Option<UnionCull<'a>>,
}

struct UnionCull<'a> {
    ops:    &'a [ShapeExpr],
    blend:  f64,
    /// grid cell size; an operand absent from a point's cell is farther than
    /// `g` from it
    g:      f64,
    grid:   HashMap<(i64, i64, i64), Vec<usize>>,
    /// operands whose distance is not exact, or whose box covers too many
    /// cells: evaluated at every point
    always: Vec<usize>,
    /// per operand: its bounding box, when its distance is exact outside it
    boxes:  Vec<Option<(Vec3, Vec3)>>,
}

/// Euclidean distance from `p` to the box `[lo, hi]` (0 inside).
fn box_distance(p: Vec3, (lo, hi): (Vec3, Vec3)) -> f64 {
    let gap = |v: f64, a: f64, b: f64| (a - v).max(v - b).max(0.0);
    Vec3::new(gap(p.x, lo.x, hi.x), gap(p.y, lo.y, hi.y), gap(p.z, lo.z, hi.z)).length()
}

/// Unions with fewer operands are evaluated directly.
const CULL_MIN_OPERANDS: usize = 8;
/// An operand registered in more grid cells than this is evaluated always.
const CULL_MAX_CELLS: i64 = 4096;

impl<'a> Field<'a> {
    pub(crate) fn new(shape: &'a ShapeExpr, vs: f64) -> Self {
        let cull = match shape {
            ShapeExpr::Union { shapes, args } if shapes.len() >= CULL_MIN_OPERANDS => {
                let blend = crate::geometry::arg_f64(args, "blend", 0.0).max(0.0);
                let boxes: Vec<_> = shapes.iter().map(analytic_extents).collect();
                // a cell about the size of a typical operand
                let mut sizes: Vec<f64> = boxes.iter()
                    .map(|e| (e.max.x - e.min.x).max(e.max.y - e.min.y).max(e.max.z - e.min.z)).collect();
                sizes.sort_by(f64::total_cmp);
                let g = sizes[sizes.len() / 2].max(1e-6) + 2.0 * blend;
                let key = |v: f64| (v / g).floor() as i64;
                let mut grid: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
                let mut always = Vec::new();
                let exact_boxes = shapes.iter().zip(&boxes).map(|(op, e)| {
                    let finite = [e.min.x, e.min.y, e.min.z, e.max.x, e.max.y, e.max.z].iter().all(|v| v.is_finite());
                    (exact_outside(op) && finite).then_some((e.min, e.max))
                }).collect();
                for (i, (op, e)) in shapes.iter().zip(&boxes).enumerate() {
                    // one extra cell on each side covers rounding in `key`
                    let (x0, x1) = (key(e.min.x - g) - 1, key(e.max.x + g) + 1);
                    let (y0, y1) = (key(e.min.y - g) - 1, key(e.max.y + g) + 1);
                    let (z0, z1) = (key(e.min.z - g) - 1, key(e.max.z + g) + 1);
                    let cells = (x1 - x0 + 1).saturating_mul(y1 - y0 + 1).saturating_mul(z1 - z0 + 1);
                    let finite = [e.min.x, e.min.y, e.min.z, e.max.x, e.max.y, e.max.z].iter().all(|v| v.is_finite());
                    if !exact_outside(op) || !finite || cells > CULL_MAX_CELLS {
                        always.push(i);
                        continue;
                    }
                    for x in x0..=x1 { for y in y0..=y1 { for z in z0..=z1 {
                        grid.entry((x, y, z)).or_default().push(i);
                    } } }
                }
                Some(UnionCull { ops: shapes, blend, g, grid, always, boxes: exact_boxes })
            }
            _ => None,
        };
        Field { shape, vs, cull }
    }

    pub(crate) fn d(&self, p: Vec3) -> f64 {
        let Some(c) = &self.cull else { return distance(self.shape, p, self.vs) };
        let fold = |a: f64, b: f64| if c.blend > 0.0 { crate::geometry::smooth_min(a, b, c.blend) } else { a.min(b) };
        // Far operands may be skipped once the running value is this low:
        // they are farther than `g`, so their value exceeds it by 2·blend.
        let low = c.g * (1.0 - 1e-9) - 2.0 * c.blend - 1e-9;
        let n = c.ops.len();
        let mut acc = distance(&c.ops[0], p, self.vs);
        let mut j = 1;
        while j < n && acc >= low {
            // An exact operand whose box is farther than acc + 2·blend leaves
            // the fold unchanged: its value is at least the box distance.
            let far = c.boxes[j].is_some_and(|b| box_distance(p, b) * (1.0 - 1e-12) - 1e-12 > acc + 2.0 * c.blend);
            if !far { acc = fold(acc, distance(&c.ops[j], p, self.vs)); }
            j += 1;
        }
        if j == n { return acc; }
        // From here on only the operands near p (and the `always` ones) can
        // change the fold, in their original order.
        let key = |v: f64| (v / c.g).floor() as i64;
        let near: &[usize] = c.grid.get(&(key(p.x), key(p.y), key(p.z))).map_or(&[], Vec::as_slice);
        let (mut a, mut b) = (0usize, 0usize);
        loop {
            let next = match (near.get(a), c.always.get(b)) {
                (Some(&x), Some(&y)) => if x <= y { a += 1; if x == y { b += 1; } x } else { b += 1; y },
                (Some(&x), None) => { a += 1; x }
                (None, Some(&y)) => { b += 1; y }
                (None, None) => break,
            };
            if next < j { continue; }
            acc = fold(acc, distance(&c.ops[next], p, self.vs));
        }
        acc
    }

    /// `geometry::gradient`, through this field: the same differences in the
    /// same order, so the same normals.
    pub(crate) fn gradient(&self, p: Vec3, eps: f64) -> Vec3 {
        let n = Vec3::new(
            self.d(Vec3::new(p.x + eps, p.y, p.z)) - self.d(Vec3::new(p.x - eps, p.y, p.z)),
            self.d(Vec3::new(p.x, p.y + eps, p.z)) - self.d(Vec3::new(p.x, p.y - eps, p.z)),
            self.d(Vec3::new(p.x, p.y, p.z + eps)) - self.d(Vec3::new(p.x, p.y, p.z - eps)),
        );
        n.normalize().unwrap_or(Vec3::Y)
    }
}

/// A distance that never changes faster than the point moves (1-Lipschitz),
/// so a sample bounds the field around it. Exact SDFs are; min, max, the
/// polynomial smooth minimum and isometries keep it; `scale` is divided by its
/// largest stretch and multiplied by its smallest, which keeps it too.
pub(crate) fn is_lipschitz(s: &ShapeExpr) -> bool {
    use ShapeExpr as S;
    match s {
        S::Sphere { .. } | S::Box_ { .. } | S::Cylinder { .. } | S::Cone { .. }
        | S::Capsule { .. } | S::Torus { .. } | S::Tube { .. } | S::Lathe { .. } => true,
        S::Ellipsoid { .. } | S::Blob { .. } | S::Heightfield { .. } => false,
        S::Shell { inner, .. } | S::At { inner, .. } | S::Spin { inner, .. } | S::Mirror { inner, .. } => is_lipschitz(inner),
        S::Extrude { profile, .. } => is_lipschitz(profile),
        S::Scale { inner, args } => {
            let f = crate::geometry::scale_factors(args);
            [f.x, f.y, f.z].iter().all(|v| v.is_finite() && *v != 0.0) && is_lipschitz(inner)
        }
        S::Union { shapes, .. } | S::Intersect { shapes } => shapes.iter().all(is_lipschitz),
        S::Difference { base, cuts, .. } => is_lipschitz(base) && cuts.iter().all(is_lipschitz),
        S::Call { .. } | S::Repeat { .. } => false,
    }
}

/// A distance that is the true Euclidean distance outside the shape, so it is
/// never below the distance to the shape's bounding box.
fn exact_outside(s: &ShapeExpr) -> bool {
    use ShapeExpr as S;
    match s {
        S::Sphere { .. } | S::Box_ { .. } | S::Cylinder { .. } | S::Cone { .. }
        | S::Capsule { .. } | S::Torus { .. } | S::Tube { .. } | S::Lathe { .. } => true,
        S::At { inner, .. } | S::Spin { inner, .. } | S::Mirror { inner, .. } | S::Shell { inner, .. } => exact_outside(inner),
        S::Union { shapes, args } => crate::geometry::arg_f64(args, "blend", 0.0) <= 0.0 && shapes.iter().all(exact_outside),
        _ => false,
    }
}

/// The field at every grid corner — or, sparse, at the corners that can
/// matter, with the rest filled by a value of the right sign. Returns the
/// samples and how many field evaluations they took.
pub(crate) fn sample_grid(field: &Field, lo: Vec3, cell: f64, dims: [usize; 3], mode: Sampling) -> (Vec<f64>, usize) {
    let [cx, cy, cz] = dims;
    let cidx = |i: usize, j: usize, k: usize| (k * cy + j) * cx + i;
    let corner = |i: usize, j: usize, k: usize| Vec3::new(
        lo.x + i as f64 * cell, lo.y + j as f64 * cell, lo.z + k as f64 * cell,
    );
    let mut d = vec![0.0f64; cx * cy * cz];
    if mode == Sampling::Dense || !is_lipschitz(field.shape) {
        for k in 0..cz { for j in 0..cy { for i in 0..cx {
            d[cidx(i, j, k)] = field.d(corner(i, j, k));
        } } }
        return (d, cx * cy * cz);
    }

    let mut count = 0usize;
    // an explicit stack of corner-index boxes [lo, hi)
    let mut stack: Vec<([usize; 3], [usize; 3])> = vec![([0, 0, 0], [cx, cy, cz])];
    while let Some((a, b)) = stack.pop() {
        let n = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        if n.contains(&0) { continue; }
        if n.iter().all(|&m| m <= 2) {
            for k in a[2]..b[2] { for j in a[1]..b[1] { for i in a[0]..b[0] {
                d[cidx(i, j, k)] = field.d(corner(i, j, k));
            } } }
            count += n[0] * n[1] * n[2];
            continue;
        }
        let mid = |t: usize| (a[t] + b[t] - 1) as f64 * 0.5;
        let c = Vec3::new(lo.x + mid(0) * cell, lo.y + mid(1) * cell, lo.z + mid(2) * cell);
        let dc = field.d(c);
        count += 1;
        // every cell touching a corner of the box lies within one cell of it
        let h = |t: usize| (n[t] - 1) as f64 * cell * 0.5 + cell;
        let r = (h(0) * h(0) + h(1) * h(1) + h(2) * h(2)).sqrt();
        if dc.is_finite() && dc.abs() > r + cell {
            for k in a[2]..b[2] { for j in a[1]..b[1] { for i in a[0]..b[0] {
                d[cidx(i, j, k)] = dc;
            } } }
            continue;
        }
        let split = |t: usize| if n[t] > 2 { vec![(a[t], a[t] + n[t] / 2), (a[t] + n[t] / 2, b[t])] } else { vec![(a[t], b[t])] };
        for &(x0, x1) in &split(0) { for &(y0, y1) in &split(1) { for &(z0, z1) in &split(2) {
            stack.push(([x0, y0, z0], [x1, y1, z1]));
        } } }
    }
    (d, count)
}

/// Write every part of a scene as a smooth mesh: `<path>.obj` with an
/// `.mtl` sidecar, one `o` group per part, materials per color.
pub fn export_scene_obj(scene: &Scene, cell: Option<f64>, path: &str) -> anyhow::Result<()> {
    let scene = &*scene.at_rest();   // revealed parts are not there at rest
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    // one material per colour, and per surface for parts whose material sets one
    let mat_key = |part: &crate::scene::Part| match (part.roughness, part.metalness) {
        (None, None) => part.color.clone(),
        (r, m) => format!("{}|{}|{}", part.color,
            r.unwrap_or(crate::gltf::DEFAULT_ROUGHNESS), m.unwrap_or(0.0)),
    };
    let mut color_to_mat: HashMap<String, String> = HashMap::new();
    let mut surface: HashMap<String, (f64, f64)> = HashMap::new();
    for layer in &scene.layers {
        for part in &layer.parts {
            let n = color_to_mat.len();
            let name = color_to_mat.entry(mat_key(part)).or_insert_with(|| format!("mat_{n}")).clone();
            if part.roughness.is_some() || part.metalness.is_some() {
                surface.insert(name, (part.roughness.unwrap_or(crate::gltf::DEFAULT_ROUGHNESS), part.metalness.unwrap_or(0.0)));
            }
        }
    }

    let mtl_path = format!("{path}.mtl");
    crate::export::write_mtl(&color_to_mat, &surface, &mtl_path)?;

    let obj_path = format!("{path}.obj");
    let mut f = BufWriter::new(File::create(&obj_path)?);
    let mtl_name = Path::new(&mtl_path).file_name().and_then(|n| n.to_str()).unwrap_or("moxi.mtl");
    writeln!(f, "# Moxi surface-nets export")?;
    writeln!(f, "mtllib {mtl_name}")?;

    let mut offset: usize = 1;
    let mut total_tris = 0usize;

    for layer in &scene.layers {
        for part in &layer.parts {
            let expr  = part.shape.to_expr();
            let frame: Frame = part.frame.to_frame();
            let g     = part.grain(layer.voxel_size);
            let c     = cell.unwrap_or_else(|| auto_cell(&expr, g));
            let mesh  = surface_nets(&expr, c, g);
            if mesh.indices.is_empty() { continue; }

            writeln!(f, "o {}", part.name)?;
            writeln!(f, "usemtl {}", color_to_mat[&mat_key(part)])?;
            for p in &mesh.positions {
                let w = frame.apply_point(Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
                writeln!(f, "v {:.4} {:.4} {:.4}", w.x, w.y, w.z)?;
            }
            for n in &mesh.normals {
                let w = frame.apply_dir(Vec3::new(n[0] as f64, n[1] as f64, n[2] as f64));
                writeln!(f, "vn {:.4} {:.4} {:.4}", w.x, w.y, w.z)?;
            }
            for tri in mesh.indices.chunks(3) {
                let (a, b, c) = (tri[0] as usize + offset, tri[1] as usize + offset, tri[2] as usize + offset);
                writeln!(f, "f {a}//{a} {b}//{b} {c}//{c}")?;
            }
            offset += mesh.positions.len();
            total_tris += mesh.indices.len() / 3;
        }
    }

    println!("  exported → {obj_path}  ({total_tris} triangles)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, NamedArg};

    fn na(k: &str, v: f64) -> NamedArg { NamedArg { key: k.into(), value: Expr::Float(v) } }
    fn sphere(r: f64) -> ShapeExpr { ShapeExpr::Sphere { args: vec![na("radius", r)] } }

    /// A part far larger than its scene's voxel size (a 260 m ground in a
    /// scene resolved at 5 cm) meshes within MAX_MESH_SAMPLES; a part the
    /// old rule could already mesh keeps its old cell exactly.
    #[test]
    fn auto_cell_is_capped_and_unchanged_below_the_cap() {
        let ground = ShapeExpr::Box_ { args: vec![na("width", 260.0), na("height", 2.0), na("depth", 260.0)] };
        let c = auto_cell(&ground, 0.05);
        let n = (260.0 / c + 5.0) * (2.0 / c + 5.0) * (260.0 / c + 5.0);
        assert!(n <= MAX_MESH_SAMPLES, "{n} samples at cell {c}");
        assert!(c > 0.05);
        assert_eq!(auto_cell(&sphere(10.0), 1.0), 0.5);
        assert_eq!(auto_cell(&sphere(100.0), 1.0), 1.0);
    }

    /// Every vertex of a meshed sphere lies within a cell of the surface,
    /// and the mesh is closed enough to have a sensible triangle count.
    #[test]
    fn sphere_mesh_vertices_lie_on_the_surface() {
        let s = sphere(3.0);
        let m = surface_nets(&s, 0.5, 1.0);
        assert!(m.indices.len() / 3 > 200, "too few triangles: {}", m.indices.len() / 3);
        for p in &m.positions {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - 3.0).abs() < 0.5, "vertex at radius {r}");
        }
    }

    /// Normals point outward: for a sphere, along the position.
    #[test]
    fn sphere_normals_point_outward() {
        let m = surface_nets(&sphere(3.0), 0.5, 1.0);
        for (p, n) in m.positions.iter().zip(&m.normals) {
            let dot = p[0] * n[0] + p[1] * n[1] + p[2] * n[2];
            assert!(dot > 0.0, "inward normal at {p:?}");
        }
    }

    /// Winding agrees with the normals, so back-face culling shows the
    /// outside: every triangle's geometric normal has positive dot with
    /// its vertices' normals.
    #[test]
    fn triangles_wind_with_their_normals() {
        let m = surface_nets(&sphere(2.0), 0.5, 1.0);
        for t in m.indices.chunks(3) {
            let p = |i: u32| { let a = m.positions[i as usize]; Vec3::new(a[0] as f64, a[1] as f64, a[2] as f64) };
            let n = |i: u32| { let a = m.normals[i as usize];   Vec3::new(a[0] as f64, a[1] as f64, a[2] as f64) };
            let face = p(t[1]).sub(p(t[0])).cross(p(t[2]).sub(p(t[0])));
            let want = n(t[0]).add(n(t[1])).add(n(t[2]));
            assert!(face.dot(want) >= 0.0, "back-facing triangle");
        }
    }

    /// A blended union meshes as one connected surface across the join.
    #[test]
    fn blended_union_meshes_the_fillet() {
        let pair = ShapeExpr::Union {
            shapes: vec![
                sphere(2.0),
                ShapeExpr::At { inner: Box::new(sphere(2.0)), args: vec![na("x", 5.0)] },
            ],
            args: vec![na("blend", 2.0)],
        };
        let m = surface_nets(&pair, 0.5, 1.0);
        let bridge = m.positions.iter().filter(|p| (p[0] - 2.5).abs() < 0.3).count();
        assert!(bridge > 0, "no vertices in the fillet region between the lobes");
    }

    #[test]
    fn obj_materials_carry_ns_only_when_the_material_has_a_surface() {
        let src = r##"
material M { color = "#888888" }
material W { color = "#888888", roughness = 0.2 }
thing T {
    part A { shape = sphere(radius=3), material = M }
    part B { shape = sphere(radius=3), material = W }
    relation { B.west on A.east }
}
print T
"##;
        let dir = std::env::temp_dir().join(format!("moxi-mtl-{}", std::process::id()));
        let path = dir.join("t");
        export_scene_obj(&crate::pipeline::compile_to_scene(src).unwrap(), Some(1.0), path.to_str().unwrap()).unwrap();
        let mtl = std::fs::read_to_string(dir.join("t.mtl")).unwrap();
        assert_eq!(mtl.matches("newmtl").count(), 2, "same colour, two surfaces: {mtl}");
        assert_eq!(mtl.matches("illum 1").count(), 1, "{mtl}");
        let (ns, _) = crate::export::highlight(0.2);
        assert!(mtl.contains(&format!("Ns {ns:.1}")) && mtl.contains("illum 2"), "{mtl}");
        let _ = std::fs::remove_dir_all(dir);
    }

    // ── Form, stage 0: sparse sampling is exact ────────────────────────────

    fn at(s: ShapeExpr, x: f64, y: f64, z: f64) -> ShapeExpr {
        ShapeExpr::At { inner: Box::new(s), args: vec![na("x", x), na("y", y), na("z", z)] }
    }
    fn capsule(h: f64, r: f64) -> ShapeExpr { ShapeExpr::Capsule { args: vec![na("height", h), na("radius", r)] } }
    fn spin(s: ShapeExpr, deg: f64) -> ShapeExpr {
        ShapeExpr::Spin { inner: Box::new(s), args: vec![
            NamedArg { key: "axis".into(), value: Expr::Ident(crate::ast::Ident { name: "z".into(), span: crate::error::Span::new(0, 0) }) },
            na("degrees", deg)] }
    }
    /// `n` capsules scattered (a fixed LCG) over a cube of side `side`.
    fn capsules(n: usize, side: f64, blend: f64) -> ShapeExpr {
        let mut seed = 12345u64;
        let mut r = || { seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (seed >> 11) as f64 / (1u64 << 53) as f64 };
        let shapes = (0..n).map(|_| {
            let (x, y, z, a) = (r() * side - side / 2.0, r() * side - side / 2.0, r() * side - side / 2.0, r() * 180.0);
            at(spin(capsule(3.0 + 6.0 * r(), 0.4 + 0.6 * r()), a), x, y, z)
        }).collect();
        ShapeExpr::Union { shapes, args: if blend > 0.0 { vec![na("blend", blend)] } else { vec![] } }
    }
    fn same(a: &TriMesh, b: &TriMesh) -> bool {
        let bits = |v: &[[f32; 3]]| v.iter().flat_map(|x| x.iter().map(|f| f.to_bits())).collect::<Vec<_>>();
        bits(&a.positions) == bits(&b.positions) && bits(&a.normals) == bits(&b.normals) && a.indices == b.indices
    }

    #[test]
    fn sparse_meshes_are_bit_identical_to_dense() {
        let cut = ShapeExpr::Difference {
            base: Box::new(ShapeExpr::Box_ { args: vec![na("width", 20.0), na("height", 8.0), na("depth", 14.0), na("round", 1.5)] }),
            cuts: vec![at(ShapeExpr::Cylinder { args: vec![na("height", 12.0), na("radius", 2.5)] }, 3.0, -2.0, 0.0), at(sphere(4.0), -6.0, 4.0, 3.0)],
            blend: 0.0,
        };
        let cases: Vec<(&str, ShapeExpr, f64)> = vec![
            ("sphere", sphere(5.0), 0.5),
            ("plain union of capsules", capsules(40, 40.0, 0.0), 0.5),
            ("blended union of capsules", capsules(30, 30.0, 1.2), 0.5),
            ("difference", cut.clone(), 0.4),
            ("intersect", ShapeExpr::Intersect { shapes: vec![sphere(6.0), at(ShapeExpr::Torus { args: vec![na("major_radius", 5.0), na("minor_radius", 2.0)] }, 0.0, 1.0, 0.0)] }, 0.3),
            ("scale", ShapeExpr::Scale { inner: Box::new(cut.clone()), args: vec![na("x", 2.0), na("y", 0.5), na("z", 1.3)] }, 0.5),
            ("shell and cone", ShapeExpr::Union { shapes: vec![
                ShapeExpr::Shell { inner: Box::new(sphere(6.0)), args: vec![na("inner_offset", 0.8)] },
                at(ShapeExpr::Cone { args: vec![na("height", 9.0), na("radius", 3.0)] }, 8.0, 0.0, 0.0)], args: vec![] }, 0.3),
            ("extrude", ShapeExpr::Extrude { profile: Box::new(ShapeExpr::Box_ { args: vec![na("width", 6.0), na("height", 0.0), na("depth", 2.0)] }), args: vec![na("height", 10.0)] }, 0.4),
            ("tube", ShapeExpr::Tube { args: vec![
                NamedArg { key: "path".into(), value: Expr::List([[0.0, 0.0, 0.0], [0.0, 8.0, 2.0], [3.0, 14.0, 6.0], [-2.0, 18.0, 4.0]].iter()
                    .map(|p| Expr::List(p.iter().map(|v| Expr::Float(*v)).collect())).collect()) },
                NamedArg { key: "radius".into(), value: Expr::List(vec![Expr::Float(1.6), Expr::Float(0.9), Expr::Float(0.4)]) }] }, 0.25),
            ("lathe", ShapeExpr::Lathe { args: vec![
                NamedArg { key: "profile".into(), value: Expr::List([[3.0, 0.0], [4.2, 2.5], [3.1, 6.0], [2.0, 8.0], [2.4, 9.0]].iter()
                    .map(|p| Expr::List(p.iter().map(|v| Expr::Float(*v)).collect())).collect()) }] }, 0.25),
            // not 1-Lipschitz: sampled densely, so identical by construction
            ("blob", ShapeExpr::Blob { args: vec![na("radius", 6.0), na("roughness", 0.3)] }, 0.5),
            ("ellipsoid in a union", ShapeExpr::Union { shapes: vec![ShapeExpr::Ellipsoid { args: vec![na("rx", 8.0), na("ry", 3.0), na("rz", 5.0)] }, sphere(2.0)], args: vec![] }, 0.4),
        ];
        for (name, shape, cell) in &cases {
            let sparse = surface_nets_with(shape, *cell, 1.0, Sampling::Auto);
            let dense  = surface_nets_with(shape, *cell, 1.0, Sampling::Dense);
            assert!(!dense.indices.is_empty(), "{name}: the case must mesh something");
            assert!(same(&sparse, &dense), "{name}: sparse and dense differ");
        }
    }

    #[test]
    fn the_culled_union_field_equals_the_plain_one_everywhere() {
        for blend in [0.0, 0.8, 2.5] {
            let u = capsules(60, 50.0, blend);
            let f = Field::new(&u, 1.0);
            assert!(f.cull.is_some());
            let mut seed = 7u64;
            let mut r = || { seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1); (seed >> 11) as f64 / (1u64 << 53) as f64 };
            for _ in 0..20_000 {
                let p = Vec3::new(r() * 70.0 - 35.0, r() * 70.0 - 35.0, r() * 70.0 - 35.0);
                assert_eq!(f.d(p).to_bits(), distance(&u, p, 1.0).to_bits(), "blend {blend} at {p:?}");
            }
        }
    }

    /// Stage 0's acceptance test: 500 capsules over a 200-unit box at cell
    /// 0.5 take under 5% of the dense sample count.
    #[test]
    fn a_sparse_network_takes_a_small_fraction_of_the_dense_samples() {
        let u = capsules(500, 200.0, 0.0);
        let cell = 0.5;
        let e = analytic_extents(&u);
        let pad = 2.0 * cell;
        let lo = Vec3::new(e.min.x - pad, e.min.y - pad, e.min.z - pad);
        let n = |a: f64, b: f64| (((b - a) + 2.0 * pad) / cell).ceil() as usize + 1;
        let dims = [n(e.min.x, e.max.x), n(e.min.y, e.max.y), n(e.min.z, e.max.z)];
        let dense = dims[0] * dims[1] * dims[2];
        let (_, sparse) = sample_grid(&Field::new(&u, 1.0), lo, cell, dims, Sampling::Auto);
        let share = sparse as f64 / dense as f64;
        assert!(share < 0.05, "{sparse} of {dense} samples ({:.1}%)", share * 100.0);
    }
}