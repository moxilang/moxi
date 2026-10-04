// src/pipeline.rs
//
// The embeddable compilation surface — the single entry point for every
// target: CLI, server, and WebAssembly. NOTHING here prints or exits:
// errors are data. That is what makes the LLM loop work — a model that
// emits bad Moxi gets structured {stage, message, line, col} back and can
// repair its own output.
//
//   compile_source(src)   -> Result<WorldOutput, Vec<CompileError>>   voxels
//   compile_to_scene(src) -> Result<Scene, Vec<CompileError>>         the IR
//   compile_to_json(src)  -> String   // {"ok":true,…} | {"ok":false,…}
//
// Both outputs come from one placement step, `place_world`: every printed
// thing's solved parts, plus — on the generator surface — every scattered
// instance expanded into ordinary parts. Voxels are one rendering of that
// list; the scene is the list itself.

use serde::Serialize;
use std::collections::HashSet;

use crate::ast::{ConstraintExpr, GeneratorDecl, ShapeExpr, TopLevel};
use crate::error::{MoxiError, Span};
use crate::frame::{Frame, Vec3};
use crate::frame_resolver::{check_relation_constraint, resolve_frames_full, PlacementError};
use crate::generator::{analytic_elevation_map, run_generators};
use crate::geometry::{self, rasterize_entity, CompiledEntity};
use crate::lexer::Lexer;
use crate::parser::Parser as MoxiParser;
use crate::resolver::{ResolvedEntity, ResolvedScene, Resolver};
use crate::types::{grid_to_scene, Voxel};

// ── Output types ───────────────────────────────────────────────────────────

/// One compile error, machine-readable. `stage` is one of:
/// "lex" | "parse" | "resolve" | "place" | "constraint".
#[derive(Debug, Clone, Serialize)]
pub struct CompileError {
    pub stage:   String,
    pub message: String,
    pub line:    Option<usize>,
    pub col:     Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayerInfo {
    pub name:   String,
    pub dims:   [u32; 3],
    pub voxels: usize,
    /// Solved parts in this layer, including generator-scattered instances
    /// (named "<generator>.<index>.<part>").
    pub parts:  usize,
}

/// The structured 3D representation the web renderer consumes.
#[derive(Debug, Clone, Serialize)]
pub struct WorldOutput {
    pub voxels: Vec<Voxel>,
    pub layers: Vec<LayerInfo>,
    pub total:  usize,
    /// Inclusive AABB of all voxels: [min_xyz, max_xyz]. Zeroes if empty.
    pub bounds: [[i32; 3]; 2],
}

// ── Error plumbing ─────────────────────────────────────────────────────────

fn err(stage: &str, message: String, span: Option<Span>) -> CompileError {
    CompileError {
        stage:   stage.to_string(),
        message,
        line:    span.map(|s| s.line),
        col:     span.map(|s| s.col),
    }
}

fn span_of(e: &MoxiError) -> Option<Span> {
    match e {
        MoxiError::UnexpectedChar { span, .. }
        | MoxiError::UnterminatedString { span }
        | MoxiError::UnexpectedToken { span, .. }
        | MoxiError::UndefinedName { span, .. }
        | MoxiError::DuplicateName { span, .. }
        | MoxiError::UndefinedMaterial { span, .. }
        | MoxiError::UndefinedAtom { span, .. }
        | MoxiError::UndefinedAnchor { span, .. }
        | MoxiError::BadAnchor { span, .. }
        | MoxiError::InstanceError { span, .. }
        | MoxiError::ExprError { span, .. }
        | MoxiError::FnError { span, .. }
        | MoxiError::PoseError { span, .. } => Some(*span),
        MoxiError::UnexpectedEof { .. }
        | MoxiError::ConstraintViolation { .. } => None,
    }
}

fn span_of_placement(e: &PlacementError) -> Option<Span> {
    match e {
        PlacementError::OverConstrained { second, .. } => Some(*second),
        PlacementError::UnknownPart { span, .. }
        | PlacementError::Anchor { span, .. }
        | PlacementError::ConstraintViolation { span, .. } => Some(*span),
        PlacementError::CyclicPlacement { .. }
        | PlacementError::NonAxisAlignedRotation { .. } => None,
    }
}

// ── Front end (shared by every output) ────────────────────────────────

/// Lex + parse + resolve. All diagnostics are collected — the LLM gets
/// every problem in one round trip, not one at a time.
///
/// S1: compile only what is inside ```moxi fences. Masking preserves line
/// numbers exactly, so spans stay absolute to the user's file. Files with
/// no fence are passed through unchanged (legacy `#`/`>` rules).
fn front_end(source: &str) -> Result<(ResolvedScene, Vec<GeneratorDecl>), Vec<CompileError>> {
    let (masked, fence_errors) = crate::lexer::fence::preprocess(source);
    let (tokens, mut lex_errors) = Lexer::new(&masked).tokenize();
    lex_errors.extend(fence_errors);
    let (doc, parse_errors)  = MoxiParser::new(tokens).parse();

    let generators: Vec<GeneratorDecl> = doc.items.iter().filter_map(|item| {
        if let TopLevel::GeneratorDecl(g) = item { Some(g.clone()) } else { None }
    }).collect();

    let (resolved, resolve_errors) = Resolver::new().resolve(doc);

    let mut errors: Vec<CompileError> = Vec::new();
    errors.extend(lex_errors.iter().map(|e| err("lex", e.to_string(), span_of(e))));
    errors.extend(parse_errors.iter().map(|e| err("parse", e.to_string(), span_of(e))));
    errors.extend(resolve_errors.iter().map(|e| err("resolve", e.to_string(), span_of(e))));
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok((resolved, generators))
}

// ── Placement (shared by every output) ────────────────────────────────

/// One printed thing, fully placed: its own solved parts plus any
/// scattered instances, all in the thing's world space.
pub struct PlacedLayer {
    pub thing:           String,
    pub voxel_size:      f64,
    pub has_heightfield: bool,
    pub parts:           Vec<(String, ShapeExpr, u16, Frame)>,
    /// The joint tree (crate::joints), keyed by the same names as `parts`.
    pub joints:          std::collections::HashMap<String, crate::joints::Joint>,
}

/// Solve every printed thing and expand generator scatter into parts.
///
/// Generators run over the FIRST printed thing containing a heightfield
/// — the world, in a world-as-thing script. Templates and generator
/// targets are never layers. Each scattered instance becomes the target's
/// parts under a translation, named `<generator>.<index>.<part>`, so it
/// is an ordinary part of the layer everywhere downstream: voxel
/// rasterization overwrites in order (scatter is appended, so trees win),
/// the scene lists them, the viewer draws them.
fn place_world(
    resolved:   &ResolvedScene,
    compiled:   &[CompiledEntity],
    generators: &[GeneratorDecl],
    pose:       Option<&str>,
) -> Result<Vec<PlacedLayer>, Vec<CompileError>> {
    let generator_targets: HashSet<&str> = generators
        .iter().map(|g| g.scatter_target.name.as_str()).collect();

    let mut layers = Vec::new();
    let mut scattered = false;
    let mut pose_found = pose.is_none() || pose == Some("rest");

    for (ent, resolved_ent) in compiled.iter().zip(resolved.entities.iter()) {
        if generator_targets.contains(ent.name.as_str()) { continue; }
        if resolved.instanced.contains(ent.name.as_str()) { continue; }

        let vs = ent.voxel_size;
        let has_heightfield = resolved_ent.parts.iter()
            .any(|p| matches!(&p.shape, Some(ShapeExpr::Heightfield { .. })));

        let this_pose = pose.and_then(|n| resolved_ent.poses.iter().find(|p| p.name == n));
        pose_found |= this_pose.is_some();
        let (mut parts, mut joints) = solved_parts(resolved_ent, ent, vs, this_pose)?;

        if has_heightfield && !scattered && !generators.is_empty() {
            scattered = true;
            let elevation = analytic_elevation_map(&parts, vs);

            for placement in run_generators(&elevation, generators) {
                let Some(target_ent) = compiled.iter()
                    .find(|e| e.name == placement.target_name) else { continue };
                let Some(target_res) = resolved.entities.iter()
                    .find(|e| e.name == target_ent.name) else { continue };

                // Stand on the surface: one voxel above the top voxel-centre,
                // the same convention the voxel path has always used.
                let at = Frame::from_pos(Vec3::new(
                    placement.x as f64 * vs,
                    (placement.y as f64 + 1.0) * vs,
                    placement.z as f64 * vs,
                ));
                let prefix = format!("{}.{}", placement.generator_name, placement.index);

                let (target_parts, target_joints) =
                    solved_parts(target_res, target_ent, target_ent.voxel_size, None)?;
                for (name, shape, atom, frame) in target_parts {
                    parts.push((format!("{prefix}.{name}"), shape, atom, at.compose(&frame)));
                }
                // an instance is its own little tree under the layer, so its
                // root stays a root (parent None), its parts keep their parents
                for (name, j) in target_joints {
                    joints.insert(format!("{prefix}.{name}"), crate::joints::Joint {
                        parent: j.parent.map(|p| format!("{prefix}.{p}")),
                        frame:  at.compose(&j.frame),
                    });
                }
            }
        }

        layers.push(PlacedLayer { thing: ent.name.clone(), voxel_size: vs, has_heightfield, parts, joints });
    }

    if !pose_found {
        let printed: HashSet<&str> = layers.iter().map(|l| l.thing.as_str()).collect();
        let listed: Vec<String> = resolved.entities.iter()
            .filter(|e| printed.contains(e.name.as_str()) && !e.poses.is_empty())
            .map(|e| format!("{}: {}", e.name,
                e.poses.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")))
            .collect();
        let have = if listed.is_empty() {
            "no printed thing declares a pose".to_string()
        } else {
            format!("poses: {}", listed.join("; "))
        };
        return Err(vec![err("pose", format!(
            "pose '{}': no printed thing has this pose — {have} (or `rest`)", pose.unwrap_or("")), None)]);
    }

    Ok(layers)
}

type SolvedParts = (Vec<(String, ShapeExpr, u16, Frame)>, std::collections::HashMap<String, crate::joints::Joint>);

/// Solve one thing's frames, gate on constraints, pair atom ids, and read
/// off the joint tree the solve implies — in `pose` (None = rest).
///
/// Every named pose is solved and its constraints checked whichever one is
/// printed (DOC-20261004-living-models-design §3.5): a pose that cannot
/// stand is a compile error of the script, not of the one command that
/// happens to print it.
fn solved_parts(
    resolved_ent: &ResolvedEntity,
    compiled_ent: &CompiledEntity,
    voxel_size:   f64,
    pose:         Option<&crate::resolver::ResolvedPose>,
) -> Result<SolvedParts, Vec<CompileError>> {
    use crate::resolver::apply_pose;

    let raw: Vec<(String, ShapeExpr)> = resolved_ent.parts.iter()
        .filter_map(|p| p.shape.clone().map(|s| (p.name.clone(), s)))
        .collect();

    let relations = match pose {
        Some(p) => apply_pose(&resolved_ent.relations, p),
        None    => resolved_ent.relations.clone(),
    };
    let (parts, frames, joints) =
        solve_checked(&raw, &relations, resolved_ent, voxel_size, pose.map(|p| p.name.as_str()))?;

    if pose.is_some() {
        solve_checked(&raw, &resolved_ent.relations, resolved_ent, voxel_size, None)?;
    }
    for p in &resolved_ent.poses {
        if pose.is_some_and(|q| q.name == p.name) { continue; }
        solve_checked(&raw, &apply_pose(&resolved_ent.relations, p), resolved_ent, voxel_size, Some(&p.name))?;
    }

    let atom_of: std::collections::HashMap<&str, u16> = compiled_ent.parts.iter()
        .map(|cp| (cp.name.as_str(), cp.atom_id))
        .collect();

    let solved = parts.into_iter().map(|(name, shape)| {
        let frame = frames[&name];
        let atom  = atom_of.get(name.as_str()).copied().unwrap_or(1);
        (name, shape, atom, frame)
    }).collect();
    Ok((solved, joints))
}

type Solved = (
    Vec<(String, ShapeExpr)>,
    crate::frame_resolver::FrameMap,
    std::collections::HashMap<String, crate::joints::Joint>,
);

/// One solve of `relations` over the unwrapped shapes: frames, mirror
/// wrappers, joints, constraints. Errors name the pose when there is one.
fn solve_checked(
    raw:          &[(String, ShapeExpr)],
    relations:    &[crate::ast::Placement],
    resolved_ent: &ResolvedEntity,
    voxel_size:   f64,
    pose:         Option<&str>,
) -> Result<Solved, Vec<CompileError>> {
    let tag = |m: String| match pose {
        Some(p) => format!("in pose '{p}': {m}"),
        None    => m,
    };
    let (frames, reflects) = resolve_frames_full(raw, relations).map_err(|errs| {
        errs.iter()
            .map(|e| err("place", tag(e.to_string()), span_of_placement(e)))
            .collect::<Vec<_>>()
    })?;

    // Joints read subject anchors off the UNWRAPPED shapes, as the solver did.
    let joints = crate::joints::joint_tree(raw, relations, &frames, &reflects);

    // A part placed by `symmetric_across` gets its shape wrapped in the
    // local reflection the solver returned, so its GEOMETRY is the mirror
    // image, not a copy in a mirrored place. Done before constraints, so
    // they measure the real shape; every backend reads the result.
    let mut parts = raw.to_vec();
    for (name, shape) in parts.iter_mut() {
        if let Some(n) = reflects.get(name.as_str()) {
            *shape = geometry::mirror_shape(shape.clone(), *n);
        }
    }

    let shape_of: std::collections::HashMap<&str, &ShapeExpr> =
        parts.iter().map(|(n, s)| (n.as_str(), s)).collect();
    for con in &resolved_ent.constraints {
        if let ConstraintExpr::Relation(rel) = &con.expr {
            if let Err(e) = check_relation_constraint(rel, &shape_of, &frames, voxel_size / 2.0) {
                return Err(vec![err("constraint", tag(e.to_string()), span_of_placement(&e))]);
            }
        }
    }
    Ok((parts, frames, joints))
}

// ── Voxel backend ──────────────────────────────────────────────────────

pub fn compile_source(source: &str) -> Result<WorldOutput, Vec<CompileError>> {
    let (resolved, generators) = front_end(source)?;
    let compiled = geometry::compile(&resolved, 1.0);
    let placed   = place_world(&resolved, &compiled, &generators, None)?;

    let mut all_voxels: Vec<Voxel> = Vec::new();
    let mut layers: Vec<LayerInfo> = Vec::new();

    for layer in &placed {
        let grid = rasterize_entity(&layer.parts, layer.voxel_size);
        let (gw, gh, gd) = grid.dims();
        layers.push(LayerInfo {
            name: layer.thing.clone(), dims: [gw, gh, gd], voxels: grid.filled_count(),
            parts: layer.parts.len(),
        });

        // The grid's (0,0,0) is the world minimum of the layer's AABB, so
        // shifting by that minimum puts every voxel back at its solved
        // world position. Layers are no longer centered or sunk by
        // convention: a scene is a thing whose parts are placed by
        // relation, and those relations are the only thing that decides
        // where anything sits.
        let origin = grid_origin(&layer.parts, layer.voxel_size);

        all_voxels.extend(grid_to_scene(&grid, &resolved.atoms, origin).voxels);
    }

    let bounds = bounds_of(&all_voxels);
    Ok(WorldOutput {
        total: all_voxels.len(),
        voxels: all_voxels,
        layers,
        bounds,
    })
}

/// The world voxel coordinate of a rasterized grid's (0,0,0) — the
/// minimum corner of the union AABB, computed the same way
/// `rasterize_entity` computes it, including the two voxels of padding.
/// Keep this in step with `geometry::vox_aabb`.
fn grid_origin(parts: &[(String, ShapeExpr, u16, Frame)], vs: f64) -> (i32, i32, i32) {
    use crate::anchors::analytic_extents;

    let mut min = (i32::MAX, i32::MAX, i32::MAX);
    for (_, shape, _, frame) in parts {
        let e = analytic_extents(shape);
        let mut lo = Vec3::new(f64::MAX, f64::MAX, f64::MAX);
        for &cx in &[e.min.x, e.max.x] {
            for &cy in &[e.min.y, e.max.y] {
                for &cz in &[e.min.z, e.max.z] {
                    let p = frame.apply_point(Vec3::new(cx, cy, cz));
                    lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                }
            }
        }
        min = (
            min.0.min((lo.x / vs).floor() as i32 - 2),
            min.1.min((lo.y / vs).floor() as i32 - 2),
            min.2.min((lo.z / vs).floor() as i32 - 2),
        );
    }

    if min.0 == i32::MAX { (0, 0, 0) } else { min }
}

fn bounds_of(voxels: &[Voxel]) -> [[i32; 3]; 2] {
    if voxels.is_empty() {
        return [[0, 0, 0], [0, 0, 0]];
    }
    let mut min = [i32::MAX; 3];
    let mut max = [i32::MIN; 3];
    for v in voxels {
        for (i, c) in [v.x, v.y, v.z].into_iter().enumerate() {
            if c < min[i] { min[i] = c; }
            if c > max[i] { max[i] = c; }
        }
    }
    [min, max]
}

// ── Scene surface (the canonical IR) ──────────────────────────────────

/// Compile to the solved scene: every printed thing's parts — scattered
/// instances included — with folded shapes, world frames, and resolved
/// colors, and NO voxels. `compile_source` is one rendering of this.
pub fn compile_to_scene(source: &str) -> Result<crate::scene::Scene, Vec<CompileError>> {
    compile_to_scene_posed(source, None)
}

/// The solved scene in a named pose (`None` or `Some("rest")` = the
/// script as written). A pose applies to every printed thing that declares
/// it; things without it stay at rest. Naming a pose no printed thing has
/// is an error listing the poses that exist.
pub fn compile_to_scene_posed(source: &str, pose: Option<&str>) -> Result<crate::scene::Scene, Vec<CompileError>> {
    use crate::colors::resolve_color;
    use crate::scene::{FrameOut, JointOut, Layer, Part, Scene, Shape, SCHEMA};

    let (resolved, generators) = front_end(source)?;
    let compiled = geometry::compile(&resolved, 1.0);
    let placed   = place_world(&resolved, &compiled, &generators, pose)?;

    let layers = placed.into_iter().map(|layer| {
        let joints = layer.joints;
        Layer {
            thing:      layer.thing,
            voxel_size: layer.voxel_size,
            parts: layer.parts.into_iter().map(|(name, shape, atom_id, frame)| {
                let atom = resolved.atoms.get(atom_id.saturating_sub(1) as usize);
                let joint = joints.get(&name).map(|j| JointOut {
                    parent: j.parent.clone(),
                    frame:  FrameOut::from_frame(&j.frame),
                });
                Part {
                    shape: Shape::from_expr(&shape),
                    frame: FrameOut::from_frame(&frame),
                    color: atom.map(|a| resolve_color(&a.color))
                        .unwrap_or_else(|| "#ff00ff".to_string()),
                    material: atom.map(|a| a.name.clone()).filter(|n| !n.is_empty()),
                    joint,
                    name,
                }
            }).collect(),
        }
    }).collect();

    Ok(Scene { version: env!("CARGO_PKG_VERSION").to_string(), schema: SCHEMA, layers })
}

// ── JSON surface (the wire format for web + WASM) ─────────────────────

/// Compile and serialize in one call — the single function every host
/// binds: CLI `moxi json`, a server endpoint, or the WASM export.
pub fn compile_to_json(source: &str) -> String {
    match compile_source(source) {
        Ok(world) => serde_json::json!({
            "ok":     true,
            "total":  world.total,
            "bounds": world.bounds,
            "layers": world.layers,
            "voxels": world.voxels,
        }).to_string(),
        Err(errors) => serde_json::json!({
            "ok":     false,
            "errors": errors,
        }).to_string(),
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
atom TRUNK { color = brown }
atom LEAF  { color = green }
material Bark  { color = brown, voxel_atom = TRUNK }
material Leafy { color = green, voxel_atom = LEAF }

entity PalmTree {
    part Trunk { shape = cylinder(height=6, radius=0.6), material = Bark }
    part Crown { shape = blob(radius=3, roughness=0.4), material = Leafy }
    relation {
        Crown.bottom on Trunk.top gap=-1
    }
    resolve voxel_size = 1.0
}

entity Grove {
    part Left   { entity = PalmTree }
    part Right  { entity = PalmTree }
    relation {
        Right.west on Left.east gap=1
    }
    resolve voxel_size = 1.0
}

print Grove detail=low
"#;

    #[test]
    fn good_source_compiles_to_a_world() {
        let world = compile_source(GOOD).expect("should compile");
        assert!(world.total > 0);
        assert_eq!(world.total, world.voxels.len());
        let colors: std::collections::HashSet<&str> =
            world.voxels.iter().map(|v| v.color.as_str()).collect();
        assert!(colors.len() >= 2, "expected trunk + leaf colors, got {colors:?}");
        assert_eq!(world.layers.len(), 1);
        assert_eq!(world.layers[0].name, "Grove");
        let json = compile_to_json(GOOD);
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"voxels\""));
    }

    #[test]
    fn bad_source_returns_structured_errors_not_exit() {
        let bad = GOOD.replace("Crown.bottom", "Crown.botom");
        let errs = compile_source(&bad).expect_err("should fail");
        assert!(errs.iter().any(|e|
            e.stage == "resolve" && e.line.is_some() && e.message.contains("botom")));

        let json = compile_to_json(&bad);
        assert!(json.contains("\"ok\":false"));
        assert!(json.contains("\"stage\""));

        let json = compile_to_json("entity { nope");
        assert!(json.contains("\"ok\":false"));
    }

    /// The parity claim for the atom/material unification, pinned as a
    /// test: the two-declaration form and the self-contained form produce
    /// voxel-identical output, colors included.
    #[test]
    fn self_contained_material_is_voxel_identical_to_the_atom_form() {
        const OLD: &str = r#"
atom BONE { color = ivory }
material Bone { color = ivory, voxel_atom = BONE }
entity E {
    part P { shape = sphere(radius=3), material = Bone }
    resolve voxel_size = 1.0
}
print E detail=low
"#;
        const NEW: &str = r#"
material Bone { color = ivory }
entity E {
    part P { shape = sphere(radius=3), material = Bone }
    resolve voxel_size = 1.0
}
print E detail=low
"#;
        let a = compile_source(OLD).expect("old form compiles");
        let b = compile_source(NEW).expect("new form compiles");

        assert_eq!(a.total, b.total, "voxel counts must match");
        assert_eq!(a.bounds, b.bounds, "bounds must match");
        for (va, vb) in a.voxels.iter().zip(b.voxels.iter()) {
            assert_eq!((va.x, va.y, va.z), (vb.x, vb.y, vb.z));
            assert_eq!(va.color, vb.color, "color must survive synthesis");
        }
    }

    /// The scene is a faithful IR: rebuilding every layer's parts from it
    /// and rasterizing gives the same voxel count the direct path reports.
    #[test]
    fn scene_round_trips_to_identical_voxels() {
        use crate::scene::Scene;
        use std::collections::HashMap;

        let world = compile_source(GOOD).expect("compiles");
        let scene = compile_to_scene(GOOD).expect("scene builds");
        assert_eq!(scene.layers.len(), world.layers.len());

        let scene = Scene::from_json(&scene.to_json_pretty()).unwrap();

        for (layer, info) in scene.layers.iter().zip(&world.layers) {
            assert_eq!(layer.thing, info.name);

            let mut atom_of: HashMap<&str, u16> = HashMap::new();
            let parts: Vec<(String, ShapeExpr, u16, Frame)> = layer.parts.iter().map(|p| {
                let next = atom_of.len() as u16 + 1;
                let id = *atom_of.entry(p.color.as_str()).or_insert(next);
                (p.name.clone(), p.shape.to_expr(), id, p.frame.to_frame())
            }).collect();

            let grid = rasterize_entity(&parts, layer.voxel_size);
            assert_eq!(grid.filled_count(), info.voxels,
                       "layer '{}' voxel count drifted through the scene", layer.thing);
        }
    }

    const WORLD: &str = r#"
material Water { color = blue }
material Grass { color = green }
material Wood  { color = brown }

thing Ocean   { part Disc   { shape = cylinder(height=2, radius=14), material = Water } resolve voxel_size = 1.0 }
thing Terrain {
    part Ground { shape = heightfield(radius=10, max_height=6, seed=3), material = Grass }
    anchor ground = Ground.surface
    resolve voxel_size = 1.0
}
thing Tree { part Trunk { shape = cylinder(height=3, radius=0.5), material = Wood } resolve voxel_size = 1.0 }

thing World {
    part Sea  { thing = Ocean }
    part Land { thing = Terrain }
    relation { Land.bottom on Sea.top }
    resolve voxel_size = 1.0
}

generator Forest {
    scatter Tree
    count = 6, min_spacing = 2, seed = 1
    where = elevation > 3
}

print World detail=low
"#;

    /// A scene is a thing: terrain instanced INSIDE the printed world is
    /// the generator's surface, and scattered trees stand on it — above
    /// the sea, not sunk to the template's origin.
    #[test]
    fn generators_scatter_over_the_printed_world() {
        let world = compile_source(WORLD).expect("world compiles");
        assert_eq!(world.layers.len(), 1, "one printed thing, one layer");
        assert_eq!(world.layers[0].name, "World");

        let sea_top = world.voxels.iter()
            .filter(|v| v.color == "#0000ff").map(|v| v.y).max().expect("sea voxels");
        let trees: Vec<i32> = world.voxels.iter()
            .filter(|v| v.color == "#8b4513").map(|v| v.y).collect();
        assert!(!trees.is_empty(), "the generator must place trees");
        let lowest_tree = *trees.iter().min().unwrap();
        assert!(lowest_tree > sea_top,
                "trees must stand on the terrain: tree y={lowest_tree}, sea top y={sea_top}");
    }

    /// Scattered instances are ordinary parts of the scene — the viewer
    /// draws them because they are simply there, named by generator.
    #[test]
    fn scattered_instances_appear_in_the_scene() {
        let scene = compile_to_scene(WORLD).expect("scene builds");
        let names: Vec<&str> = scene.layers[0].parts.iter().map(|p| p.name.as_str()).collect();
        let trees = names.iter().filter(|n| n.starts_with("Forest.")).count();
        assert!(trees > 0, "expected Forest.<i>.Trunk parts, got {names:?}");
        assert!(names.contains(&"Land.Ground"), "the world's own parts are still there");
    }

    /// Voxels sit where the solver put them. Layers are no longer centered
    /// or sunk by convention, so the voxel output and the scene agree on
    /// world position — the property that lets the viewer draw the scene
    /// directly and get the voxel picture back.
    #[test]
    fn voxels_land_at_their_solved_world_position() {
        const OFFSET: &str = r#"
material Stone { color = gray }

thing Pillar {
    part Base { shape = box(width=4, height=2, depth=4), material = Stone }
    part Top  { shape = box(width=2, height=6, depth=2), material = Stone }
    relation { Top.bottom on Base.top }
    resolve voxel_size = 1.0
}

print Pillar detail=low
"#;
        let world = compile_source(OFFSET).expect("compiles");
        let scene = compile_to_scene(OFFSET).expect("scene builds");

        // Base is centered on the origin, 2 tall: it spans y in [-1, 1].
        // Top stands on it and rises to y = 7. Previously the whole layer
        // was slid so its top sat at y = 0.
        let top_y = world.voxels.iter().map(|v| v.y).max().unwrap();
        let bot_y = world.voxels.iter().map(|v| v.y).min().unwrap();
        assert!(top_y > 0, "the pillar rises above the origin, got {top_y}");
        assert!(bot_y < 0, "the base straddles the origin, got {bot_y}");

        // And the scene agrees: the Top part's frame is where the voxels are.
        let top = scene.layers[0].parts.iter().find(|p| p.name == "Top").unwrap();
        assert!((top.frame.pos[1] - 4.0).abs() < 1e-9,
                "Top centre at y=4 (base top 1 + half of 6), got {}", top.frame.pos[1]);
    }

    /// Orientation is glTF's: +Y up, +Z front, +X right as seen from the
    /// front. Every keyword must agree with that single frame — the old
    /// table put `in_front_of` at -Z while `right_of` was +X, which no
    /// right-handed viewer can see as consistent.
    #[test]
    fn relation_keywords_agree_on_one_orientation() {
        const SRC: &str = r#"
material M { color = red }
thing T {
    part Core  { shape = box(width=2, height=2, depth=2), material = M }
    part Front { shape = sphere(radius=1), material = M }
    part Back  { shape = sphere(radius=1), material = M }
    part Right { shape = sphere(radius=1), material = M }
    part Left  { shape = sphere(radius=1), material = M }
    part Top   { shape = sphere(radius=1), material = M }
    relation {
        Front in_front_of Core
        Back  behind      Core
        Right right_of    Core
        Left  left_of     Core
        Top   above       Core
    }
    resolve voxel_size = 1.0
}
print T detail=low
"#;
        let scene = compile_to_scene(SRC).expect("compiles");
        let pos = |name: &str| scene.layers[0].parts.iter()
            .find(|p| p.name == name).unwrap_or_else(|| panic!("no part {name}")).frame.pos;
        assert!(pos("Front")[2] >  1.5, "in_front_of is +Z, the front");
        assert!(pos("Back")[2]  < -1.5, "behind is -Z");
        assert!(pos("Right")[0] >  1.5, "right_of is +X, right as seen from the front");
        assert!(pos("Left")[0]  < -1.5, "left_of is -X");
        assert!(pos("Top")[1]   >  1.5, "above is +Y");
    }

    #[test]
    fn symmetric_across_mirrors_geometry_not_just_placement() {
        // The bug this pins: a part offset to +X inside its own shape,
        // mirrored across a post at the origin, used to land ON its source.
        const SRC: &str = r#"
material M { color = red }
thing T {
    part Post { shape = cylinder(height=4, radius=0.5), material = M }
    part R    { shape = at(sphere(radius=1), x=3, y=2), material = M }
    part L    { shape = at(sphere(radius=1), x=3, y=2), material = M }
    relation {
        L symmetric_across Post from=R
    }
    resolve voxel_size = 1.0
}
print T detail=low
"#;
        let world = compile_source(SRC).expect("compiles");
        let east = world.voxels.iter().filter(|v| v.x >= 2).count();
        let west = world.voxels.iter().filter(|v| v.x <= -2).count();
        assert!(east > 0, "the source sits at +x");
        assert_eq!(east, west, "its mirror image sits at -x, voxel for voxel");
    }

    /// The north star: twelve rib pairs sized by a function, mirrored, on a
    /// spine with ten vertebrae — compiles, solves, rasterizes.
    #[test]
    fn the_ribcage_compiles_end_to_end() {
        const RIBCAGE: &str = r#"
material Bone { color = ivory }

fn taper(i, n) = sin(180 * (i + 0.5) / n)

thing Rib(reach=4, thick=0.3) {
    part Bone {
        shape = difference(
            torus(major_radius=reach, minor_radius=thick),
            at(box(width=2*reach + 2, height=2*thick + 2, depth=2*reach + 2*thick + 2), x=0 - reach - 1.3)
        ),
        material = Bone
    }
    anchor root = Bone.surface(angle=270, phi=0)
    resolve voxel_size = 1.0
}

thing Ribcage(pairs=12, verts=10) {
    part Spine { shape = capsule(height=20, radius=0.8), material = Bone }

    for i in 0..pairs {
        let t     = 0.3 + 0.55 * i / pairs
        let reach = 2.5 + 3.5 * taper(i, pairs)
        part RibR[i] { thing = Rib(reach=reach) }
        part RibL[i] { thing = Rib(reach=reach) }
        relation {
            RibR[i].root on Spine.side(t=t, angle=0)
            RibL[i] symmetric_across Spine from=RibR[i]
        }
    }

    for k in 0..verts {
        part Vert[k] { shape = box(width=1.6, height=1.1, depth=1.0, round=0.3), material = Bone }
        relation { Vert[k].south on Spine.side(t=0.05 + 0.9 * k / (verts - 1), angle=180) }
    }

    resolve voxel_size = 0.5
}

print Ribcage detail=low
"#;
        let world = compile_source(RIBCAGE).unwrap_or_else(|e| panic!("ribcage failed: {e:?}"));
        assert!(world.total > 0);
        assert_eq!(world.layers.len(), 1);
        assert_eq!(world.layers[0].parts, 1 + 24 + 10, "spine + 24 rib bones + 10 vertebrae");
    }
}