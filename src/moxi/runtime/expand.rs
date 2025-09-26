use crate::types::{Voxel, Model};
use crate::colors::default_colors;
use unicode_segmentation::UnicodeSegmentation;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use super::ast_collect::ModelAst;

pub fn expand_layers(m: &ModelAst) -> Model {
    let mut voxels = Vec::new();
    let builtins = default_colors();

    for (z, rows) in &m.layers {
        for (y, row) in rows.iter().enumerate() {
            for (x, symbol) in UnicodeSegmentation::graphemes(row.as_str(), true).enumerate() {
                if symbol == "." || symbol == " " { continue; }

                let raw = m.explicit_colors
                    .get(symbol)
                    .cloned()
                    .or_else(|| builtins.get(symbol).cloned())
                    .unwrap_or_else(|| default_color_for_symbol(symbol));

                let hex = builtins.get(&raw).cloned().unwrap_or(raw);

                voxels.push(Voxel {
                    x: x as i32,
                    y: y as i32,
                    z: *z as i32,
                    color: hex,
                });
            }
        }
    }

    Model { name: m.name.clone(), voxels }
}

pub fn default_color_for_symbol(sym: &str) -> String {
    let mut hasher = DefaultHasher::new();
    sym.hash(&mut hasher);
    let h = hasher.finish();
    format!("#{:06x}", (h as u32) & 0xffffff)
}
