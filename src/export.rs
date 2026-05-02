// src/export.rs
//
// Exports a VoxelScene to a Wavefront OBJ file.
// Each voxel becomes a unit cube (8 vertices, 6 quad faces).
// Colors are written as an MTL sidecar file so Blender can read them.

use crate::types::{Voxel, VoxelScene};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Write `scene` to `<path>.obj` and `<path>.mtl`.
pub fn export_to_obj(scene: &VoxelScene, path: &str) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Collect unique colors → material names
    let mut color_to_mat: HashMap<String, String> = HashMap::new();
    for voxel in &scene.voxels {
        let entry_count = color_to_mat.len();
        color_to_mat
            .entry(voxel.color.clone())
            .or_insert_with(|| format!("mat_{entry_count}"));
    }

    // Write MTL sidecar
    let mtl_path = format!("{path}.mtl");
    write_mtl(&color_to_mat, &mtl_path)?;

    // Write OBJ
    let obj_path = format!("{path}.obj");
    let mut f = BufWriter::new(File::create(&obj_path)?);

    let mtl_filename = Path::new(&mtl_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("moxi.mtl");

    writeln!(f, "# Moxi voxel export")?;
    writeln!(f, "mtllib {mtl_filename}")?;
    writeln!(f)?;

    let mut vertex_offset: usize = 1; // OBJ indices are 1-based

    for voxel in &scene.voxels {
        let mat = color_to_mat.get(&voxel.color).map(|s| s.as_str()).unwrap_or("mat_0");
        writeln!(f, "usemtl {mat}")?;

        let (verts, faces) = cube_geometry(voxel);

        for (vx, vy, vz) in &verts {
            writeln!(f, "v {vx:.3} {vy:.3} {vz:.3}")?;
        }

        for face in &faces {
            writeln!(
                f, "f {} {} {} {}",
                vertex_offset + face[0],
                vertex_offset + face[1],
                vertex_offset + face[2],
                vertex_offset + face[3],
            )?;
        }

        vertex_offset += 8;
    }

    println!("  exported → {obj_path}  ({} voxels)", scene.voxels.len());
    Ok(())
}

fn write_mtl(color_to_mat: &HashMap<String, String>, path: &str) -> anyhow::Result<()> {
    let mut f = BufWriter::new(File::create(path)?);
    writeln!(f, "# Moxi material library")?;

    for (hex, mat_name) in color_to_mat {
        let (r, g, b) = hex_to_rgb_f32(hex);
        writeln!(f)?;
        writeln!(f, "newmtl {mat_name}")?;
        writeln!(f, "Kd {r:.4} {g:.4} {b:.4}")?;   // diffuse color
        writeln!(f, "Ka 0.1 0.1 0.1")?;             // ambient
        writeln!(f, "Ks 0.0 0.0 0.0")?;             // no specular
        writeln!(f, "illum 1")?;
    }

    Ok(())
}

/// Returns the 8 corner vertices and 6 quad faces for a unit cube at voxel position.
fn cube_geometry(v: &Voxel) -> (Vec<(f32, f32, f32)>, Vec<[usize; 4]>) {
    let (x, y, z) = (v.x as f32, v.y as f32, v.z as f32);

    let verts = vec![
        (x,       y,       z      ),  // 0 — bottom-front-left
        (x + 1.0, y,       z      ),  // 1 — bottom-front-right
        (x + 1.0, y + 1.0, z      ),  // 2 — top-front-right
        (x,       y + 1.0, z      ),  // 3 — top-front-left
        (x,       y,       z + 1.0),  // 4 — bottom-back-left
        (x + 1.0, y,       z + 1.0),  // 5 — bottom-back-right
        (x + 1.0, y + 1.0, z + 1.0),  // 6 — top-back-right
        (x,       y + 1.0, z + 1.0),  // 7 — top-back-left
    ];

    let faces = vec![
        [0, 1, 2, 3],  // front  (z-)
        [5, 4, 7, 6],  // back   (z+)
        [4, 0, 3, 7],  // left   (x-)
        [1, 5, 6, 2],  // right  (x+)
        [4, 5, 1, 0],  // bottom (y-)
        [3, 2, 6, 7],  // top    (y+)
    ];

    (verts, faces)
}

fn hex_to_rgb_f32(hex: &str) -> (f32, f32, f32) {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return (1.0, 0.0, 1.0); // hot pink for bad input
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255) as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255) as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255) as f32 / 255.0;
    (r, g, b)
}