use std::collections::HashMap;

/// Represents a single voxel with a position and a color.
#[derive(Debug)]
pub struct Voxel {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub color: String,
}

/// Holds the entire voxel scene.
#[derive(Debug)]
pub struct VoxelScene {
    pub voxels: Vec<Voxel>,
}

/// Map symbol to color name or hex
pub type ColorMap = HashMap<String, String>;
