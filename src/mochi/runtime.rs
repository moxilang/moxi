use crate::types::{Voxel, VoxelScene};
use crate::colors::default_colors;
use super::parser::AstNode;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Deterministic fallback color generator for unknown symbols
fn default_color_for_symbol(sym: &str) -> String {
    let mut hasher = DefaultHasher::new();
    sym.hash(&mut hasher);
    let h = hasher.finish();
    format!("#{:06x}", (h as u32) & 0xffffff)
}

/// Build a voxel scene from the AST
pub fn build_scene(ast: Vec<AstNode>) -> VoxelScene {
    let mut voxels: Vec<Voxel> = Vec::new();

    // Registries
    let mut scenes: HashMap<String, VoxelScene> = HashMap::new();
    let mut explicit_colors: HashMap<String, String> = HashMap::new();
    let mut layers: Vec<(usize, Vec<String>)> = Vec::new();

    // === 1. Collect declarations from AST ===
    for node in &ast {
        match node {
            AstNode::VoxelDecl { name } => {
                scenes.insert(name.clone(), VoxelScene { voxels: vec![] });
                println!("Executing voxel declaration for {}", name);
            }
            AstNode::ColorDecl { symbol, color } => {
                explicit_colors.insert(symbol.clone(), color.clone());
            }
            AstNode::LayerDecl { z, rows } => {
                layers.push((*z, rows.clone()));
            }
            _ => {}
        }
    }

    // === 2. Built-in named colors ===
    let builtins = default_colors();

    // === 3. Convert layers → voxels ===
    for (z, rows) in layers {
        for (y, row) in rows.iter().enumerate() {
            for (x, symbol) in
                unicode_segmentation::UnicodeSegmentation::graphemes(row.as_str(), true).enumerate()
            {
                if symbol == "." || symbol == " " {
                    continue;
                }

                // Resolve color: explicit > builtin > fallback
                let raw = explicit_colors
                    .get(symbol)
                    .cloned()
                    .or_else(|| builtins.get(symbol).cloned())
                    .unwrap_or_else(|| default_color_for_symbol(symbol));

                let hex = builtins.get(&raw).cloned().unwrap_or(raw);

                voxels.push(Voxel { x, y, z, color: hex });
            }
        }
    }

    // === 4. Handle commands (translate, merge, etc.) ===
    for node in ast {
        if let AstNode::Command { name, args } = node {
            match name.as_str() {
                "print" => {
                    println!("Scene has {} voxels", voxels.len());
                }
                "translate" => {
                    if args.len() >= 4 {
                        let base = args[0].clone();
                        let dx: isize = args[1].parse().unwrap_or(0);
                        let dy: isize = args[2].parse().unwrap_or(0);
                        let dz: isize = args[3].parse().unwrap_or(0);

                        if let Some(scene) = scenes.get(&base) {
                            let moved = translate(scene, dx, dy, dz);
                            scenes.insert(format!("{}_translated", base), moved.clone());
                            voxels.extend(moved.voxels);
                            println!("Translated {} by ({}, {}, {})", base, dx, dy, dz);
                        }
                    }
                }
                "merge" => {
                    if !args.is_empty() {
                        let mut collected = Vec::new();
                        for a in &args {
                            if let Some(scene) = scenes.get(a) {
                                collected.push(scene.clone());
                            }
                        }
                        let merged = merge(collected);
                        scenes.insert("merged".to_string(), merged.clone());
                        voxels.extend(merged.voxels);
                        println!("Merged scenes: {:?}", args);
                    }
                }
                _ => println!("Unknown command: {}", name),
            }
        }
    }

    VoxelScene { voxels }
}

/// Translate a voxel scene
pub fn translate(scene: &VoxelScene, dx: isize, dy: isize, dz: isize) -> VoxelScene {
    let voxels = scene
        .voxels
        .iter()
        .map(|v| Voxel {
            x: (v.x as isize + dx) as usize,
            y: (v.y as isize + dy) as usize,
            z: (v.z as isize + dz) as usize,
            color: v.color.clone(),
        })
        .collect();
    VoxelScene { voxels }
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


