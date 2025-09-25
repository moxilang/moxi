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
}

/// Pass 1: collect all declarations (but don’t create voxels yet)
fn collect_ast(ast: &[AstNode]) -> CollectedAst {
    let mut explicit_colors = HashMap::new();
    let mut layers = Vec::new();

    for node in ast {
        match node {
            AstNode::ColorDecl { symbol, color } => {
                explicit_colors.insert(symbol.clone(), color.clone());
            }
            AstNode::LayerDecl { z, rows } => {
                layers.push((*z, rows.clone()));
            }
            _ => {}
        }
    }

    CollectedAst { explicit_colors, layers }
}

/// Pass 2: expand layers → voxels using colors
fn expand_layers(collected: CollectedAst) -> VoxelScene {
    let mut voxels = Vec::new();

    let builtins = default_colors();
    let CollectedAst { explicit_colors, layers } = collected;

    for (z, rows) in layers {
        for (y, row) in rows.iter().enumerate() {
            for (x, symbol) in
                UnicodeSegmentation::graphemes(row.as_str(), true).enumerate()
            {
                if symbol == "." || symbol == " " {
                    continue;
                }

                // Step 1: declared → builtins → fallback
                let raw = explicit_colors
                    .get(symbol)
                    .cloned()
                    .or_else(|| builtins.get(symbol).cloned())
                    .unwrap_or_else(|| default_color_for_symbol(symbol));

                // Step 2: normalize if still a named color
                let hex = builtins.get(&raw).cloned().unwrap_or(raw);

                voxels.push(Voxel { x, y, z, color: hex });
            }
        }
    }

    VoxelScene { voxels }
}

/// Public entrypoint
pub fn build_scene(ast: Vec<AstNode>) -> VoxelScene {
    let collected = collect_ast(&ast);
    expand_layers(collected)
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


