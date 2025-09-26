use crate::types::{Voxel, VoxelScene};

/// Translate a voxel scene
pub fn translate(scene: &VoxelScene, dx: i32, dy: i32, dz: i32) -> VoxelScene {
    let voxels = scene.voxels.iter().map(|v| Voxel {
        x: v.x + dx,
        y: v.y + dy,
        z: v.z + dz,
        color: v.color.clone(),
    }).collect();
    VoxelScene { voxels }
}

/// Rotate a voxel scene 90° multiples around an axis
pub fn rotate(scene: &VoxelScene, axis: &str, turns: i32) -> VoxelScene {
    let mut voxels = Vec::new();
    let turns = ((turns % 4) + 4) % 4;

    for v in &scene.voxels {
        let (mut x, mut y, mut z) = (v.x, v.y, v.z);
        for _ in 0..turns {
            match axis {
                "x" => { let ny = -z; let nz = y; y = ny; z = nz; }
                "y" => { let nx = z; let nz = -x; x = nx; z = nz; }
                "z" => { let nx = -y; let ny = x; x = nx; y = ny; }
                _ => {}
            }
        }
        voxels.push(Voxel { x, y, z, color: v.color.clone() });
    }

    let mut rotated = VoxelScene { voxels };
    rotated.normalize();
    rotated
}

/// Merge multiple voxel scenes
pub fn merge(scenes: Vec<VoxelScene>) -> VoxelScene {
    let mut voxels = Vec::new();
    for s in scenes {
        voxels.extend(s.voxels);
    }
    VoxelScene { voxels }
}
