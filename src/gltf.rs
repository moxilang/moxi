//! `moxi gltf` — the solved scene as a glTF 2.0 node tree.
//!
//! One node per part, named exactly as the part (`ArmR`, `HammerHead`…);
//! the hierarchy is the relation tree (crate::joints: the object of a
//! part's mate is its parent); each node's origin and axes are the JOINT
//! frame — the mate point, after shift/gap/lean/twist/pitch/flip. So a
//! consumer that turns `ArmR` turns it about the shoulder it was mated at,
//! and everything mated onto the arm comes along. Each printed thing is a
//! root node named after the thing.
//!
//! Geometry is the same surface-nets mesh `moxi mesh` writes, expressed in
//! node space: v_node = N⁻¹ · F · v_local. Composing the node chain gives
//! back N, so world vertices equal `moxi mesh`'s — the parity claim the
//! tests check.
//!
//! Output is GLB (one binary file) by default; `--text` writes `.gltf`
//! JSON with the buffer inlined as a base64 data URI, for tests and review.
//! Materials: one per (material, colour), PBR metal-rough, baseColorFactor
//! in LINEAR space (converted from the sRGB hex), metallic 0.
//! Node extras carry `{"moxi":{"part","parent","thing"}}`, plus `joint` (ranges) and
//! `breakable` (`hits` or `event`, and `push` in the thing's space) when declared.

use std::collections::HashMap;

use serde_json::{json, Value};

use crate::frame::{Frame, Mat3, Vec3};
use crate::mesh::{auto_cell, surface_nets};
use crate::scene::Scene;

/// A glTF document: the JSON and its single binary buffer.
pub struct Gltf {
    pub json: Value,
    pub bin:  Vec<u8>,
}

const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;
const FLOAT: u32 = 5126;
const UNSIGNED_INT: u32 = 5125;

/// Build the document. `cell` overrides the per-part automatic mesh cell.
pub fn scene_to_gltf(scene: &Scene, cell: Option<f64>) -> Gltf {
    let mut nodes:     Vec<Value> = Vec::new();
    let mut meshes:    Vec<Value> = Vec::new();
    let mut accessors: Vec<Value> = Vec::new();
    let mut views:     Vec<Value> = Vec::new();
    let mut materials: Vec<Value> = Vec::new();
    let mut mat_index: HashMap<(String, String, u64, u64, bool), usize> = HashMap::new();
    let mut emissive_strength_used = false;
    // KHR_materials_variants: "rest" plus every state, across all layers
    let mut variants: Vec<String> = Vec::new();
    for l in &scene.layers {
        for st in &l.states {
            if !variants.contains(&st.name) { variants.push(st.name.clone()); }
        }
    }
    if !variants.is_empty() { variants.insert(0, "rest".to_string()); }
    let mut bin: Vec<u8> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    let mut animations: Vec<Value> = Vec::new();

    for layer in &scene.layers {
        let layer_node = nodes.len();
        nodes.push(json!({ "name": layer.thing, "extras": { "moxi": { "thing": layer.thing } } }));
        roots.push(layer_node);

        // World joint frame per part (falls back to the part frame).
        let joint_of: HashMap<&str, Frame> = layer.parts.iter()
            .map(|p| (p.name.as_str(), p.joint.as_ref().map(|j| j.frame.to_frame())
                .unwrap_or_else(|| p.frame.to_frame())))
            .collect();
        let first_node = nodes.len();
        let index_of: HashMap<&str, usize> = layer.parts.iter().enumerate()
            .map(|(i, p)| (p.name.as_str(), first_node + i))
            .collect();

        let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
        for part in &layer.parts {
            let n = joint_of[part.name.as_str()];
            let parent = part.joint.as_ref().and_then(|j| j.parent.as_deref())
                .filter(|p| index_of.contains_key(p) && *p != part.name);
            let (parent_node, parent_frame) = match parent {
                Some(p) => (index_of[p], joint_of[p]),
                None    => (layer_node, Frame::IDENTITY),
            };
            children.entry(parent_node).or_default().push(index_of[part.name.as_str()]);

            let local = parent_frame.inverse().compose(&n);
            let mut node = json!({
                "name": part.name,
                "translation": [local.pos.x, local.pos.y, local.pos.z],
                "rotation": quat_of(&local.rot),
                "extras": { "moxi": { "part": part.name, "parent": parent, "thing": layer.thing } },
            });
            if let Some(j) = part.joint.as_ref().filter(|j| !j.limits.is_empty()) {
                node["extras"]["moxi"]["joint"] = json!({ "limits": j.limits, "rest": j.rest });
            }
            // A breakable mate: this node and its subtree leave as one lump.
            // `push` is in the thing's space — the frame of the thing's node.
            if let Some(b) = &part.breakable {
                node["extras"]["moxi"]["breakable"] = json!(b);
            }

            // Geometry in node space.
            let expr = part.shape.to_expr();
            let c    = cell.unwrap_or_else(|| auto_cell(&expr, layer.voxel_size));
            let tri  = surface_nets(&expr, c, layer.voxel_size);
            if !tri.indices.is_empty() {
                let to_node = n.inverse().compose(&part.frame.to_frame());
                let pos: Vec<[f32; 3]> = tri.positions.iter().map(|p| {
                    let v = to_node.apply_point(Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
                    [v.x as f32, v.y as f32, v.z as f32]
                }).collect();
                let nor: Vec<[f32; 3]> = tri.normals.iter().map(|d| {
                    let v = to_node.apply_dir(Vec3::new(d[0] as f64, d[1] as f64, d[2] as f64));
                    let l = v.length();
                    if l > 1e-12 { [(v.x / l) as f32, (v.y / l) as f32, (v.z / l) as f32] } else { [0.0, 1.0, 0.0] }
                }).collect();

                let (mn, mx) = bounds(&pos);
                let pa = push_accessor(&mut bin, &mut views, &mut accessors, f32s(&pos), Some(ARRAY_BUFFER),
                    json!({ "componentType": FLOAT, "count": pos.len(), "type": "VEC3", "min": mn, "max": mx }));
                let na = push_accessor(&mut bin, &mut views, &mut accessors, f32s(&nor), Some(ARRAY_BUFFER),
                    json!({ "componentType": FLOAT, "count": nor.len(), "type": "VEC3" }));
                let ia = push_accessor(&mut bin, &mut views, &mut accessors,
                    tri.indices.iter().flat_map(|i| i.to_le_bytes()).collect(), Some(ELEMENT_ARRAY_BUFFER),
                    json!({ "componentType": UNSIGNED_INT, "count": tri.indices.len(), "type": "SCALAR" }));

                let mut material = |name: Option<&String>, color: &str, emissive: Option<f64>, opacity: Option<f64>, additive: bool| -> usize {
                    let e = emissive.unwrap_or(0.0);
                    let a = opacity.unwrap_or(1.0);
                    let key = (name.cloned().unwrap_or_default(), color.to_string(), e.to_bits(), a.to_bits(), additive);
                    *mat_index.entry(key.clone()).or_insert_with(|| {
                        let label = if key.0.is_empty() { key.1.clone() } else { key.0.clone() };
                        let mut base = linear_rgba(&key.1);
                        base[3] = a;
                        let mut mat = json!({
                            "name": label,
                            "pbrMetallicRoughness": {
                                "baseColorFactor": base,
                                "metallicFactor": 0.0,
                                "roughnessFactor": 0.9,
                            },
                        });
                        if e > 0.0 {
                            // the material glows in its own colour × e: factor ≤ 1,
                            // the rest as KHR_materials_emissive_strength
                            let k = e.min(1.0);
                            mat["emissiveFactor"] = json!([base[0] * k, base[1] * k, base[2] * k]);
                            if e > 1.0 {
                                mat["extensions"] = json!({ "KHR_materials_emissive_strength": { "emissiveStrength": e } });
                                emissive_strength_used = true;
                            }
                        }
                        if a < 1.0 || additive {
                            mat["alphaMode"] = json!("BLEND");
                        }
                        if additive {
                            // glTF has no additive mode: say it in extras (three.js
                            // puts a material's extras in `material.userData`)
                            mat["extras"] = json!({ "moxi": { "blend": "additive" } });
                        }
                        materials.push(mat);
                        materials.len() - 1
                    })
                };
                let m = material(part.material.as_ref(), &part.color, part.emissive, part.opacity, part.blend.is_some());
                let mut prim = json!({ "attributes": { "POSITION": pa, "NORMAL": na }, "indices": ia, "material": m });
                // one mapping per distinct look this part takes in some state
                let mut mappings: Vec<(usize, Vec<usize>)> = Vec::new();
                for st in &layer.states {
                    let Some(look) = st.looks.get(&part.name) else { continue };
                    let lm = material(look.material.as_ref(), &look.color, look.emissive, look.opacity, look.blend.is_some());
                    let vi = variants.iter().position(|v| *v == st.name).expect("variant listed");
                    match mappings.iter_mut().find(|(x, _)| *x == lm) {
                        Some((_, vs)) => vs.push(vi),
                        None => mappings.push((lm, vec![vi])),
                    }
                }
                if !mappings.is_empty() {
                    // `rest` names the default look explicitly: not every viewer
                    // falls back to the default material by itself
                    match mappings.iter_mut().find(|(x, _)| *x == m) {
                        Some((_, vs)) => vs.insert(0, 0),
                        None => mappings.insert(0, (m, vec![0])),
                    }
                    prim["extensions"] = json!({ "KHR_materials_variants": { "mappings":
                        mappings.iter().map(|(m, vs)| json!({ "material": m, "variants": vs })).collect::<Vec<_>>() } });
                }

                node["mesh"] = json!(meshes.len());
                meshes.push(json!({ "name": part.name, "primitives": [prim] }));
            }
            nodes.push(node);
        }
        for (parent, kids) in children {
            nodes[parent]["children"] = json!(kids);
        }

        // Animations: per keyframe, each node's local transform from that
        // keyframe's joint frames; only nodes that move get channels.
        for anim in &layer.animations {
            let parent_of = |part: &str| layer.parts.iter().find(|p| p.name == part)
                .and_then(|p| p.joint.as_ref()).and_then(|j| j.parent.clone())
                .filter(|q| index_of.contains_key(q.as_str()));
            let times: Vec<f32> = anim.keys.iter().map(|k| k.time as f32).collect();
            let mut samplers: Vec<Value> = Vec::new();
            let mut channels: Vec<Value> = Vec::new();
            let mut time_acc: Option<usize> = None;
            for part in &layer.parts {
                let locals: Vec<Frame> = anim.keys.iter().map(|k| {
                    let n = k.joints.get(&part.name).map(|f| f.to_frame()).unwrap_or(joint_of[part.name.as_str()]);
                    let pf = parent_of(&part.name)
                        .and_then(|q| k.joints.get(&q).map(|f| f.to_frame()))
                        .unwrap_or(Frame::IDENTITY);
                    pf.inverse().compose(&n)
                }).collect();
                let moves = locals.windows(2).any(|w| {
                    w[0].pos.sub(w[1].pos).length() > 1e-9
                        || (0..3).any(|r| (0..3).any(|c| (w[0].rot.0[r][c] - w[1].rot.0[r][c]).abs() > 1e-9))
                });
                if !moves { continue; }
                let t_acc = *time_acc.get_or_insert_with(|| {
                    let (mn, mx) = (times[0], *times.last().unwrap_or(&0.0));
                    push_accessor(&mut bin, &mut views, &mut accessors,
                        times.iter().flat_map(|t| t.to_le_bytes()).collect(), None,
                        json!({ "componentType": FLOAT, "count": times.len(), "type": "SCALAR",
                                "min": [mn], "max": [mx] }))
                });
                let tr: Vec<[f32; 3]> = locals.iter().map(|f| [f.pos.x as f32, f.pos.y as f32, f.pos.z as f32]).collect();
                // sign-continuous quaternions, so each segment slerps the short way
                let mut qs: Vec<[f64; 4]> = Vec::new();
                for f in &locals {
                    let mut q = quat_of(&f.rot);
                    if let Some(prev) = qs.last() {
                        if prev[0] * q[0] + prev[1] * q[1] + prev[2] * q[2] + prev[3] * q[3] < 0.0 {
                            q = [-q[0], -q[1], -q[2], -q[3]];
                        }
                    }
                    qs.push(q);
                }
                let ta = push_accessor(&mut bin, &mut views, &mut accessors, f32s(&tr), None,
                    json!({ "componentType": FLOAT, "count": tr.len(), "type": "VEC3" }));
                let ra = push_accessor(&mut bin, &mut views, &mut accessors,
                    qs.iter().flat_map(|q| q.iter().flat_map(|c| (*c as f32).to_le_bytes())).collect(), None,
                    json!({ "componentType": FLOAT, "count": qs.len(), "type": "VEC4" }));
                let node = index_of[part.name.as_str()];
                for (path, out) in [("translation", ta), ("rotation", ra)] {
                    channels.push(json!({ "sampler": samplers.len(), "target": { "node": node, "path": path } }));
                    samplers.push(json!({ "input": t_acc, "output": out, "interpolation": "LINEAR" }));
                }
            }
            if channels.is_empty() { continue; }
            animations.push(json!({
                "name": anim.name,
                "channels": channels,
                "samplers": samplers,
                "extras": { "moxi": { "thing": layer.thing, "loop": anim.looped,
                    "keys": anim.keys.iter().map(|k| json!({ "time": k.time, "pose": k.pose })).collect::<Vec<_>>() } },
            }));
        }
    }

    let mut doc = json!({
        "asset": { "version": "2.0", "generator": format!("moxi {}", env!("CARGO_PKG_VERSION")) },
        "scene": 0,
        "scenes": [{ "nodes": roots }],
        "nodes": nodes,
    });
    if !meshes.is_empty() {
        doc["meshes"] = json!(meshes);
        doc["materials"] = json!(materials);
    }
    if !animations.is_empty() {
        doc["animations"] = json!(animations);
    }
    let mut used: Vec<&str> = Vec::new();
    if emissive_strength_used { used.push("KHR_materials_emissive_strength"); }
    if !variants.is_empty() {
        used.push("KHR_materials_variants");
        doc["extensions"] = json!({ "KHR_materials_variants": {
            "variants": variants.iter().map(|v| json!({ "name": v })).collect::<Vec<_>>() } });
    }
    if !used.is_empty() {
        doc["extensionsUsed"] = json!(used);
    }
    if !accessors.is_empty() {
        doc["accessors"] = json!(accessors);
        doc["bufferViews"] = json!(views);
        doc["buffers"] = json!([{ "byteLength": bin.len() }]);
    }
    Gltf { json: doc, bin }
}

impl Gltf {
    /// The binary container: header, JSON chunk (space-padded), BIN chunk
    /// (zero-padded), every chunk 4-byte aligned.
    pub fn to_glb(&self) -> Vec<u8> {
        let mut js = serde_json::to_vec(&self.json).expect("gltf json is serializable");
        while !js.len().is_multiple_of(4) { js.push(b' '); }
        let mut bin = self.bin.clone();
        while !bin.len().is_multiple_of(4) { bin.push(0); }
        let has_bin = !bin.is_empty();
        let total = 12 + 8 + js.len() + if has_bin { 8 + bin.len() } else { 0 };

        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(js.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes()); // "JSON"
        out.extend_from_slice(&js);
        if has_bin {
            out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
            out.extend_from_slice(&0x004E_4942u32.to_le_bytes()); // "BIN\0"
            out.extend_from_slice(&bin);
        }
        out
    }

    /// Text glTF with the buffer as a base64 data URI.
    pub fn to_text(&self) -> String {
        let mut doc = self.json.clone();
        if let Some(b) = doc.get_mut("buffers").and_then(|b| b.get_mut(0)) {
            b["uri"] = json!(format!("data:application/octet-stream;base64,{}", base64(&self.bin)));
        }
        serde_json::to_string_pretty(&doc).expect("gltf json is serializable")
    }
}

// ── helpers ────────────────────────────────────────────────────────────

fn push_accessor(
    bin: &mut Vec<u8>, views: &mut Vec<Value>, accessors: &mut Vec<Value>,
    bytes: Vec<u8>, target: Option<u32>, mut accessor: Value,
) -> usize {
    while !bin.len().is_multiple_of(4) { bin.push(0); }
    let mut view = json!({ "buffer": 0, "byteOffset": bin.len(), "byteLength": bytes.len() });
    if let Some(t) = target { view["target"] = json!(t); }
    views.push(view);
    bin.extend_from_slice(&bytes);
    accessor["bufferView"] = json!(views.len() - 1);
    accessors.push(accessor);
    accessors.len() - 1
}

fn f32s(v: &[[f32; 3]]) -> Vec<u8> {
    v.iter().flat_map(|p| p.iter().flat_map(|c| c.to_le_bytes())).collect()
}

fn bounds(v: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut mn = [f32::INFINITY; 3];
    let mut mx = [f32::NEG_INFINITY; 3];
    for p in v {
        for k in 0..3 { mn[k] = mn[k].min(p[k]); mx[k] = mx[k].max(p[k]); }
    }
    (mn, mx)
}

/// Unit quaternion [x, y, z, w] of a proper rotation (row-major).
pub fn quat_of(m: &Mat3) -> [f64; 4] {
    let r = &m.0;
    let tr = r[0][0] + r[1][1] + r[2][2];
    let q = if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [(r[2][1] - r[1][2]) / s, (r[0][2] - r[2][0]) / s, (r[1][0] - r[0][1]) / s, 0.25 * s]
    } else if r[0][0] > r[1][1] && r[0][0] > r[2][2] {
        let s = (1.0 + r[0][0] - r[1][1] - r[2][2]).sqrt() * 2.0;
        [0.25 * s, (r[0][1] + r[1][0]) / s, (r[0][2] + r[2][0]) / s, (r[2][1] - r[1][2]) / s]
    } else if r[1][1] > r[2][2] {
        let s = (1.0 + r[1][1] - r[0][0] - r[2][2]).sqrt() * 2.0;
        [(r[0][1] + r[1][0]) / s, 0.25 * s, (r[1][2] + r[2][1]) / s, (r[0][2] - r[2][0]) / s]
    } else {
        let s = (1.0 + r[2][2] - r[0][0] - r[1][1]).sqrt() * 2.0;
        [(r[0][2] + r[2][0]) / s, (r[1][2] + r[2][1]) / s, 0.25 * s, (r[1][0] - r[0][1]) / s]
    };
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    [q[0] / l, q[1] / l, q[2] / l, q[3] / l]
}

/// `#rrggbb` (sRGB) → linear [r, g, b, 1]; magenta if unparsable.
fn linear_rgba(hex: &str) -> [f64; 4] {
    let h = hex.trim_start_matches('#');
    let ch = |i: usize| h.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok());
    let (Some(r), Some(g), Some(b)) = (ch(0), ch(2), ch(4)) else { return [1.0, 0.0, 1.0, 1.0] };
    let lin = |c: u8| {
        let c = c as f64 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    [lin(r), lin(g), lin(b), 1.0]
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quaternion_round_trips_rotations() {
        for m in [Mat3::IDENTITY, Mat3::rot_x(1.0), Mat3::rot_y(-2.5), Mat3::rot_z(3.1),
                  Mat3::rot_x(0.3).mul(&Mat3::rot_y(2.9)).mul(&Mat3::rot_z(-1.7))] {
            let [x, y, z, w] = quat_of(&m);
            // rotate e_x by q and compare with the matrix column
            let back = [
                [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
                [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
                [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
            ];
            for r in 0..3 { for c in 0..3 { assert!((back[r][c] - m.0[r][c]).abs() < 1e-9); } }
        }
    }

    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn srgb_to_linear() {
        let c = linear_rgba("#ffffff");
        assert!((c[0] - 1.0).abs() < 1e-12);
        let g = linear_rgba("#808080");
        assert!((g[1] - 0.2158605).abs() < 1e-6);
        assert_eq!(linear_rgba("nope"), [1.0, 0.0, 1.0, 1.0]);
    }

    // ── Design-doc bench tests 2–4 (DOC-20261004-living-models-design §6) ──

    use crate::anchors::resolve_anchor;
    use crate::ast::{Expr, NamedArg};
    use crate::pipeline::compile_to_scene;

    const LAMP: &str = include_str!("../scripts/LAMP.md");
    const SKELETON: &str = include_str!("../scripts/SKELETON_v3.md");
    const AIM: &str = include_str!("../scripts/AIM.md");
    const FACING: &str = include_str!("../scripts/FACING.md");

    fn doc(src: &str) -> (Scene, Gltf) {
        let scene = compile_to_scene(src).expect("script compiles");
        let g = scene_to_gltf(&scene, None);
        (scene, g)
    }

    fn node_index(g: &Gltf, name: &str) -> usize {
        g.json["nodes"].as_array().unwrap().iter()
            .position(|n| n["name"] == name).unwrap_or_else(|| panic!("no node {name}"))
    }

    fn local_frame(n: &Value) -> Frame {
        let t = n["translation"].as_array().map(|a| Vec3::new(
            a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap()))
            .unwrap_or(Vec3::ZERO);
        let (x, y, z, w) = n["rotation"].as_array().map(|a| (
            a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap(), a[3].as_f64().unwrap()))
            .unwrap_or((0.0, 0.0, 0.0, 1.0));
        Frame::new(Mat3([
            [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
            [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
            [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
        ]), t)
    }

    /// World matrix of every node, by walking the scene graph from the roots.
    fn world_frames(g: &Gltf) -> Vec<Frame> {
        let nodes = g.json["nodes"].as_array().unwrap();
        let mut out = vec![None; nodes.len()];
        let mut stack: Vec<(usize, Frame)> = g.json["scenes"][0]["nodes"].as_array().unwrap()
            .iter().map(|v| (v.as_u64().unwrap() as usize, Frame::IDENTITY)).collect();
        while let Some((i, parent)) = stack.pop() {
            assert!(out[i].is_none(), "node {i} reached twice: not a tree");
            let w = parent.compose(&local_frame(&nodes[i]));
            out[i] = Some(w);
            for c in nodes[i]["children"].as_array().into_iter().flatten() {
                stack.push((c.as_u64().unwrap() as usize, w));
            }
        }
        out.into_iter().enumerate().map(|(i, f)| f.unwrap_or_else(|| panic!("node {i} unreachable"))).collect()
    }

    fn close(a: Vec3, b: Vec3, tol: f64) -> bool { a.sub(b).length() <= tol }

    fn parent_of(g: &Gltf, name: &str) -> Option<String> {
        let i = node_index(g, name);
        g.json["nodes"].as_array().unwrap().iter()
            .find(|n| n["children"].as_array().is_some_and(|c| c.iter().any(|c| c == i)))
            .map(|n| n["name"].as_str().unwrap().to_string())
    }

    /// Test 2 — tree. One node per part (+ one per printed thing); the
    /// object of a mate is the parent; a mirror image hangs where its source
    /// hangs, or from the image of its source's parent.
    #[test]
    fn tree_follows_relations() {
        let (scene, g) = doc(SKELETON);
        let parts = scene.layers[0].parts.len();
        assert_eq!(g.json["nodes"].as_array().unwrap().len(), parts + 1);
        world_frames(&g); // every node reachable exactly once

        assert_eq!(parent_of(&g, "Spine").as_deref(), Some("Skeleton"));
        assert_eq!(parent_of(&g, "Skull").as_deref(), Some("Neck"));
        assert_eq!(parent_of(&g, "RightArm.Humerus").as_deref(), Some("Ribcage"));
        assert_eq!(parent_of(&g, "LeftArm.Humerus"), parent_of(&g, "RightArm.Humerus"),
            "mirrored part's parent = source's parent");
        assert_eq!(parent_of(&g, "LeftArm.Forearm").as_deref(), Some("LeftArm.Humerus"));
        assert_eq!(parent_of(&g, "LeftLeg.Foot").as_deref(), Some("LeftLeg.Shin"));

        // the mirrored pivot is the mirror image of the source pivot (plane x=0)
        let w = world_frames(&g);
        let r = w[node_index(&g, "RightArm.Forearm")].pos;
        let l = w[node_index(&g, "LeftArm.Forearm")].pos;
        assert!(close(l, Vec3::new(-r.x, r.y, r.z), 1e-9), "{l:?} vs {r:?}");
    }

    /// Test 3 — pivot. Arm's node sits on the solved `Post.side(t=0.92,
    /// angle=0)` point; turning the Arm node by the lean difference about
    /// its own origin reproduces the script with that lean, Bulb included.
    #[test]
    fn pivots_sit_on_mate_frames() {
        let (scene, g) = doc(LAMP);
        let w = world_frames(&g);
        let post = scene.layers[0].parts.iter().find(|p| p.name == "Post").unwrap();
        let f = |k: &str, v: f64| NamedArg { key: k.into(), value: Expr::Float(v) };
        let socket = resolve_anchor(&post.shape.to_expr(), "side", &[f("t", 0.92), f("angle", 0.0)])
            .expect("side anchor");
        let expect = post.frame.to_frame().compose(&socket.frame).pos;
        let arm20 = w[node_index(&g, "Arm")];
        assert!(close(arm20.pos, expect, 1e-9), "{:?} vs {expect:?}", arm20.pos);

        let src30 = LAMP.replace("lean=(20, 0)", "lean=(30, 0)");
        assert_ne!(src30, LAMP);
        let (_, g30) = doc(&src30);
        let w30 = world_frames(&g30);
        let arm30 = w30[node_index(&g30, "Arm")];
        assert!(close(arm30.pos, arm20.pos, 1e-9), "lean moves the pivot");

        // D = the node-local turn taking lean 20 to lean 30: pure rotation, 10°
        let d = arm20.inverse().compose(&arm30);
        assert!(d.pos.length() < 1e-9);
        let angle = ((d.rot.0[0][0] + d.rot.0[1][1] + d.rot.0[2][2] - 1.0) / 2.0).clamp(-1.0, 1.0).acos();
        assert!((angle.to_degrees() - 10.0).abs() < 1e-6, "turn = {}°", angle.to_degrees());

        // apply D to the 20° file's Arm node: its subtree lands on the 30° file
        let bulb_local = arm20.inverse().compose(&w[node_index(&g, "Bulb")]);
        let turned = arm20.compose(&d).compose(&bulb_local);
        let bulb30 = w30[node_index(&g30, "Bulb")];
        assert!(close(turned.pos, bulb30.pos, 1e-9), "{:?} vs {:?}", turned.pos, bulb30.pos);
    }

    fn read_vec3s(g: &Gltf, accessor: usize) -> Vec<Vec3> {
        let a = &g.json["accessors"][accessor];
        let v = &g.json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let off = v["byteOffset"].as_u64().unwrap() as usize;
        let n = a["count"].as_u64().unwrap() as usize;
        (0..n).map(|i| {
            let c = |k: usize| f32::from_le_bytes(g.bin[off + 12 * i + 4 * k..][..4].try_into().unwrap()) as f64;
            Vec3::new(c(0), c(1), c(2))
        }).collect()
    }

    /// Test 4 — parity. Through the node chain, every part's vertices are
    /// exactly where `moxi mesh` puts them (to f32 storage precision).
    #[test]
    fn world_mesh_matches_moxi_mesh() {
        for src in [LAMP, SKELETON, AIM, FACING] {
            let (scene, g) = doc(src);
            let w = world_frames(&g);
            let layer = &scene.layers[0];
            for part in &layer.parts {
                let expr = part.shape.to_expr();
                let tri = surface_nets(&expr, auto_cell(&expr, layer.voxel_size), layer.voxel_size);
                let i = node_index(&g, &part.name);
                let Some(m) = g.json["nodes"][i]["mesh"].as_u64() else {
                    assert!(tri.indices.is_empty(), "{} lost its mesh", part.name);
                    continue;
                };
                let prim = &g.json["meshes"][m as usize]["primitives"][0];
                let got = read_vec3s(&g, prim["attributes"]["POSITION"].as_u64().unwrap() as usize);
                assert_eq!(got.len(), tri.positions.len(), "{}", part.name);
                let f = part.frame.to_frame();
                for (v, p) in got.iter().zip(&tri.positions) {
                    let want = f.apply_point(Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
                    let have = w[i].apply_point(*v);
                    let tol = 1e-6 + 2e-7 * (1.0 + want.length());
                    assert!(close(have, want, tol), "{}: {have:?} vs {want:?}", part.name);
                }
            }
        }
    }

    #[test]
    fn glb_container_is_well_formed() {
        let (_, g) = doc(LAMP);
        let b = g.to_glb();
        assert_eq!(&b[0..4], b"glTF");
        assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize, b.len());
        let jl = u32::from_le_bytes(b[12..16].try_into().unwrap()) as usize;
        assert_eq!(jl % 4, 0);
        assert_eq!(&b[16..20], b"JSON");
        let bl = u32::from_le_bytes(b[20 + jl..24 + jl].try_into().unwrap()) as usize;
        assert_eq!(&b[24 + jl..28 + jl], b"BIN\0");
        assert_eq!(28 + jl + bl, b.len());
        assert!(g.to_text().contains("data:application/octet-stream;base64,"));
    }

    // ── Animations (living models step 3) ──

    const LAMP_ANIM: &str = r##"
material Iron { color = "#333333" }
thing Lamp(height=300, reach=90) {
    part Base { shape = cylinder(height=12, radius=45), material = Iron }
    part Post { shape = cylinder(height=height, radius=6), material = Iron }
    part Arm  { shape = capsule(height=reach, radius=4), material = Iron }
    part Bulb { shape = sphere(radius=18), material = Iron }
    relation {
        Post.bottom on Base.top
        Arm.bottom  on Post.side(t=0.92, angle=0) lean=(20, 0)
        Bulb.center on Arm.top
    }
    pose Up   { Arm lean=(70, 0) }
    pose Down { Arm lean=(-40, 0) twist=30 }
    animate Wave loop { 0 rest  0.5 Up  1.0 Down  1.5 rest }
    resolve voxel_size = 3
}
print Lamp
"##;

    fn read_floats(g: &Gltf, accessor: usize, width: usize) -> Vec<Vec<f64>> {
        let a = &g.json["accessors"][accessor];
        let v = &g.json["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
        let off = v["byteOffset"].as_u64().unwrap() as usize;
        (0..a["count"].as_u64().unwrap() as usize).map(|i| (0..width).map(|k| {
            f32::from_le_bytes(g.bin[off + 4 * (width * i + k)..][..4].try_into().unwrap()) as f64
        }).collect()).collect()
    }

    /// One clip per `animate`; channels only for nodes that move (the Arm —
    /// the Bulb rides on it with a constant local transform); and every
    /// keyframe's value is exactly the node transform `moxi gltf --pose P`
    /// writes for that keyframe's pose.
    #[test]
    fn animation_keyframes_equal_the_posed_exports() {
        let (_, g) = doc(LAMP_ANIM);
        let anims = g.json["animations"].as_array().expect("animations");
        assert_eq!(anims.len(), 1);
        let a = &anims[0];
        assert_eq!(a["name"], "Wave");
        assert_eq!(a["extras"]["moxi"]["loop"], true);
        let arm = node_index(&g, "Arm");
        let targets: Vec<(u64, &str)> = a["channels"].as_array().unwrap().iter()
            .map(|c| (c["target"]["node"].as_u64().unwrap(), c["target"]["path"].as_str().unwrap())).collect();
        assert_eq!(targets, vec![(arm as u64, "translation"), (arm as u64, "rotation")]);

        let samplers = a["samplers"].as_array().unwrap();
        let times = read_floats(&g, samplers[0]["input"].as_u64().unwrap() as usize, 1);
        assert_eq!(times.iter().map(|t| t[0]).collect::<Vec<_>>(), vec![0.0, 0.5, 1.0, 1.5]);
        let tr = read_floats(&g, samplers[0]["output"].as_u64().unwrap() as usize, 3);
        let rot = read_floats(&g, samplers[1]["output"].as_u64().unwrap() as usize, 4);
        for (k, pose) in ["rest", "Up", "Down", "rest"].iter().enumerate() {
            let posed = scene_to_gltf(&crate::pipeline::compile_to_scene_posed(LAMP_ANIM, Some(pose)).unwrap(), None);
            let n = &posed.json["nodes"][node_index(&posed, "Arm")];
            for c in 0..3 {
                assert!((tr[k][c] - n["translation"][c].as_f64().unwrap()).abs() < 1e-3, "{pose} t");
            }
            let q: Vec<f64> = (0..4).map(|c| n["rotation"][c].as_f64().unwrap()).collect();
            let dot: f64 = (0..4).map(|c| q[c] * rot[k][c]).sum();
            assert!((dot.abs() - 1.0).abs() < 1e-6, "{pose} rotation (q and -q are the same turn)");
        }
        // consecutive quaternions share a hemisphere: each segment slerps the short way
        for w in rot.windows(2) {
            assert!((0..4).map(|c| w[0][c] * w[1][c]).sum::<f64>() >= 0.0);
        }
    }

    // ── Revolve: continuous rotation ──

    const SPINNER: &str = r##"
material Iron { color = "#333333" }
thing S {
    part Plate { shape = box(width=40, height=2, depth=20), material = Iron }
    part A { shape = box(width=10, height=2, depth=2), material = Iron }
    part B { shape = box(width=10, height=2, depth=2), material = Iron }
    relation {
        A.bottom on Plate.top shift=(-8, 0) twist=10
        B.bottom on Plate.top shift=(8, 0)
    }
    revolve Turn { A twist=90  B twist=-45 }
    resolve voxel_size = 1
}
print S
"##;

    /// A revolve is one looping clip: period 8 s for 90/-45 °/s, a key every
    /// second (A turns 90° per key, never more), the last rotation equal to
    /// the first (whole turns), and every key equal to the export of the
    /// script with `twist = rest + rate·t` written into the mate.
    #[test]
    fn revolve_exports_a_seamless_looping_clip() {
        let (_, g) = doc(SPINNER);
        let anims = g.json["animations"].as_array().expect("animations");
        assert_eq!(anims.len(), 1);
        let a = &anims[0];
        assert_eq!(a["name"], "Turn");
        assert_eq!(a["extras"]["moxi"]["loop"], true);
        let node_a = node_index(&g, "A") as u64;
        let ch = a["channels"].as_array().unwrap();
        let rot_sampler = ch.iter().find(|c| c["target"]["node"] == node_a && c["target"]["path"] == "rotation")
            .expect("A has a rotation channel")["sampler"].as_u64().unwrap() as usize;
        let sampler = &a["samplers"][rot_sampler];
        let times = read_floats(&g, sampler["input"].as_u64().unwrap() as usize, 1);
        assert_eq!(times.iter().map(|t| t[0]).collect::<Vec<_>>(), (0..=8).map(|k| k as f64).collect::<Vec<_>>());
        let rot = read_floats(&g, sampler["output"].as_u64().unwrap() as usize, 4);
        let dot = |p: &[f64], q: &[f64]| (0..4).map(|c| p[c] * q[c]).sum::<f64>();
        assert!((dot(&rot[0], &rot[8]).abs() - 1.0).abs() < 1e-6, "seamless: last key is the first turn");
        for w in rot.windows(2) {
            // a 90° turn between keys: |q0·q1| = cos(45°)
            assert!((dot(&w[0], &w[1]) - (45f64).to_radians().cos()).abs() < 1e-4, "short way, 90° steps");
        }
        // pose = substitution, for revolves: key k is the script with A twist = 10 + 90·k
        for k in [1usize, 3, 6] {
            let written = SPINNER.replace("twist=10\n", &format!("twist={}\n", 10.0 + 90.0 * k as f64))
                .replace("    revolve Turn { A twist=90  B twist=-45 }\n", "");
            let posed = scene_to_gltf(&crate::pipeline::compile_to_scene(&written).unwrap(), None);
            let n = &posed.json["nodes"][node_index(&posed, "A")];
            let q: Vec<f64> = (0..4).map(|c| n["rotation"][c].as_f64().unwrap()).collect();
            assert!((dot(&q, &rot[k]).abs() - 1.0).abs() < 1e-5, "key {k} equals the written twist");
        }
    }

    /// Every key is solved and checked like a pose: a revolve that takes a
    /// part through a constraint fails, and the error says when.
    #[test]
    fn a_revolve_that_breaks_a_constraint_names_the_time() {
        let src = SPINNER.replace("revolve Turn { A twist=90  B twist=-45 }",
            "revolve Turn { A twist=90  B pitch=90 }\n    constraint B above Plate");
        let errs = crate::pipeline::compile_to_scene(&src).expect_err("B swings under the plate");
        let m = errs.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n");
        assert!(m.contains("in revolve 'Turn' at") && m.contains("'B' above 'Plate'"), "{m}");
    }

    #[test]
    fn no_animation_no_animations_key() {
        let (_, g) = doc(LAMP);
        assert!(g.json.get("animations").is_none());
    }

    // ── See-through materials (ISSUE-20261004-see-through) ──

    const CANDLE: &str = r##"
material Wick  { color = "#222222" }
material Flame { color = "#ff7a22", emissive = 1.6, opacity = 0.5, blend = additive }
thing Candle {
    part Stick { shape = cylinder(height=10, radius=2), material = Wick }
    part Fire  { shape = cone(height=6, radius=1.5), material = Flame }
    relation { Fire.bottom on Stick.top }
    state Out { Flame.opacity = 0.1 }
    resolve voxel_size = 1
}
print Candle
"##;

    #[test]
    fn see_through_materials_export_alpha_and_additive_extras() {
        let (_, g) = doc(CANDLE);
        let mats = g.json["materials"].as_array().unwrap();
        let prim = |n: &str| g.json["meshes"][g.json["nodes"][node_index(&g, n)]["mesh"].as_u64().unwrap() as usize]["primitives"][0].clone();
        let fire = &mats[prim("Fire")["material"].as_u64().unwrap() as usize];
        assert_eq!(fire["alphaMode"], "BLEND");
        assert_eq!(fire["pbrMetallicRoughness"]["baseColorFactor"][3], 0.5);
        assert_eq!(fire["extras"]["moxi"]["blend"], "additive");
        let stick = &mats[prim("Stick")["material"].as_u64().unwrap() as usize];
        assert!(stick.get("alphaMode").is_none() && stick.get("extras").is_none(), "opaque stays opaque");
        assert_eq!(stick["pbrMetallicRoughness"]["baseColorFactor"][3], 1.0);
        // the Out variant: the same material, fainter
        let maps = prim("Fire")["extensions"]["KHR_materials_variants"]["mappings"].as_array().unwrap().clone();
        let out = maps.iter().find(|m| m["variants"].as_array().unwrap().iter().any(|v| v == 1)).unwrap();
        let out = &mats[out["material"].as_u64().unwrap() as usize];
        assert_eq!(out["pbrMetallicRoughness"]["baseColorFactor"][3], 0.1);
        assert_eq!(out["extras"]["moxi"]["blend"], "additive");
    }

    // ── Emissive and states (living models step 4) ──

    const GLOW: &str = r##"
material Iron { color = "#333333" }
material Glow { color = "#fff2b0", emissive = 1.5 }
material Dim  { color = "#808080", emissive = 0.5 }
thing Lamp {
    part Base { shape = cylinder(height=4, radius=10), material = Iron }
    part Bulb { shape = sphere(radius=4), material = Glow }
    relation { Bulb.bottom on Base.top }
    state Off  { Glow.emissive = 0 }
    state Half { Bulb material = Dim }
    resolve voxel_size = 1
}
print Lamp
"##;

    #[test]
    fn emissive_and_states_export_as_khr_extensions() {
        let (_, g) = doc(GLOW);
        let used: Vec<&str> = g.json["extensionsUsed"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(used, vec!["KHR_materials_emissive_strength", "KHR_materials_variants"]);
        let variants: Vec<&str> = g.json["extensions"]["KHR_materials_variants"]["variants"].as_array().unwrap()
            .iter().map(|v| v["name"].as_str().unwrap()).collect();
        assert_eq!(variants, vec!["rest", "Off", "Half"]);

        let mats = g.json["materials"].as_array().unwrap();
        let bulb = &g.json["meshes"][g.json["nodes"][node_index(&g, "Bulb")]["mesh"].as_u64().unwrap() as usize]["primitives"][0];
        let rest = &mats[bulb["material"].as_u64().unwrap() as usize];
        assert_eq!(rest["name"], "Glow");
        assert_eq!(rest["extensions"]["KHR_materials_emissive_strength"]["emissiveStrength"], 1.5);
        let base = rest["pbrMetallicRoughness"]["baseColorFactor"].clone();
        assert_eq!(rest["emissiveFactor"][0], base[0], "glows in its own colour (factor capped at 1)");

        let maps = bulb["extensions"]["KHR_materials_variants"]["mappings"].as_array().unwrap();
        let by_variant = |vi: u64| maps.iter().find(|m| m["variants"].as_array().unwrap().iter().any(|v| v == vi))
            .map(|m| &mats[m["material"].as_u64().unwrap() as usize]).unwrap();
        assert_eq!(by_variant(0)["name"], "Glow", "rest maps to the default look explicitly");
        assert!(by_variant(1).get("emissiveFactor").is_none(), "Off: no glow");
        let half = by_variant(2);
        assert_eq!(half["name"], "Dim");
        assert!(half.get("extensions").is_none(), "emissive ≤ 1 needs no strength extension");
        assert!((half["emissiveFactor"][0].as_f64().unwrap() - 0.5 * half["pbrMetallicRoughness"]["baseColorFactor"][0].as_f64().unwrap()).abs() < 1e-12);

        // the base never changes in any state: no mappings on it
        let base_prim = &g.json["meshes"][g.json["nodes"][node_index(&g, "Base")]["mesh"].as_u64().unwrap() as usize]["primitives"][0];
        assert!(base_prim.get("extensions").is_none());
    }
}

