// src/types.rs
// Bridge between VoxelGrid (geometry backend) and the viewer/exporter.

// Vec3 only needed when viewer feature is on
#[cfg(feature = "viewer")]
use bevy::prelude::{Resource, Vec3};

/// A single voxel: integer position + resolved hex color.
#[derive(Debug, Clone)]
pub struct Voxel {
    pub x:     i32,
    pub y:     i32,
    pub z:     i32,
    pub color: String,
}

/// Flat voxel list handed to the viewer and exporter.
#[cfg_attr(feature = "viewer", derive(Resource))]
#[derive(Debug, Clone)]
pub struct VoxelScene {
    pub voxels: Vec<Voxel>,
}

impl VoxelScene {
    pub fn new(voxels: Vec<Voxel>) -> Self { Self { voxels } }

    #[cfg(feature = "viewer")]
    pub fn bounds(&self) -> (Vec3, Vec3) {
        let mut min = Vec3::splat(f32::MAX);
        let mut max = Vec3::splat(f32::MIN);
        for v in &self.voxels {
            let pos = Vec3::new(v.x as f32, v.y as f32, v.z as f32);
            min = min.min(pos);
            max = max.max(pos);
        }
        (min, max)
    }

    #[cfg(feature = "viewer")]
    pub fn center(&self) -> Vec3 {
        let (min, max) = self.bounds();
        (min + max) * 0.5
    }

    #[cfg(feature = "viewer")]
    pub fn size(&self) -> Vec3 {
        let (min, max) = self.bounds();
        max - min
    }

    #[cfg(feature = "viewer")]
    pub fn max_dim(&self) -> f32 {
        self.size().max_element()
    }
}

/// Convert a VoxelGrid into a VoxelScene for viewing and exporting.
pub fn grid_to_scene(
    grid:   &crate::voxel::VoxelGrid,
    atoms:  &[crate::resolver::ResolvedAtom],
    offset: (i32, i32, i32),
) -> VoxelScene {
    use crate::colors::resolve_color;
    let voxels = grid.iter_filled().map(|(x, y, z, atom_id)| {
        let color = atoms
            .get(atom_id.saturating_sub(1) as usize)
            .map(|a| resolve_color(&a.color))
            .unwrap_or_else(|| "#ff00ff".to_string());
        Voxel { x: x as i32 + offset.0, y: y as i32 + offset.1, z: z as i32 + offset.2, color }
    }).collect();
    VoxelScene { voxels }
}