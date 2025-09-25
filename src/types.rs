use std::collections::HashMap;
use bevy::prelude::{Resource, Vec3};

/// Represents a single voxel with a position and a color.
#[derive(Debug, Clone)]
pub struct Voxel {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub color: String,
}

/// Holds the entire voxel scene.
#[derive(Debug, Clone, Resource)]
pub struct VoxelScene {
    pub voxels: Vec<Voxel>,
}

impl VoxelScene {
    /// Returns (min, max) corners of the scene’s bounding box
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

    /// Center point of the scene
    pub fn center(&self) -> Vec3 {
        let (min, max) = self.bounds();
        (min + max) * 0.5
    }

    /// Size (width, height, depth) of the scene
    pub fn size(&self) -> Vec3 {
        let (min, max) = self.bounds();
        max - min
    }

    /// Largest dimension (useful for camera radius/FOV)
    pub fn max_dim(&self) -> f32 {
        self.size().max_element()
    }

    /// Shift all voxels so that the minimum x,y,z becomes 0
    pub fn normalize(&mut self) {
        if self.voxels.is_empty() { return; }

        let min_x = self.voxels.iter().map(|v| v.x).min().unwrap();
        let min_y = self.voxels.iter().map(|v| v.y).min().unwrap();
        let min_z = self.voxels.iter().map(|v| v.z).min().unwrap();

        if min_x < 0 || min_y < 0 || min_z < 0 {
            for v in &mut self.voxels {
                v.x -= min_x;
                v.y -= min_y;
                v.z -= min_z;
            }
        }
    }
}

/// Map symbol to color name or hex
pub type ColorMap = HashMap<String, String>;
