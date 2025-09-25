use crate::types::*;
use crate::colors::default_colors;
use std::collections::HashMap;
use std::fs;

/// Legacy voxel grid parser (from ASCII [Layer]/[Colors] format).
pub fn parse_voxgrid_file(path: &str) -> anyhow::Result<VoxelScene> {
    let contents = fs::read_to_string(path)?;
    let mut voxels = Vec::new();
    let mut color_map: ColorMap = HashMap::new();
    let mut layers: Vec<Vec<String>> = Vec::new();
    let mut current_layer: Vec<String> = Vec::new();
    let mut _z = 0;
    let mut in_colors = false;

    for line in contents.lines() {
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        if line.starts_with("[Layer") {
            if !current_layer.is_empty() {
                layers.push(current_layer);
                current_layer = Vec::new();
                _z += 1;
            }
            in_colors = false;
        } else if line.starts_with("[Colors]") {
            if !current_layer.is_empty() {
                layers.push(current_layer.clone());
            }
            in_colors = true;
        } else if in_colors {
            if let Some((k, v)) = line.split_once(':') {
                color_map.insert(k.trim().to_string(), v.trim().to_string());
            }
        } else {
            current_layer.push(line.to_string());
        }
    }

    if !current_layer.is_empty() {
        layers.push(current_layer.clone());
    }

    // Convert layers to voxels
    for (z_idx, layer) in layers.iter().enumerate() {
        for (y, row) in layer.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                if ch == '.' || ch == ' ' {
                    continue;
                }
                let key = ch.to_string();

                let fallback_colors = default_colors();
                let raw_color = color_map.get(&key)
                    .or_else(|| fallback_colors.get(&key))
                    .cloned()
                    .unwrap_or_else(|| "#888888".to_string());

                let color = fallback_colors.get(&raw_color).cloned().unwrap_or(raw_color);

                voxels.push(Voxel { x, y, z: z_idx, color });
            }
        }
    }

    Ok(VoxelScene { voxels })
}
