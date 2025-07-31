use crate::types::*;
use std::fs::File;
use std::io::{BufWriter, Write};

use std::path::Path;

pub fn export_to_obj(scene: &VoxelScene, path: &str) -> anyhow::Result<()> {
    // Create parent directory if needed
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = BufWriter::new(File::create(path)?);
    let mut vertex_offset = 1;

    for voxel in &scene.voxels {
        let x = voxel.x as f32;
        let y = voxel.y as f32;
        let z = voxel.z as f32;

        let cube = generate_cube_vertices(x, y, z);

        for (vx, vy, vz) in &cube.vertices {
            writeln!(file, "v {} {} {}", vx, vy, vz)?;
        }

        for face in &cube.faces {
            writeln!(
                file,
                "f {} {} {} {}",
                vertex_offset + face[0],
                vertex_offset + face[1],
                vertex_offset + face[2],
                vertex_offset + face[3],
            )?;
        }

        vertex_offset += 8;
    }

    Ok(())
}

struct Cube {
    vertices: Vec<(f32, f32, f32)>,
    faces: Vec<[usize; 4]>,
}

fn generate_cube_vertices(x: f32, y: f32, z: f32) -> Cube {
    let vertices = vec![
        (x, y, z),
        (x + 1.0, y, z),
        (x + 1.0, y + 1.0, z),
        (x, y + 1.0, z),
        (x, y, z + 1.0),
        (x + 1.0, y, z + 1.0),
        (x + 1.0, y + 1.0, z + 1.0),
        (x, y + 1.0, z + 1.0),
    ];

    let faces = vec![
        [0, 1, 2, 3], // bottom
        [4, 5, 6, 7], // top
        [0, 1, 5, 4], // front
        [2, 3, 7, 6], // back
        [1, 2, 6, 5], // right
        [0, 3, 7, 4], // left
    ];

    Cube { vertices, faces }
}
