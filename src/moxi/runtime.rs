use crate::types::{Voxel, VoxelScene, Model, SceneGraph, Instance, Transform3D};
use crate::colors::default_colors;
use super::parser::AstNode;
use super::commands::{do_print, do_clone, do_translate, do_rotate, do_merge};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use unicode_segmentation::UnicodeSegmentation;


/// Container for intermediate AST info
struct CollectedAst {
    explicit_colors: HashMap<String, String>,
    layers: Vec<(usize, Vec<String>)>,
    commands: Vec<(String, Vec<String>)>,
}

/// Public entrypoint
pub fn build_scene(ast: Vec<AstNode>) -> SceneGraph {
    let collected = collect_ast(&ast);
    let base_model = expand_layers(&collected);

    let mut scene = SceneGraph {
        instances: vec![Instance {
            model: base_model,
            transform: Transform3D { dx:0, dy:0, dz:0, rotations: vec![] }
        }]
    };

    apply_commands(&mut scene, &collected.commands);
    scene
}

/// Pass 1: collect colors, layers, and commands
fn collect_ast(ast: &[AstNode]) -> CollectedAst {
    let mut explicit_colors = HashMap::new();
    let mut layers = Vec::new();
    let mut commands = Vec::new();

    for node in ast {
        match node {
            AstNode::ColorDecl { symbol, color } => {
                explicit_colors.insert(symbol.clone(), color.clone());
            }
            AstNode::LayerDecl { z, rows } => {
                layers.push((*z, rows.clone()));
            }
            AstNode::Command { name, args } => {
                commands.push((name.clone(), args.clone()));
            }
            _ => {}
        }
    }

    CollectedAst { explicit_colors, layers, commands }
}

/// Pass 2: expand layers → voxels using explicit + builtins
fn expand_layers(collected: &CollectedAst) -> Model {
    let mut voxels = Vec::new();
    let builtins = default_colors();

    for (z, rows) in &collected.layers {
        for (y, row) in rows.iter().enumerate() {
            for (x, symbol) in unicode_segmentation::UnicodeSegmentation::graphemes(row.as_str(), true).enumerate() {
                if symbol == "." || symbol == " " {
                    continue;
                }

                let raw = collected.explicit_colors
                    .get(symbol)
                    .cloned()
                    .or_else(|| builtins.get(symbol).cloned())
                    .unwrap_or_else(|| default_color_for_symbol(symbol));

                let hex = builtins.get(&raw).cloned().unwrap_or(raw);

                voxels.push(Voxel { 
                    x: x as i32, 
                    y: y as i32, 
                    z: *z as i32, 
                    color: hex 
                });
            }
        }
    }

    Model { name: "anonymous".into(), voxels }
}

/// Deterministic fallback color generator for unknown symbols
fn default_color_for_symbol(sym: &str) -> String {
    let mut hasher = DefaultHasher::new();
    sym.hash(&mut hasher);
    let h = hasher.finish();
    format!("#{:06x}", (h as u32) & 0xffffff)
}

fn apply_commands(scene: &mut SceneGraph, commands: &[(String, Vec<String>)]) {
    for (name, args) in commands {
        match name.as_str() {
            "print" => do_print(scene),
            "clone" => do_clone(scene),
            "translate" => do_translate(scene, args),
            "rotate" => do_rotate(scene, args),
            "merge" => do_merge(scene),
            _ => println!("Unknown command: {}", name),
        }
    }
}




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
    let turns = ((turns % 4) + 4) % 4; // normalize to 0..3

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
        voxels.push(Voxel {
            x, y, z,
            color: v.color.clone(),
        });
    }

    let mut rotated = VoxelScene { voxels };
    rotated.normalize(); // <- shift back into positive space
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



pub fn run(ast: Vec<AstNode>) -> VoxelScene {
    let scene_graph = build_scene(ast);
    scene_graph.flatten()
}
