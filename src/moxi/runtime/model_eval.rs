use std::collections::HashMap;

use crate::types::{Voxel, Model, Value};
use crate::colors::default_colors;
use super::super::parser::AstNode;

/// Compile a voxel model body into a concrete Model (STRICT MODE)
pub fn eval_model_body(
    model_name: &str,
    body: &[AstNode],
    env: &HashMap<String, Value>,
) -> Model {
    let (_colors, legend, pending_layers) =
        collect_model_metadata(body, env);

    let voxels = emit_voxels_strict(pending_layers, &legend, env);

    Model {
        name: model_name.to_string(),
        voxels,
    }
}

/// === PASS 1 ===
/// Collect legend mappings and layer definitions
fn collect_model_metadata(
    body: &[AstNode],
    _env: &HashMap<String, Value>,
) -> (
    HashMap<String, String>,              // colors (unused in strict mode)
    HashMap<String, String>,              // legend: glyph -> atom
    Vec<(usize, Vec<String>)>,            // pending layers
) {
    let colors = default_colors(); // kept for forward compatibility
    let mut legend: HashMap<String, String> = HashMap::new();
    let mut pending_layers: Vec<(usize, Vec<String>)> = Vec::new();

    for node in body {
        match node {
            AstNode::LegendDecl { mappings } => {
                for (glyph, atom) in mappings {
                    legend.insert(glyph.clone(), atom.clone());
                }
            }

            AstNode::LayerDecl { z, rows } => {
                pending_layers.push((*z, rows.clone()));
            }

            // ❌ ColorDecl / AddColor are now illegal in strict mode
            AstNode::ColorDecl { .. } | AstNode::AddColor { .. } => {
                panic!("❌ Color declarations are not allowed in strict mode. Use atoms + legend.");
            }

            _ => {}
        }
    }

    (colors, legend, pending_layers)
}

/// === PASS 2 ===
/// Emit voxels using *strict* legend → atom → color resolution
fn emit_voxels_strict(
    pending_layers: Vec<(usize, Vec<String>)>,
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

                // 1️⃣ ASCII ONLY
                if !sym.is_ascii() {
                    panic!("❌ Non-ASCII glyph '{}' is not allowed in strict mode", sym);
                }

                let glyph = sym.to_string();

                // 2️⃣ MUST exist in legend
                let atom_name = legend.get(&glyph).unwrap_or_else(|| {
                    panic!("❌ Glyph '{}' is not defined in legend", glyph);
                });

                // 3️⃣ Atom must exist
                let atom = env.get(atom_name).unwrap_or_else(|| {
                    panic!("❌ Legend references unknown atom '{}'", atom_name);
                });

                // 4️⃣ Atom must have color
                let color = match atom {
                    Value::Atom { props, .. } => {
                        let color_name = props.get("color").unwrap_or_else(|| {
                            panic!("❌ Atom '{}' is missing required property: color", atom_name);
                        });

                        default_colors()
                            .get(color_name)
                            .cloned()
                            .unwrap_or_else(|| {
                                panic!(
                                    "❌ Unknown color '{}' in atom '{}'",
                                    color_name, atom_name
                                )
                            })
                    }

                    _ => {
                        panic!("❌ '{}' is not an atom", atom_name);
                    }
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
