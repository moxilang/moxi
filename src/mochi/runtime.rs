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

/// Lower an AST into a VoxelScene
pub fn build_scene(ast: Vec<AstNode>) -> VoxelScene {
    let mut voxels = Vec::new();
    let mut explicit_colors: HashMap<String, String> = HashMap::new();
    let mut layers: Vec<(usize, Vec<String>)> = Vec::new();

    // 1. Collect from AST
    for node in &ast {
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

    // 2. Built-in named colors (mochi-pink, brown, etc.)
    let builtins = default_colors();

    // 3. Convert layers → voxels
    for (z, rows) in layers {
        for (y, row) in rows.iter().enumerate() {
            // use grapheme clusters (for emojis)
            for (x, symbol) in unicode_segmentation::UnicodeSegmentation::graphemes(row.as_str(), true).enumerate() {
                if symbol == "." || symbol == " " {
                    continue;
                }

                // Step 1: find declared color
                let raw = explicit_colors
                    .get(symbol)
                    .cloned()
                    .or_else(|| builtins.get(symbol).cloned())
                    .unwrap_or_else(|| default_color_for_symbol(symbol));

                // Step 2: normalize to hex
                let hex = builtins
                    .get(&raw) // maybe "green" → "#00ff00"
                    .cloned()
                    .unwrap_or(raw);

                voxels.push(Voxel { x, y, z, color: hex });
            }
        }
    }

    VoxelScene { voxels }
}


pub fn run(ast: Vec<AstNode>) -> VoxelScene {
    for node in &ast {
        println!("Executing node: {:?}", node);
    }
    super::runtime::build_scene(ast)
}


