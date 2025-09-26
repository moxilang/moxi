use std::collections::HashMap;
use crate::types::{Voxel, Model, Value};
use crate::colors::default_colors;

use super::util::value_to_string;
use super::super::parser::AstNode;

pub fn eval_model_body(body: &[AstNode], env: &HashMap<String, Value>) -> Model {
    let mut pending_layers: Vec<(usize, Vec<String>)> = Vec::new();
    let mut voxels = Vec::new();
    let mut colors = default_colors();

    for node in body {
        match node {
            AstNode::ColorDecl { symbol, color } => {
                let resolved = default_colors()
                    .get(color)
                    .cloned()
                    .unwrap_or(color.clone());
                colors.insert(symbol.clone(), resolved);
            }

            AstNode::AddColor { symbol, color } => {
                // Step 1: start with raw identifier/literal
                let mut resolved_name = color.clone();

                // Step 2: resolve through env (chase variables → arrays → strings)
                while let Some(val) = env.get(&resolved_name) {
                    resolved_name = match val {
                        Value::Array(vs) if !vs.is_empty() => value_to_string(&vs[0]),
                        _ => value_to_string(val),
                    };
                    // break once it’s no longer a variable in env
                    if !env.contains_key(&resolved_name) {
                        break;
                    }
                }

                // Step 3: final hex resolution
                let final_hex = if resolved_name.starts_with('#') {
                    if resolved_name.len() == 9 {
                        resolved_name[..7].to_string() // strip alpha → #RRGGBB
                    } else {
                        resolved_name
                    }
                } else {
                    default_colors()
                        .get(&resolved_name)
                        .cloned()
                        .unwrap_or(resolved_name)
                };

                colors.insert(symbol.clone(), final_hex);
            }

            AstNode::LayerDecl { z, rows } => {
                pending_layers.push((*z, rows.clone()));
            }

            _ => {}
        }
    }

    for (z, rows) in pending_layers {
        for (y, row) in rows.iter().enumerate() {
            for (x, sym) in row.chars().enumerate() {
                if sym == '.' || sym == ' ' {
                    continue;
                }
                let key = sym.to_string();
                let resolved = colors
                    .get(&key)
                    .cloned()
                    .unwrap_or("#888888".into());
                voxels.push(Voxel {
                    x: x as i32,
                    y: y as i32,
                    z: z as i32,
                    color: resolved,
                });
            }
        }
    }

    Model { name: "anonymous".into(), voxels }
}
