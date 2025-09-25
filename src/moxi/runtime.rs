use crate::types::{Voxel, VoxelScene};
use crate::colors::default_colors;
use super::parser::AstNode;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use unicode_segmentation::UnicodeSegmentation;

/// Deterministic fallback color generator for unknown symbols
fn default_color_for_symbol(sym: &str) -> String {
    let mut hasher = DefaultHasher::new();
    sym.hash(&mut hasher);
    let h = hasher.finish();
    format!("#{:06x}", (h as u32) & 0xffffff)
}

/// Container for intermediate AST info
struct CollectedAst {
    explicit_colors: HashMap<String, String>,
    layers: Vec<(usize, Vec<String>)>,
    commands: Vec<(String, Vec<String>)>,
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
fn expand_layers(collected: &CollectedAst) -> VoxelScene {
    let mut voxels = Vec::new();
    let builtins = default_colors();

    for (z, rows) in &collected.layers {
        for (y, row) in rows.iter().enumerate() {
            for (x, symbol) in UnicodeSegmentation::graphemes(row.as_str(), true).enumerate() {
                if symbol == "." || symbol == " " {
                    continue;
                }

                // Step 1: declared → builtins → fallback
                let raw = collected.explicit_colors
                    .get(symbol)
                    .cloned()
                    .or_else(|| builtins.get(symbol).cloned())
                    .unwrap_or_else(|| default_color_for_symbol(symbol));

                // Step 2: normalize if still a named color
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

    VoxelScene { voxels }
}

/// Apply commands like print / translate / clone / rotate / merge
fn apply_commands(scene: &mut VoxelScene, commands: &[(String, Vec<String>)]) {
    let mut last_clone_size = 0;

    for (name, args) in commands {
        match name.as_str() {
            "print" => {
                println!("Scene has {} voxels", scene.voxels.len());
            }

            "clone" => {
                let copy = clone_scene(scene);
                last_clone_size = copy.voxels.len();
                scene.voxels.extend(copy.voxels);
                println!(
                    "Cloned scene: added {} voxels, now {} total",
                    last_clone_size,
                    scene.voxels.len()
                );
            }

            "translate" => {
                if args.len() >= 3 {
                    let dx: i32 = args[0].parse().unwrap_or(0);
                    let dy: i32 = args[1].parse().unwrap_or(0);
                    let dz: i32 = args[2].parse().unwrap_or(0);

                    let n = scene.voxels.len();
                    let start = n.saturating_sub(last_clone_size);

                    for v in &mut scene.voxels[start..] {
                        v.x += dx;
                        v.y += dy;
                        v.z += dz;
                    }

                    println!(
                        "Translated last {} voxels by ({}, {}, {})",
                        last_clone_size, dx, dy, dz
                    );
                }
            }

            "rotate" => {
                if args.len() >= 2 {
                    let axis = args[0].as_str();
                    let turns: i32 = args[1].parse().unwrap_or(1);

                    let n = scene.voxels.len();
                    let start = n.saturating_sub(last_clone_size);

                    let rotated: Vec<_> = scene.voxels[start..]
                        .iter()
                        .map(|v| {
                            let (mut x, mut y, mut z) = (v.x, v.y, v.z);
                            for _ in 0..((turns % 4 + 4) % 4) {
                                match axis {
                                    "x" => { let ny = -z; let nz = y; y = ny; z = nz; }
                                    "y" => { let nx = z; let nz = -x; x = nx; z = nz; }
                                    "z" => { let nx = -y; let ny = x; x = nx; y = ny; }
                                    _ => {}
                                }
                            }
                            Voxel { x, y, z, color: v.color.clone() }
                        })
                        .collect();

                    scene.voxels.truncate(start);
                    scene.voxels.extend(rotated);

                    scene.normalize(); // <- keep everything non-negative

                    println!(
                        "Rotated last {} voxels around {} by {} quarter turns",
                        last_clone_size, axis, turns
                    );
                }
            }

            "merge" => {
                println!("Merge: scene has {} voxels", scene.voxels.len());
            }

            _ => {
                println!("Unknown command: {}", name);
            }
        }
    }
}


/// Clone a voxel scene (deep copy)
pub fn clone_scene(scene: &VoxelScene) -> VoxelScene {
    VoxelScene { voxels: scene.voxels.clone() }
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

/// Public entrypoint
pub fn build_scene(ast: Vec<AstNode>) -> VoxelScene {
    let collected = collect_ast(&ast);
    let mut scene = expand_layers(&collected);
    apply_commands(&mut scene, &collected.commands);
    scene.normalize(); // <- always normalize final result
    scene
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
    for node in &ast {
        println!("Executing node: {:?}", node);
    }
    super::runtime::build_scene(ast)
}


