use std::collections::HashMap;
use bevy::prelude::{Resource, Vec3};

use crate::geom::{Axis, rotate_point_90};

#[derive(Debug, Clone)]
pub enum Value {
    Atom {
        name: String,
        props: HashMap<String, String>,
    },

    ModelDef {
        params: Vec<String>,
        body: Vec<crate::moxi::parser::AstNode>,
    },

    Instance(Instance),
    Array(Vec<Value>),
    String(String),
    Number(i32),
    Map(HashMap<String, Value>),
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

    pub fn center(&self) -> Vec3 {
        let (min, max) = self.bounds();
        (min + max) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        let (min, max) = self.bounds();
        max - min
    }

    pub fn max_dim(&self) -> f32 {
        self.size().max_element()
    }

}

/// A named model (collection of voxels, before transforms).
#[derive(Debug, Clone)]
pub struct Model {
    pub name: String,
    pub voxels: Vec<Voxel>,
}

/// Simple transform for an instance of a model.
#[derive(Debug, Clone)]
pub struct Transform3D {
    pub dx: i32,
    pub dy: i32,
    pub dz: i32,
    pub rotations: Vec<(Axis, i32)>, // quarter turns
}

impl Default for Transform3D {
    fn default() -> Self {
        Transform3D {
            dx: 0,
            dy: 0,
            dz: 0,
            rotations: Vec::new(),
        }
    }
}

impl Transform3D {
    pub fn transform_voxel_local_to_world(&self, v: &mut Voxel) {
        let (mut x, mut y, mut z) = (v.x, v.y, v.z);

        for (axis, turns) in &self.rotations {
            (x, y, z) = rotate_point_90(x, y, z, *axis, *turns);
        }

        v.x = x + self.dx;
        v.y = y + self.dy;
        v.z = z + self.dz;
    }
}


/// An instance of a model in the scene.
#[derive(Debug, Clone)]
pub struct Instance {
    pub model: Model,
    pub transform: Transform3D,
}


/// Scene graph: collection of instances.
#[derive(Debug, Clone)]
pub struct SceneGraph {
    pub instances: Vec<Instance>,
}

impl SceneGraph {
    pub fn resolve_voxels(&self) -> VoxelScene {
        let mut voxels = Vec::new();

        for inst in &self.instances {
            let mut transformed = inst.model.voxels.clone();

            for v in &mut transformed {
                inst.transform.transform_voxel_local_to_world(v);
            }

            voxels.extend(transformed);
        }

        VoxelScene { voxels }
    }
}
