use std::collections::HashMap;

use crate::types::{Voxel, Model, Value};
use crate::colors::default_colors;
use super::super::parser::AstNode;


/// Compile a voxel model body into a concrete Model
pub fn eval_model_body(
    model_name: &str,
    body: &[AstNode],
    env: &HashMap<String, Value>,
) -> Model {
    let (colors, legend, pending_layers) =
        collect_model_metadata(body, env);

    let voxels = emit_voxels(pending_layers, &colors, &legend, env);

    Model {
        name: model_name.to_string(),
        voxels,
    }
}

/// Convert a Value into a string (used for env chasing)
fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => format!("{:?}", v),
    }
}


/// === PASS 1 ===
/// Collect colors, legend mappings, and layer definitions
fn collect_model_metadata(
    body: &[AstNode],
    env: &HashMap<String, Value>,
) -> (
    HashMap<String, String>,              // colors
    HashMap<String, String>,              // legend: glyph -> atom
    Vec<(usize, Vec<String>)>,            // pending layers
) {
    let mut colors = default_colors();
    let mut legend: HashMap<String, String> = HashMap::new();
    let mut pending_layers: Vec<(usize, Vec<String>)> = Vec::new();

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
                let mut resolved_name = color.clone();

                while let Some(val) = env.get(&resolved_name) {
                    resolved_name = match val {
                        Value::Array(vs) if !vs.is_empty() => value_to_string(&vs[0]),
                        _ => value_to_string(val),
                    };
                    if !env.contains_key(&resolved_name) {
                        break;
                    }
                }

                let final_hex = if resolved_name.starts_with('#') {
                    resolved_name[..7.min(resolved_name.len())].to_string()
                } else {
                    default_colors()
                        .get(&resolved_name)
                        .cloned()
                        .unwrap_or(resolved_name)
                };

                colors.insert(symbol.clone(), final_hex);
            }

            AstNode::LegendDecl { mappings } => {
                for (glyph, atom) in mappings {
                    legend.insert(glyph.clone(), atom.clone());
                }
            }

            AstNode::LayerDecl { z, rows } => {
                pending_layers.push((*z, rows.clone()));
            }

            _ => {}
        }
    }

    (colors, legend, pending_layers)
}

/// === PASS 2 ===
/// Emit concrete voxels using legend → atom → color resolution
/// 
fn emit_voxels(
    pending_layers: Vec<(usize, Vec<String>)>,
    colors: &HashMap<String, String>,
    legend: &HashMap<String, String>,
    env: &HashMap<String, Value>,
) -> Vec<Voxel> {
    let mut voxels = Vec::new();

    for (z, rows) in pending_layers {
        for (y, row) in rows.iter().enumerate() {
            for (x, sym) in row.chars().enumerate() {
                if sym == '.' || sym == ' ' {
                    continue;
                }

                if !sym.is_ascii() {
                    eprintln!("⚠️ Non-ASCII glyph '{}' is deprecated", sym);
                }

                let glyph = sym.to_string();

                let color = if let Some(atom_name) = legend.get(&glyph) {
                    match env.get(atom_name) {
                        Some(Value::Atom { props, .. }) => {
                            props.get("color")
                                .and_then(|c| {
                                    default_colors()
                                        .get(c)
                                        .cloned()
                                        .or(Some(c.clone()))
                                })
                                .unwrap_or("#888888".into())
                        }
                        _ => {
                            eprintln!("⚠️ Unknown atom '{}'", atom_name);
                            "#888888".into()
                        }
                    }
                } else if let Some(c) = colors.get(&glyph) {
                    // 🔁 legacy color mapping
                    c.clone()
                } else {
                    eprintln!("⚠️ Glyph '{}' has no legend or color", glyph);
                    "#888888".into()
                };


                voxels.push(Voxel {
                    x: x as i32,
                    y: y as i32,
                    z: z as i32,
                    color,
                });
            }
        }
    }

    voxels
}
