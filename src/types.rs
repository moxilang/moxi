use std::collections::HashMap;
use bevy::prelude::{Resource, Vec3};

#[derive(Debug, Clone)]
pub enum Value {
    Atom {
        name: String,
        props: std::collections::HashMap<String, String>,
    },

    ModelDef { params: Vec<String>, body: Vec<crate::moxi::parser::AstNode> },
    Instance(Instance),
    Array(Vec<Value>),
    String(String),
    Number(i32),
    Ident(String),
    Map(std::collections::HashMap<String, Value>),
}



impl Default for Transform3D {
    fn default() -> Self {
        Transform3D { dx: 0, dy: 0, dz: 0, rotations: vec![] }
    }
}


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

/// A named model (collection of voxels, before transforms).
#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub voxels: Vec<Voxel>,
}

/// Simple transform for an instance of a model
#[derive(Debug, Clone)]
pub struct Transform3D {
    pub dx: i32,
    pub dy: i32,
    pub dz: i32,
    pub rotations: Vec<(String, i32)>, // e.g. [("x", 1), ("z", 3)]
}

/// An instance of a model in the scene
#[derive(Debug, Clone)]
pub struct Instance {
    pub model: Model,
    pub transform: Transform3D,
}

/// Scene graph: collection of instances
#[derive(Debug, Clone)]
pub struct SceneGraph {
    pub instances: Vec<Instance>,
}

impl SceneGraph {
    /// Flatten into a raw voxel scene (for export/viewing)
    pub fn flatten(&self) -> VoxelScene {
        let mut voxels = Vec::new();
        for inst in &self.instances {
            let mut transformed = inst.model.voxels.clone();

            for v in &mut transformed {
                let mut x = v.x;
                let mut y = v.y;
                let mut z = v.z;

                // apply rotations in sequence
                for (axis, turns) in &inst.transform.rotations {
                    for _ in 0..((turns % 4 + 4) % 4) {
                        match axis.as_str() {
                            "x" => { let ny = -z; let nz = y; y = ny; z = nz; }
                            "y" => { let nx = z; let nz = -x; x = nx; z = nz; }
                            "z" => { let nx = -y; let ny = x; x = nx; y = ny; }
                            _ => {}
                        }
                    }
                }

                v.x = x + inst.transform.dx;
                v.y = y + inst.transform.dy;
                v.z = z + inst.transform.dz;
            }

            voxels.extend(transformed);
        }
        VoxelScene { voxels }
    }
}
