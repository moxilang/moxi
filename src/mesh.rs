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
use crate::geometry::{distance, gradient};
use crate::scene::Scene;

#[derive(Debug, Clone, Default)]
pub struct TriMesh {
    pub positions: Vec<[f32; 3]>,
    pub normals:   Vec<[f32; 3]>,
    pub indices:   Vec<u32>,
}

/// Choose a cell size: half the voxel size, unless that would mean more
/// than ~2M samples for this shape, in which case the voxel size itself.
pub fn auto_cell(shape: &ShapeExpr, vs: f64) -> f64 {
    let e = analytic_extents(shape);
    let span = (e.max.x - e.min.x) * (e.max.y - e.min.y) * (e.max.z - e.min.z);
    let samples_at_half = span / (vs * 0.5).powi(3);
    if samples_at_half > 2_000_000.0 { vs } else { vs * 0.5 }
}

pub fn surface_nets(shape: &ShapeExpr, cell: f64, vs: f64) -> TriMesh {
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

    // Sample the field at every corner.
    let mut d = vec![0.0f64; cx * cy * cz];
    for k in 0..cz {
        for j in 0..cy {
            for i in 0..cx {
                d[cidx(i, j, k)] = distance(shape, corner(i, j, k), vs);
            }
        }
    }
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
                let nrm = gradient(shape, p, vs, eps);
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

/// Write every part of a scene as a smooth mesh: `<path>.obj` with an
/// `.mtl` sidecar, one `o` group per part, materials per color.
pub fn export_scene_obj(scene: &Scene, cell: Option<f64>, path: &str) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut color_to_mat: HashMap<String, String> = HashMap::new();
    for layer in &scene.layers {
        for part in &layer.parts {
            let n = color_to_mat.len();
            color_to_mat.entry(part.color.clone()).or_insert_with(|| format!("mat_{n}"));
        }
    }

    let mtl_path = format!("{path}.mtl");
    crate::export::write_mtl(&color_to_mat, &mtl_path)?;

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
            let c     = cell.unwrap_or_else(|| auto_cell(&expr, layer.voxel_size));
            let mesh  = surface_nets(&expr, c, layer.voxel_size);
            if mesh.indices.is_empty() { continue; }

            writeln!(f, "o {}", part.name)?;
            writeln!(f, "usemtl {}", color_to_mat[&part.color])?;
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
}