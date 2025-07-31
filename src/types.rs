use std::collections::HashMap;

/// Represents a single voxel with a position and a color.
#[derive(Debug, Clone)]
pub struct Voxel {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub color: String,
}

/// Holds the entire voxel scene.
use bevy::prelude::Resource;

#[derive(Debug, Clone, Resource)]
pub struct VoxelScene {
    pub voxels: Vec<Voxel>,
}


/// Map symbol to color name or hex
pub type ColorMap = HashMap<String, String>;
