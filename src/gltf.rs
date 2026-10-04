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
//! Node extras carry `{"moxi":{"part","parent","thing"}}`.

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
    let mut mat_index: HashMap<(String, String), usize> = HashMap::new();
    let mut bin: Vec<u8> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();

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
                let pa = push_accessor(&mut bin, &mut views, &mut accessors, f32s(&pos), ARRAY_BUFFER,
                    json!({ "componentType": FLOAT, "count": pos.len(), "type": "VEC3", "min": mn, "max": mx }));
                let na = push_accessor(&mut bin, &mut views, &mut accessors, f32s(&nor), ARRAY_BUFFER,
                    json!({ "componentType": FLOAT, "count": nor.len(), "type": "VEC3" }));
                let ia = push_accessor(&mut bin, &mut views, &mut accessors,
                    tri.indices.iter().flat_map(|i| i.to_le_bytes()).collect(), ELEMENT_ARRAY_BUFFER,
                    json!({ "componentType": UNSIGNED_INT, "count": tri.indices.len(), "type": "SCALAR" }));

                let key = (part.material.clone().unwrap_or_default(), part.color.clone());
                let m = *mat_index.entry(key.clone()).or_insert_with(|| {
                    let name = if key.0.is_empty() { key.1.clone() } else { key.0.clone() };
                    materials.push(json!({
                        "name": name,
                        "pbrMetallicRoughness": {
                            "baseColorFactor": linear_rgba(&key.1),
                            "metallicFactor": 0.0,
                            "roughnessFactor": 0.9,
                        },
                    }));
                    materials.len() - 1
                });

                node["mesh"] = json!(meshes.len());
                meshes.push(json!({
                    "name": part.name,
                    "primitives": [{ "attributes": { "POSITION": pa, "NORMAL": na }, "indices": ia, "material": m }],
                }));
            }
            nodes.push(node);
        }
        for (parent, kids) in children {
            nodes[parent]["children"] = json!(kids);
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
    bytes: Vec<u8>, target: u32, mut accessor: Value,
) -> usize {
    while !bin.len().is_multiple_of(4) { bin.push(0); }
    views.push(json!({ "buffer": 0, "byteOffset": bin.len(), "byteLength": bytes.len(), "target": target }));
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
        for src in [LAMP, SKELETON] {
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
}
