// src/pipeline.rs
//
// The embeddable compilation surface — the single entry point for every
// target: CLI, server, and WebAssembly. Unlike main.rs's original flow,
// NOTHING here prints or exits: errors are data. That is what makes the
// LLM loop work — a model that emits bad Moxi gets structured
// {stage, message, line, col} back and can repair its own output.
//
//   compile_source(src)  -> Result<WorldOutput, Vec<CompileError>>
//   compile_to_json(src) -> String   // {"ok":true,…} | {"ok":false,…}

use serde::Serialize;
use std::collections::HashSet;

use crate::ast::{ConstraintExpr, GeneratorDecl, ShapeExpr, TopLevel};
use crate::error::{MoxiError, Span};
use crate::frame::Frame;
use crate::frame_resolver::{check_relation_constraint, resolve_frames, PlacementError};
use crate::generator::run_generators;
use crate::geometry::{self, rasterize_entity, CompiledEntity};
use crate::lexer::Lexer;
use crate::parser::Parser as MoxiParser;
use crate::resolver::{ResolvedEntity, Resolver};
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
        | MoxiError::ExprError { span, .. } => Some(*span),
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

// ── Compilation ────────────────────────────────────────────────────────────

pub fn compile_source(source: &str) -> Result<WorldOutput, Vec<CompileError>> {
    // Front end: lex + parse + resolve. All diagnostics are collected —
    // the LLM gets every problem in one round trip, not one at a time.
    
    let (resolved, generators) = front_end(source)?;
    let compiled = geometry::compile(&resolved, 1.0);

    // ── World assembly (same layering rules as the viewer path) ───────────

    let mut all_voxels: Vec<Voxel> = Vec::new();
    let mut layers: Vec<LayerInfo> = Vec::new();

    let generator_targets: HashSet<&str> = generators
        .iter().map(|g| g.scatter_target.name.as_str()).collect();

    let primary_terrain_name = resolved.entities.iter().find(|e| {
        e.parts.iter().any(|p| matches!(&p.shape, Some(ShapeExpr::Heightfield { .. })))
    }).map(|e| e.name.as_str());

    let mut primary_terrain_grid = None;
    let mut terrain_center_offset: (i32, i32, i32) = (0, 0, 0);
    if let Some(pname) = primary_terrain_name {
        if let Some((ent, resolved_ent)) = compiled.iter()
            .zip(resolved.entities.iter())
            .find(|(e, _)| e.name.as_str() == pname)
        {
            let sp   = solved_parts(resolved_ent, ent, ent.voxel_size)?;
            let grid = rasterize_entity(&sp, ent.voxel_size);
            let (w, _, d) = grid.dims();
            terrain_center_offset = (-(w as i32 / 2), 0, -(d as i32 / 2));
            primary_terrain_grid = Some(grid);
        }
    }

    for (ent, resolved_ent) in compiled.iter().zip(resolved.entities.iter()) {
        if generator_targets.contains(ent.name.as_str()) {
            continue;
        }
        if resolved.instanced.contains(ent.name.as_str()) {
            continue;
        }

        let is_primary_terrain = Some(ent.name.as_str()) == primary_terrain_name;
        if let Some(grid) = primary_terrain_grid.as_ref().filter(|_| is_primary_terrain) {
            let (w, h, d) = grid.dims();
            layers.push(LayerInfo {
                name: ent.name.clone(), dims: [w, h, d], voxels: grid.filled_count(),
            });
            all_voxels.extend(
                grid_to_scene(grid, &resolved.atoms, terrain_center_offset).voxels
            );
            continue;
        }

        let sp   = solved_parts(resolved_ent, ent, ent.voxel_size)?;
        let grid = rasterize_entity(&sp, ent.voxel_size);
        let (gw, gh, gd) = grid.dims();
        layers.push(LayerInfo {
            name: ent.name.clone(), dims: [gw, gh, gd], voxels: grid.filled_count(),
        });

        let is_heightfield = resolved_ent.parts.iter().any(|p| matches!(&p.shape,
            Some(ShapeExpr::Heightfield { .. })));
        let y_off = if is_heightfield { 0 } else { -(gh as i32 - 1) };
        let layer_offset = (-(gw as i32 / 2), y_off, -(gd as i32 / 2));

        all_voxels.extend(
            grid_to_scene(&grid, &resolved.atoms, layer_offset).voxels
        );
    }

    if !generators.is_empty() {
        if let Some(ref terrain_grid) = primary_terrain_grid {
            let placements = run_generators(terrain_grid, &generators);
            for placement in &placements {
                let Some(target_ent) = compiled.iter()
                    .find(|e| e.name == placement.target_name) else { continue };
                let Some(target_resolved) = resolved.entities.iter()
                    .find(|e| e.name == target_ent.name) else { continue };

                let sp   = solved_parts(target_resolved, target_ent, target_ent.voxel_size)?;
                let grid = rasterize_entity(&sp, target_ent.voxel_size);

                let (tw, _, td) = grid.dims();
                let world_off = (
                    placement.x - tw as i32 / 2 + terrain_center_offset.0,
                    placement.y + 1,
                    placement.z - td as i32 / 2 + terrain_center_offset.2,
                );
                all_voxels.extend(
                    grid_to_scene(&grid, &resolved.atoms, world_off).voxels
                );
            }
        }
    }

    let bounds = bounds_of(&all_voxels);
    Ok(WorldOutput {
        total: all_voxels.len(),
        voxels: all_voxels,
        layers,
        bounds,
    })
}

/// Solve one entity's frames, gate on constraints, pair atom ids — the
/// library twin of main's solved_parts, returning errors instead of
/// exiting the process.
fn solved_parts(
    resolved_ent: &ResolvedEntity,
    compiled_ent: &CompiledEntity,
    voxel_size:   f64,
) -> Result<Vec<(String, ShapeExpr, u16, Frame)>, Vec<CompileError>> {
    let parts: Vec<(String, ShapeExpr)> = resolved_ent.parts.iter()
        .filter_map(|p| p.shape.clone().map(|s| (p.name.clone(), s)))
        .collect();

    let frames = resolve_frames(&parts, &resolved_ent.relations).map_err(|errs| {
        errs.iter()
            .map(|e| err("place", e.to_string(), span_of_placement(e)))
            .collect::<Vec<_>>()
    })?;

    let shape_of: std::collections::HashMap<&str, &ShapeExpr> =
        parts.iter().map(|(n, s)| (n.as_str(), s)).collect();
    for con in &resolved_ent.constraints {
        if let ConstraintExpr::Relation(rel) = &con.expr {
            if let Err(e) = check_relation_constraint(rel, &shape_of, &frames, voxel_size / 2.0) {
                return Err(vec![err("constraint", e.to_string(), span_of_placement(&e))]);
            }
        }
    }

    let atom_of: std::collections::HashMap<&str, u16> = compiled_ent.parts.iter()
        .map(|cp| (cp.name.as_str(), cp.atom_id))
        .collect();

    Ok(parts.into_iter().map(|(name, shape)| {
        let frame = frames[&name];
        let atom  = atom_of.get(name.as_str()).copied().unwrap_or(1);
        (name, shape, atom, frame)
    }).collect())
}

// ── Front end (shared by every output) ────────────────────────────────

/// Lex + parse + resolve. All diagnostics are collected — the LLM gets
/// every problem in one round trip, not one at a time.
///
/// S1: compile only what is inside ```moxi fences. Masking preserves line
/// numbers exactly, so spans stay absolute to the user's file. Files with
/// no fence are passed through unchanged (legacy `#`/`>` rules).
fn front_end(
    source: &str,
) -> Result<(crate::resolver::ResolvedScene, Vec<GeneratorDecl>), Vec<CompileError>> {
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

// ── Scene surface (the canonical IR) ──────────────────────────────────

/// Compile to the solved scene: every printed thing's parts with folded
/// shapes, world frames, and resolved colors — and NO voxels. This is the
/// representation every backend should derive from; `compile_source` is
/// the voxel backend.
///
/// Layer selection matches `compile_source` exactly (templates and
/// generator targets are components, not layers) so the two outputs
/// describe the same things in the same order.
pub fn compile_to_scene(source: &str) -> Result<crate::scene::Scene, Vec<CompileError>> {
    use crate::colors::resolve_color;
    use crate::scene::{FrameOut, Layer, Part, Scene, Shape, SCHEMA};

    let (resolved, generators) = front_end(source)?;
    let compiled = geometry::compile(&resolved, 1.0);

    let generator_targets: HashSet<&str> = generators
        .iter().map(|g| g.scatter_target.name.as_str()).collect();

    let mut layers = Vec::new();
    for (ent, resolved_ent) in compiled.iter().zip(resolved.entities.iter()) {
        if generator_targets.contains(ent.name.as_str()) { continue; }
        if resolved.instanced.contains(ent.name.as_str()) { continue; }

        let parts = solved_parts(resolved_ent, ent, ent.voxel_size)?
            .into_iter()
            .map(|(name, shape, atom_id, frame)| Part {
                name,
                shape: Shape::from_expr(&shape),
                frame: FrameOut::from_frame(&frame),
                color: resolved.atoms
                    .get(atom_id.saturating_sub(1) as usize)
                    .map(|a| resolve_color(&a.color))
                    .unwrap_or_else(|| "#ff00ff".to_string()),
            })
            .collect();

        layers.push(Layer { thing: ent.name.clone(), voxel_size: ent.voxel_size, parts });
    }

    Ok(Scene { version: env!("CARGO_PKG_VERSION").to_string(), schema: SCHEMA, layers })
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

// ── JSON surface (the wire format for web + WASM) ─────────────────────────

/// Compile and serialize in one call — the single function every host
/// binds: CLI `moxi json`, a server endpoint, or the WASM export.
///
/// Success: {"ok":true,"total":N,"bounds":[[..],[..]],
///           "layers":[{"name","dims","voxels"}…],
///           "voxels":[{"x","y","z","color"}…]}
/// Failure: {"ok":false,"errors":[{"stage","message","line","col"}…]}
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
        // Both materials made it through to colors.
        let colors: std::collections::HashSet<&str> =
            world.voxels.iter().map(|v| v.color.as_str()).collect();
        assert!(colors.len() >= 2, "expected trunk + leaf colors, got {colors:?}");
        // The template is a component, not a layer: Grove only.
        assert_eq!(world.layers.len(), 1);
        assert_eq!(world.layers[0].name, "Grove");
        // JSON round-trips and carries the ok flag.
        let json = compile_to_json(GOOD);
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"voxels\""));
    }

    #[test]
    fn bad_source_returns_structured_errors_not_exit() {
        // Unknown anchor: the LLM should get stage=resolve + line/col +
        // the valid vocabulary, in one round trip.
        let bad = GOOD.replace("Crown.bottom", "Crown.botom");
        let errs = compile_source(&bad).expect_err("should fail");
        assert!(errs.iter().any(|e|
            e.stage == "resolve" && e.line.is_some() && e.message.contains("botom")));

        let json = compile_to_json(&bad);
        assert!(json.contains("\"ok\":false"));
        assert!(json.contains("\"stage\""));

        // Total garbage still yields structured parse errors, no panic.
        let json = compile_to_json("entity { nope");
        assert!(json.contains("\"ok\":false"));
    }

    /// The parity claim for the atom/material unification, pinned as a
    /// test rather than left to the migration script: the old
    /// two-declaration form and the new self-contained form must produce
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
    /// Cells are compared too, on the first layer, through a rebuilt
    /// grid — count parity alone would pass a shape that moved.
    #[test]
    fn scene_round_trips_to_identical_voxels() {
        use crate::scene::Scene;
        use std::collections::HashMap;

        let world = compile_source(GOOD).expect("compiles");
        let scene = compile_to_scene(GOOD).expect("scene builds");
        assert_eq!(scene.layers.len(), world.layers.len());

        // JSON round-trip first, so we test the wire form, not the struct.
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
}