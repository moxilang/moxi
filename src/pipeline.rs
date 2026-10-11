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
use crate::resolver::{ResolvedEntity, ResolvedScene, Resolver};
use crate::types::{grid_to_scene, Voxel};
use crate::imports::Program;

// ── Output types ───────────────────────────────────────────────────────────

/// One compile error, machine-readable. `stage` is one of:
/// "lex" | "parse" | "resolve" | "place" | "constraint".
#[derive(Debug, Clone, Serialize)]
pub struct CompileError {
    pub stage:   String,
    pub message: String,
    pub line:    Option<usize>,
    pub col:     Option<usize>,
    /// The imported file the error is in (Issue J); absent for the script
    /// being compiled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file:    Option<String>,
}

thread_local! {
    /// The file table of the compile in progress on this thread: index =
    /// `Span::file`. Set by `front_end`, read by `err` — every error made
    /// from a span after the front end (resolve, place, constraint) names
    /// the imported file it points into.
    static FILES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn file_of(span: Option<Span>) -> Option<String> {
    let id = span?.file as usize;
    if id == 0 { return None; }
    FILES.with(|f| f.borrow().get(id).cloned())
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
        col:     span.map(|s| s.col as usize),
        file:    file_of(span),
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
        | MoxiError::UnplacedParts { span, .. }
        | MoxiError::InstanceError { span, .. }
        | MoxiError::ExprError { span, .. }
        | MoxiError::FnError { span, .. }
        | MoxiError::PoseError { span, .. }
        | MoxiError::AnimationError { span, .. }
        | MoxiError::MaterialError { span, .. }
        | MoxiError::StateError { span, .. }
        | MoxiError::JointError { span, .. }
        | MoxiError::RevolveError { span, .. }
        | MoxiError::AimError { span, .. }
        | MoxiError::BreakError { span, .. }
        | MoxiError::ImportError { span, .. } => Some(*span),
        MoxiError::UnexpectedEof { .. }
        | MoxiError::ConstraintViolation { .. } => None,
    }
}

fn span_of_placement(e: &PlacementError) -> Option<Span> {
    match e {
        PlacementError::OverConstrained { second, .. } => Some(*second),
        PlacementError::UnknownPart { span, .. }
        | PlacementError::Anchor { span, .. }
        | PlacementError::ConstraintViolation { span, .. }
        | PlacementError::AimCycle { span, .. } => Some(*span),
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
fn front_end(program: Program) -> Result<(ResolvedScene, Vec<GeneratorDecl>), Vec<CompileError>> {
    FILES.with(|f| f.borrow_mut().clear());
    let loaded = crate::imports::load(program).map_err(|es| es.into_iter().map(|e| {
        let span = span_of(&e.error);
        CompileError {
            stage: e.stage.to_string(), message: e.error.to_string(),
            line: span.map(|s| s.line), col: span.map(|s| s.col as usize), file: e.file,
        }
    }).collect::<Vec<_>>())?;
    FILES.with(|f| *f.borrow_mut() = loaded.files.clone());
    let crate::imports::Loaded { doc, from, errors: load_errors, .. } = loaded;

    let generators: Vec<GeneratorDecl> = doc.items.iter().filter_map(|item| {
        if let TopLevel::GeneratorDecl(g) = item { Some(g.clone()) } else { None }
    }).collect();

    let (mut resolved, resolve_errors) = Resolver::new().resolve(doc);
    resolved.imported = from;

    let mut errors: Vec<CompileError> = load_errors.iter().map(|e| {
        let span = span_of(&e.error);
        CompileError { stage: e.stage.to_string(), message: e.error.to_string(),
            line: span.map(|s| s.line), col: span.map(|s| s.col as usize), file: e.file.clone() }
    }).collect();
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
    /// The thing's animations, keyframes solved (empty for most things).
    pub animations:      Vec<crate::scene::Animation>,
    /// part → material index (into `ResolvedScene::materials`)
    pub materials:       std::collections::HashMap<String, usize>,
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
        let (mut parts, mut joints, animations) = solved_parts(resolved_ent, ent, vs, this_pose)?;
        let mut materials: std::collections::HashMap<String, usize> = resolved_ent.parts.iter()
            .filter_map(|p| p.material_index.map(|m| (p.name.clone(), m))).collect();

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

                let (target_parts, target_joints, _) =
                    solved_parts(target_res, target_ent, target_ent.voxel_size, None)?;
                for p in &target_res.parts {
                    if let Some(m) = p.material_index {
                        materials.insert(format!("{prefix}.{}", p.name), m);
                    }
                }
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

        layers.push(PlacedLayer {
            thing: ent.name.clone(), voxel_size: vs, has_heightfield, parts, joints, animations, materials,
        });
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

type SolvedParts = (
    Vec<(String, ShapeExpr, u16, Frame)>,
    std::collections::HashMap<String, crate::joints::Joint>,
    Vec<crate::scene::Animation>,
);

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

    // Joints of rest and of every named pose, for animations.
    let mut pose_joints: std::collections::HashMap<&str, std::collections::HashMap<String, crate::joints::Joint>> =
        std::collections::HashMap::new();
    pose_joints.insert(pose.map_or("rest", |p| p.name.as_str()), joints.clone());
    if pose.is_some() {
        let (_, _, j) = solve_checked(&raw, &resolved_ent.relations, resolved_ent, voxel_size, None)?;
        pose_joints.insert("rest", j);
    }
    for p in &resolved_ent.poses {
        if pose.is_some_and(|q| q.name == p.name) { continue; }
        let (_, _, j) = solve_checked(&raw, &apply_pose(&resolved_ent.relations, p), resolved_ent, voxel_size, Some(&p.name))?;
        pose_joints.insert(p.name.as_str(), j);
    }
    let mut animations = animations_of(resolved_ent, &pose_joints)?;
    animations.extend(revolves_of(resolved_ent, &raw, voxel_size)?);

    let atom_of: std::collections::HashMap<&str, u16> = compiled_ent.parts.iter()
        .map(|cp| (cp.name.as_str(), cp.atom_id))
        .collect();

    let solved = parts.into_iter().map(|(name, shape)| {
        let frame = frames[&name];
        let atom  = atom_of.get(name.as_str()).copied().unwrap_or(1);
        (name, shape, atom, frame)
    }).collect();
    Ok((solved, joints, animations))
}

/// Largest turn, in degrees, an animation may ask of one joint between two
/// keyframes. A player slerps the short way, so 200° would play as −160°.
const MAX_TURN_DEG: f64 = 179.0;

/// Keyframes → joint frames, with the 179° rule checked two ways: on the
/// written qualifiers (so `lean` 0 → 270 is caught, which a rotation
/// matrix cannot tell from −90) and on each joint's rotation relative to its
/// parent (so a combination of qualifiers that nears a half turn is too).
fn animations_of(
    ent:         &ResolvedEntity,
    pose_joints: &std::collections::HashMap<&str, std::collections::HashMap<String, crate::joints::Joint>>,
) -> Result<Vec<crate::scene::Animation>, Vec<CompileError>> {
    use crate::resolver::apply_pose;
    use crate::scene::{AnimKey, Animation, FrameOut};

    let num = |e: &crate::ast::Expr| match crate::value::eval(e, &std::collections::HashMap::new()) {
        Ok(crate::value::Value::Num(v)) => v,
        _ => 0.0,
    };
    // pose → subject → [twist, pitch, lean.a, lean.b]
    let quals_of = |rels: &[crate::ast::Placement]| -> std::collections::HashMap<String, [f64; 4]> {
        rels.iter().filter_map(|p| match p {
            crate::ast::Placement::Align { subject, twist, pitch, offsets, .. } => Some((
                subject.part.clone(),
                [num(twist), num(pitch), num(&offsets.lean.0), num(&offsets.lean.1)],
            )),
            _ => None,
        }).collect()
    };
    let rels_of = |pose: &str| match ent.poses.iter().find(|p| p.name == pose) {
        Some(p) => apply_pose(&ent.relations, p),
        None    => ent.relations.clone(),
    };
    let local = |joints: &std::collections::HashMap<String, crate::joints::Joint>, part: &str| -> Option<Frame> {
        let j = joints.get(part)?;
        let parent = j.parent.as_ref().and_then(|q| joints.get(q)).map_or(Frame::IDENTITY, |p| p.frame);
        Some(parent.inverse().compose(&j.frame))
    };

    let mut errors = Vec::new();
    let mut out = Vec::new();
    for a in &ent.animations {
        let aerr = |message: String, span: crate::error::Span| {
            let e = MoxiError::AnimationError { animation: a.name.clone(), message, span };
            err("animate", e.to_string(), span_of(&e))
        };
        for w in a.keys.windows(2) {
            let (k0, k1) = (&w[0], &w[1]);
            let (q0, q1) = (quals_of(&rels_of(&k0.pose)), quals_of(&rels_of(&k1.pose)));
            let mut parts: Vec<&String> = q0.keys().collect();
            parts.sort();
            for part in parts {
                let (Some(a0), Some(a1)) = (q0.get(part), q1.get(part)) else { continue };
                for (i, what) in ["twist", "pitch", "lean (first angle)", "lean (second angle)"].iter().enumerate() {
                    let d = (a1[i] - a0[i]).abs();
                    if d > MAX_TURN_DEG {
                        errors.push(aerr(format!(
                            "'{part}' {what} turns {d}° between {} s and {} s ({} → {}) — a player \
                             takes the short way round; insert a keyframe between them",
                            k0.time, k1.time, k0.pose, k1.pose), k1.span));
                    }
                }
            }
            let (j0, j1) = (&pose_joints[k0.pose.as_str()], &pose_joints[k1.pose.as_str()]);
            let mut names: Vec<&String> = j0.keys().collect();
            names.sort();
            for part in names {
                let (Some(l0), Some(l1)) = (local(j0, part), local(j1, part)) else { continue };
                let r = l0.rot.transpose().mul(&l1.rot);
                let c = ((r.0[0][0] + r.0[1][1] + r.0[2][2] - 1.0) / 2.0).clamp(-1.0, 1.0);
                let deg = c.acos().to_degrees();
                if deg > MAX_TURN_DEG {
                    errors.push(aerr(format!(
                        "'{part}' turns {deg:.1}° between {} s and {} s ({} → {}) — a player takes \
                         the short way round; insert a keyframe between them",
                        k0.time, k1.time, k0.pose, k1.pose), k1.span));
                }
            }
        }
        out.push(Animation {
            name:   a.name.clone(),
            looped: a.looped,
            keys:   a.keys.iter().map(|k| AnimKey {
                time:   k.time,
                pose:   k.pose.clone(),
                joints: pose_joints[k.pose.as_str()].iter()
                    .map(|(n, j)| (n.clone(), FrameOut::from_frame(&j.frame))).collect(),
            }).collect(),
        });
    }
    if errors.is_empty() { Ok(out) } else { Err(errors) }
}

/// `revolve` clips: every keyframe is the rest pose with each listed
/// qualifier advanced by RATE·t, solved like any pose (constraints checked).
/// The resolver chose the period (whole turns for every part) and the key
/// count (at most 90° per step), so the clip loops and plays the right way.
fn revolves_of(
    ent:        &ResolvedEntity,
    raw:        &[(String, ShapeExpr)],
    voxel_size: f64,
) -> Result<Vec<crate::scene::Animation>, Vec<CompileError>> {
    use crate::ast::{Expr, Placement, PoseQuals, Qual};
    use crate::resolver::{apply_pose, qual_value, PoseOverride, ResolvedPose};
    use crate::scene::{AnimKey, Animation, FrameOut};

    let mut out = Vec::new();
    for r in &ent.spins {
        let mut keys = Vec::new();
        for j in 0..=r.segments {
            let t = r.period * j as f64 / r.segments as f64;
            let mut overrides: Vec<PoseOverride> = Vec::new();
            for sp in &r.parts {
                let rest = ent.relations.iter()
                    .find(|p| matches!(p, Placement::Align { subject, .. } if subject.part == sp.part))
                    .and_then(|p| qual_value(p, sp.qual))
                    .unwrap_or(0.0);
                let v = Expr::Float(rest + sp.rate * t);
                let idx = match overrides.iter().position(|o| o.part == sp.part) {
                    Some(i) => i,
                    None => {
                        overrides.push(PoseOverride { part: sp.part.clone(), quals: PoseQuals::default(), span: sp.span });
                        overrides.len() - 1
                    }
                };
                match sp.qual {
                    Qual::Twist => overrides[idx].quals.twist = Some(v),
                    Qual::Pitch => overrides[idx].quals.pitch = Some(v),
                    _ => {}
                }
            }
            let label = format!("{}@{}", r.name, t);
            let pose = ResolvedPose { name: label.clone(), overrides, span: r.span };
            let (_, _, joints) = solve_checked(raw, &apply_pose(&ent.relations, &pose), ent, voxel_size, None)
                .map_err(|errs| errs.into_iter().map(|mut e| {
                    e.message = format!("in revolve '{}' at {t} s: {}", r.name, e.message);
                    e
                }).collect::<Vec<_>>())?;
            keys.push(AnimKey {
                time:   t,
                pose:   label,
                joints: joints.iter().map(|(n, jt)| (n.clone(), FrameOut::from_frame(&jt.frame))).collect(),
            });
        }
        out.push(Animation { name: r.name.clone(), looped: true, keys });
    }
    Ok(out)
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

pub fn compile_source<'a>(source: impl Into<Program<'a>>) -> Result<WorldOutput, Vec<CompileError>> {
    compile_source_posed(source, None)
}

/// The voxel backend with a named pose applied (`None` or `rest` = as
/// written). Same front end and placement step as `compile_source`; the pose
/// is a substitution into the mate qualifiers before the one solve, so a posed
/// voxel build is exactly what writing those numbers into the mates gives.
/// An unknown pose is an error listing the poses the printed things declare.
pub fn compile_source_posed<'a>(source: impl Into<Program<'a>>, pose: Option<&str>) -> Result<WorldOutput, Vec<CompileError>> {
    let (resolved, generators) = front_end(source.into())?;
    let compiled = geometry::compile(&resolved, 1.0);
    let placed   = place_world(&resolved, &compiled, &generators, pose)?;

    let mut all_voxels: Vec<Voxel> = Vec::new();
    let mut layers: Vec<LayerInfo> = Vec::new();

    for layer in &placed {
        // revealed parts are not there at rest (Issue G)
        let hidden = resolved.entities.iter().find(|e| e.name == layer.thing).map(hidden_parts).unwrap_or_default();
        let shown: Vec<(String, ShapeExpr, u16, Frame)>;
        let parts = if hidden.is_empty() { &layer.parts } else {
            shown = layer.parts.iter().filter(|p| !hidden.contains(p.0.as_str())).cloned().collect();
            &shown
        };
        let grid = rasterize_entity(parts, layer.voxel_size);
        let (gw, gh, gd) = grid.dims();
        layers.push(LayerInfo {
            name: layer.thing.clone(), dims: [gw, gh, gd], voxels: grid.filled_count(),
            parts: parts.len(),
        });

        // The grid's (0,0,0) is the world minimum of the layer's AABB, so
        // shifting by that minimum puts every voxel back at its solved
        // world position. Layers are no longer centered or sunk by
        // convention: a scene is a thing whose parts are placed by
        // relation, and those relations are the only thing that decides
        // where anything sits.
        let origin = grid_origin(parts, layer.voxel_size);

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
/// The solver's notes (not errors) for every printed thing, in a pose:
/// `(thing, message)` — the solver's own, an animation whose keys are all one
/// pose, and printed things drawn on top of each other. A second solve, read only for its notes — the scene's
/// frames come from `compile_to_scene_posed` as always.
pub fn solve_notes<'a>(source: impl Into<Program<'a>>, pose: Option<&str>) -> Result<Vec<(String, String)>, Vec<CompileError>> {
    let (resolved, generators) = front_end(source.into())?;
    let generator_targets: HashSet<&str> = generators.iter().map(|g| g.scatter_target.name.as_str()).collect();
    let mut out = Vec::new();
    // (thing, world box) of every drawn thing, for the on-top-of-each-other note
    let mut boxes: Vec<(String, [Vec3; 2])> = Vec::new();
    for ent in &resolved.entities {
        if generator_targets.contains(ent.name.as_str()) || resolved.instanced.contains(ent.name.as_str()) { continue; }
        let raw: Vec<(String, ShapeExpr)> = ent.parts.iter()
            .filter_map(|p| p.shape.clone().map(|s| (p.name.clone(), s)))
            .collect();
        let relations = match pose.and_then(|n| ent.poses.iter().find(|p| p.name == n)) {
            Some(p) => crate::resolver::apply_pose(&ent.relations, p),
            None    => ent.relations.clone(),
        };
        if let Ok((frames, _, notes)) = crate::frame_resolver::resolve_frames_noted(&raw, &relations) {
            out.extend(notes.into_iter().map(|n| (ent.name.clone(), n.message)));
            let hidden = hidden_parts(ent);
            let shown: Vec<&(String, ShapeExpr)> = raw.iter().filter(|(n, _)| !hidden.contains(n.as_str())).collect();
            if let Some(b) = world_box(&shown, &frames) { boxes.push((ent.name.clone(), b)); }
        }
        // An animation whose every key is the same pose never moves.
        for a in &ent.animations {
            if let Some(first) = a.keys.first() {
                if a.keys.len() >= 2 && a.keys.iter().all(|k| k.pose == first.pose) {
                    out.push((ent.name.clone(), format!(
                        "animation '{}' never changes: all {} keys are pose '{}' — \
                         give the keys different poses",
                        a.name, a.keys.len(), first.pose)));
                }
            }
        }
    }
    // Printed things are all drawn at the origin: two whose boxes mostly
    // coincide are drawn on top of each other.
    for (i, (a, ba)) in boxes.iter().enumerate() {
        for (b, bb) in &boxes[i + 1..] {
            let vol = |m: &[Vec3; 2]| (m[1].x - m[0].x) * (m[1].y - m[0].y) * (m[1].z - m[0].z);
            let lo = Vec3::new(ba[0].x.max(bb[0].x), ba[0].y.max(bb[0].y), ba[0].z.max(bb[0].z));
            let hi = Vec3::new(ba[1].x.min(bb[1].x), ba[1].y.min(bb[1].y), ba[1].z.min(bb[1].z));
            if hi.x <= lo.x || hi.y <= lo.y || hi.z <= lo.z { continue; }
            let shared = vol(&[lo, hi]);
            let smaller = vol(ba).min(vol(bb));
            if smaller > 0.0 && shared > 0.5 * smaller {
                out.push((b.clone(), format!(
                    "'{b}' is drawn on top of '{a}': printed things all sit at the origin \
                     (their boxes share {}% of the smaller). Fine for alternatives the game \
                     swaps between; otherwise make one a part of the other to place it",
                    (100.0 * shared / smaller).round())));
            }
        }
    }
    Ok(out)
}

/// The world box of a set of shaped parts (corners of each part's analytic
/// extents under its solved frame). `None` when nothing is shaped.
fn world_box(parts: &[&(String, ShapeExpr)], frames: &crate::frame_resolver::FrameMap) -> Option<[Vec3; 2]> {
    let mut min = Vec3::new(f64::MAX, f64::MAX, f64::MAX);
    let mut max = Vec3::new(f64::MIN, f64::MIN, f64::MIN);
    let mut any = false;
    for (name, shape) in parts.iter().copied() {
        let Some(f) = frames.get(name) else { continue };
        let e = crate::anchors::analytic_extents(shape);
        for &cx in &[e.min.x, e.max.x] {
            for &cy in &[e.min.y, e.max.y] {
                for &cz in &[e.min.z, e.max.z] {
                    let p = f.apply_point(Vec3::new(cx, cy, cz));
                    min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
                    max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
                    any = true;
                }
            }
        }
    }
    any.then_some([min, max])
}

/// Parts a breakable's `after` block reveals: hidden until that lump breaks.
fn hidden_parts(e: &crate::resolver::ResolvedEntity) -> HashSet<&str> {
    e.breakables.iter().flat_map(|b| b.reveal.iter().map(|p| p.as_str())).collect()
}

pub fn compile_to_scene<'a>(source: impl Into<Program<'a>>) -> Result<crate::scene::Scene, Vec<CompileError>> {
    compile_to_scene_posed(source, None)
}

/// The solved scene in a named pose (`None` or `Some("rest")` = the
/// script as written). A pose applies to every printed thing that declares
/// it; things without it stay at rest. Naming a pose no printed thing has
/// is an error listing the poses that exist.
pub fn compile_to_scene_posed<'a>(source: impl Into<Program<'a>>, pose: Option<&str>) -> Result<crate::scene::Scene, Vec<CompileError>> {
    use crate::colors::resolve_color;
    use crate::resolver::StateAttr;
    use crate::resolver::Trigger;
    use crate::scene::{BreakOut, FrameOut, JointOut, Layer, Look, Part, Scene, Shape, State, SCHEMA};

    let (resolved, generators) = front_end(source.into())?;
    let compiled = geometry::compile(&resolved, 1.0);
    let placed   = place_world(&resolved, &compiled, &generators, pose)?;

    let color_of_material = |m: usize| -> Option<String> {
        let mat = resolved.materials.get(m)?;
        resolved.atoms.get(mat.atom_index).map(|a| resolve_color(&a.color))
    };
    let layers = placed.into_iter().map(|layer| {
        let joints = layer.joints;
        let mats = layer.materials;
        let ent = resolved.entities.iter().find(|e| e.name == layer.thing);
        let pushes = ent.map(|e| push_vectors(e, &layer.parts, &joints)).unwrap_or_default();
        let hidden = ent.map(hidden_parts).unwrap_or_default();
        // a material's look, for a breakable's `after` swaps
        let look_of = |m: usize| {
            let x = resolved.materials.get(m);
            Look {
                material:  x.map(|x| x.name.clone()),
                color:     color_of_material(m).unwrap_or_else(|| "#ff00ff".to_string()),
                emissive:  x.and_then(|x| x.emissive),
                opacity:   x.and_then(|x| x.opacity),
                blend:     x.filter(|x| x.additive).map(|_| "additive".to_string()),
                roughness: x.and_then(|x| x.roughness),
                metalness: x.and_then(|x| x.metalness),
            }
        };
        let parts: Vec<Part> = layer.parts.into_iter().map(|(name, shape, atom_id, frame)| {
            let atom = resolved.atoms.get(atom_id.saturating_sub(1) as usize);
            let mat = mats.get(&name).and_then(|m| resolved.materials.get(*m));
            let limited = ent.and_then(|e| e.joints.iter().find(|j| j.part == name));
            let mate = ent.and_then(|e| e.relations.iter().find(|p|
                matches!(p, crate::ast::Placement::Align { .. }) && p.subject_name() == name));
            let joint = joints.get(&name).map(|j| JointOut {
                parent: j.parent.clone(),
                frame:  FrameOut::from_frame(&j.frame),
                limits: limited.map(|l| l.limits.iter().map(|(q, a, b)| (q.key().to_string(), [*a, *b])).collect())
                    .unwrap_or_default(),
                rest:   limited.zip(mate).map(|(l, m)| l.limits.iter()
                    .filter_map(|(q, _, _)| crate::resolver::qual_value(m, *q).map(|v| (q.key().to_string(), v)))
                    .collect()).unwrap_or_default(),
            });
            Part {
                shape: Shape::from_expr(&shape),
                frame: FrameOut::from_frame(&frame),
                color: atom.map(|a| resolve_color(&a.color))
                    .unwrap_or_else(|| "#ff00ff".to_string()),
                material: mat.map(|m| m.name.clone())
                    .or_else(|| atom.map(|a| a.name.clone()))
                    .filter(|n| !n.is_empty()),
                emissive: mat.and_then(|m| m.emissive),
                opacity: mat.and_then(|m| m.opacity),
                blend: mat.filter(|m| m.additive).map(|_| "additive".to_string()),
                roughness: mat.and_then(|m| m.roughness),
                metalness: mat.and_then(|m| m.metalness),
                joint,
                breakable: ent.and_then(|e| e.breakables.iter().find(|b| b.part == name)).map(|b| BreakOut {
                    hits:  match &b.trigger { Trigger::Hits(n) => Some(*n), Trigger::Event(_) => None },
                    event: match &b.trigger { Trigger::Event(e) => Some(e.clone()), Trigger::Hits(_) => None },
                    push:  pushes.get(&name).copied().unwrap_or([0.0, 1.0, 0.0]),
                    after: b.swaps.iter().map(|(p, m)| (p.clone(), look_of(*m))).collect(),
                    reveal: b.reveal.clone(),
                }),
                hidden: hidden.contains(name.as_str()),
                from: None,
                name,
            }
        }).collect();

        // Each state: the look of every part that differs from rest.
        let states = ent.map(|e| e.states.iter().map(|st| {
            let mut looks = std::collections::BTreeMap::new();
            for p in &parts {
                let Some(&m0) = mats.get(&p.name) else { continue };
                let m = st.part_materials.iter().find(|(q, _)| *q == p.name).map_or(m0, |(_, m)| *m);
                let mut look = Look {
                    material: resolved.materials.get(m).map(|x| x.name.clone()),
                    color:    color_of_material(m).unwrap_or_else(|| p.color.clone()),
                    emissive: resolved.materials.get(m).and_then(|x| x.emissive),
                    opacity:  resolved.materials.get(m).and_then(|x| x.opacity),
                    blend:    resolved.materials.get(m).filter(|x| x.additive).map(|_| "additive".to_string()),
                    roughness: resolved.materials.get(m).and_then(|x| x.roughness),
                    metalness: resolved.materials.get(m).and_then(|x| x.metalness),
                };
                let in_scope = st.scope.as_ref().is_none_or(|sc| p.name.starts_with(&format!("{sc}.")));
                for (am, a) in &st.attrs {
                    if *am != m || !in_scope { continue; }
                    match a {
                        StateAttr::Color(c)    => look.color = resolve_color(c),
                        StateAttr::Emissive(v) => look.emissive = Some(*v),
                        StateAttr::Opacity(v)  => look.opacity = (*v < 1.0).then_some(*v),
                        StateAttr::Roughness(v) => look.roughness = Some(*v),
                        StateAttr::Metalness(v) => look.metalness = Some(*v),
                    }
                }
                let rest = Look { material: p.material.clone(), color: p.color.clone(), emissive: p.emissive, opacity: p.opacity,
                    blend: p.blend.clone(), roughness: p.roughness, metalness: p.metalness };
                if look != rest {
                    looks.insert(p.name.clone(), look);
                }
            }
            State { name: st.name.clone(), family: st.family.clone(), looks }
        }).collect()).unwrap_or_default();

        // An instance of a thing from another file (Issue J): its root part
        // says where it came from (outermost instance first).
        let mut parts = parts;
        if let Some(e) = ent.filter(|_| !resolved.imported.is_empty()) {
            for (prefix, thing) in &e.instances {
                let Some((file, original)) = resolved.imported.get(thing) else { continue };
                let pre = format!("{prefix}.");
                let root = parts.iter().position(|p| p.name.starts_with(&pre)
                    && p.joint.as_ref().and_then(|j| j.parent.as_ref()).is_none_or(|q| !q.starts_with(&pre)));
                if let Some(i) = root {
                    if parts[i].from.is_none() {
                        parts[i].from = Some(crate::scene::FromOut { file: file.clone(), thing: original.clone() });
                    }
                }
            }
        }
        Layer {
            thing:      layer.thing,
            voxel_size: layer.voxel_size,
            animations: layer.animations,
            states,
            parts,
        }
    }).collect();

    let mut scene = Scene { version: env!("CARGO_PKG_VERSION").to_string(), schema: SCHEMA, layers };
    if !resolved.imported.is_empty() || resolved.materials.iter().any(|m| m.name.contains('@')) {
        show_names(&mut scene);
    }
    Ok(scene)
}

/// Private imported names (`Blood@lib_flame_md`) leave the compiler as
/// written in their file (`Blood`): a game matches materials by name.
fn show_names(scene: &mut crate::scene::Scene) {
    let fix = |m: &mut Option<String>| { if let Some(n) = m { *n = crate::imports::shown_name(n).to_string(); } };
    for l in &mut scene.layers {
        for p in &mut l.parts {
            fix(&mut p.material);
            if let Some(b) = &mut p.breakable { for look in b.after.values_mut() { fix(&mut look.material); } }
        }
        for st in &mut l.states { for look in st.looks.values_mut() { fix(&mut look.material); } }
    }
}

/// Each breakable part's `push` as a unit vector in the thing's space.
///
/// - `out`: the way the part leaves its socket — minus its own subject
///   anchor's normal (which faces the parent), or, for an orientation-free
///   subject anchor (`center`), the object socket's normal.
/// - a direction word: in the space of the thing that declared it, i.e.
///   turned by that instance's root part (the identity in its own thing).
/// - a mirror image: its source's vector reflected by the solver's own
///   mirror normal, read off the image's `mirror` wrapper — exact for any
///   plane.
fn push_vectors(
    ent:    &ResolvedEntity,
    parts:  &[(String, ShapeExpr, u16, Frame)],
    joints: &std::collections::HashMap<String, crate::joints::Joint>,
) -> std::collections::HashMap<String, [f64; 3]> {
    use crate::anchors::{resolve_anchor, AnchorKind};
    use crate::resolver::Push;

    let part = |n: &str| parts.iter().find(|(name, ..)| name == n);
    let space_rot = |space: &Option<String>| -> crate::frame::Mat3 {
        let Some(s) = space else { return crate::frame::Mat3::IDENTITY };
        let prefix = format!("{s}.");
        parts.iter()
            .find(|(n, ..)| n.starts_with(&prefix)
                && joints.get(n).and_then(|j| j.parent.as_ref()).is_none_or(|p| !p.starts_with(&prefix)))
            .map_or(crate::frame::Mat3::IDENTITY, |(_, _, _, f)| f.rot)
    };
    let own = |b: &crate::resolver::ResolvedBreak| -> Option<Vec3> {
        match b.push {
            Push::Dir(d) => {
                let [x, y, z] = d.vector();
                Some(space_rot(&b.space).apply(Vec3::new(x, y, z)))
            }
            Push::Out => {
                let crate::ast::Placement::Align { subject, object, .. } = ent.relations.iter()
                    .find(|p| matches!(p, crate::ast::Placement::Align { .. }) && p.subject_name() == b.part)?
                else { return None };
                let (_, s_shape, _, _) = part(&subject.part)?;
                let a = resolve_anchor(s_shape, &subject.anchor, &subject.args).ok()?;
                if a.kind == AnchorKind::Oriented {
                    return Some(joints.get(&b.part)?.frame.rot.col(1).neg());
                }
                let (_, o_shape, _, o_frame) = part(&object.part)?;
                let o = resolve_anchor(o_shape, &object.anchor, &object.args).ok()?;
                Some(o_frame.rot.apply(o.frame.rot.col(1)))
            }
        }
    };
    let mut out = std::collections::HashMap::new();
    for b in &ent.breakables {
        let v = match &b.mirror_of {
            None => own(b),
            Some(src) => ent.breakables.iter().find(|s| s.part == *src).and_then(own).and_then(|v| {
                let (_, shape, ..) = part(&b.part)?;
                let ShapeExpr::Mirror { args, .. } = shape else { return Some(v) };
                let arg = |k: &str| args.iter().find(|a| a.key == k).and_then(|a| match a.value {
                    crate::ast::Expr::Float(f) => Some(f),
                    _ => None,
                }).unwrap_or(0.0);
                let n = Vec3::new(arg("nx"), arg("ny"), arg("nz"));
                Some(v.sub(n.scale(2.0 * v.dot(n))))
            }),
        };
        if let Some(v) = v.and_then(|v| v.normalize()) {
            out.insert(b.part.clone(), [v.x, v.y, v.z]);
        }
    }
    out
}

// ── JSON surface (the wire format for web + WASM) ─────────────────────

/// Compile and serialize in one call — the single function every host
/// binds: CLI `moxi json`, a server endpoint, or the WASM export.
pub fn compile_to_json<'a>(source: impl Into<Program<'a>>) -> String {
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

    /// A second unplaced part used to sit at the origin and compile clean.
    /// Now the compile fails at resolve, pointing at the offender's line.
    #[test]
    fn a_second_unplaced_part_fails_the_compile_at_its_line() {
        const SRC: &str = r#"
material M { color = red }
thing T {
    part Post { shape = cylinder(height=4, radius=0.5), material = M }
    part Cap  { shape = sphere(radius=1), material = M }
    resolve voxel_size = 1.0
}
print T detail=low
"#;
        let Err(errs) = compile_source(SRC) else { panic!("must not compile") };
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].stage, "resolve");
        assert_eq!(errs[0].line, Some(5));
        assert!(errs[0].message.contains("'Cap' has no placement; its root is 'Post'"), "{}", errs[0].message);
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
        R.point(x=0, y=0, z=0, nx=0, ny=-1, nz=0) on Post.point(x=0, y=0, z=0, nx=0, ny=1, nz=0)
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

    // ── closing-the-circle L2 (DOC-20261011 §7 items 1–5) ─────────────────
    fn part_at(src: &str, name: &str) -> [f64; 3] {
        let scene = compile_to_scene(src).unwrap_or_else(|e| panic!("should compile: {e:?}"));
        let p = scene.layers.iter().flat_map(|l| l.parts.iter()).find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no part {name}"));
        p.frame.pos
    }

    const L2_BASE: &str = r##"
material M { color = "#888888" }   # a comment after code
thing T {
    part Base { shape = box(width=40, height=10, depth=40), material = M }
    part Peg  { shape = cylinder(height=10, radius=2), material = M }   # here too
    relation { Peg.bottom on Base.point(10, 5, 0, 0, 1, 0) }
    for i in 0..5 {
        part Dot[i] { shape = sphere(radius=1 + i % 3), material = M }
        relation { Dot[i].bottom on Base.top shift=(-15 + 7 * i, 0) }
    }
    joint Peg { twist=(0..110) }
}
print T
"##;

    #[test]
    fn end_of_line_comments_modulo_positional_point_and_paren_range() {
        // positional point(x, y, z, nx, ny, nz): the peg stands at x = 10, not at the origin
        let peg = part_at(L2_BASE, "Peg");
        assert!((peg[0] - 10.0).abs() < 1e-9 && (peg[1] - 5.0).abs() < 1e-9, "{peg:?}");
        // `%` cycles: radii 1, 2, 3, 1, 2 — Dot[3] sits as low as Dot[0]
        let (d0, d3, d2) = (part_at(L2_BASE, "Dot[0]"), part_at(L2_BASE, "Dot[3]"), part_at(L2_BASE, "Dot[2]"));
        assert!((d0[1] - d3[1]).abs() < 1e-9 && d2[1] > d0[1], "{d0:?} {d3:?} {d2:?}");
    }

    #[test]
    fn modulo_is_floored_and_refuses_zero() {
        let neg = L2_BASE.replace("1 + i % 3", "1 + (i - 7) % 3");   // (−7) % 3 = 2, not −1
        let d0 = part_at(&neg, "Dot[0]");
        let base = part_at(L2_BASE, "Dot[2]");
        assert!((d0[1] - base[1]).abs() < 1e-9, "floored remainder: {d0:?} vs {base:?}");
        let zero = L2_BASE.replace("1 + i % 3", "1 + i % 0");
        let errs = compile_to_scene(&zero).expect_err("modulo by zero is an error");
        assert!(format!("{errs:?}").contains("modulo by zero"), "{errs:?}");
    }

    #[test]
    fn positional_anchor_args_are_checked() {
        let after_named = L2_BASE.replace("Base.point(10, 5, 0, 0, 1, 0)", "Base.point(x=10, 5)");
        let e = format!("{:?}", compile_to_scene(&after_named).expect_err("positional after named"));
        assert!(e.contains("positional arguments first"), "{e}");
        let too_many = L2_BASE.replace("Base.point(10, 5, 0, 0, 1, 0)", "Base.point(1, 2, 3, 4, 5, 6, 7)");
        let e = format!("{:?}", compile_to_scene(&too_many).expect_err("seven positional"));
        assert!(e.contains("x, y, z, nx, ny, nz"), "{e}");
        let surface = L2_BASE.replace("Peg.bottom on Base.point(10, 5, 0, 0, 1, 0)", "Peg.bottom on Base.top shift=(0, 0)")
            .replace("relation { Dot[i].bottom on Base.top", "relation { Dot[i].bottom on Base.surface(10, 20)");
        let e = format!("{:?}", compile_to_scene(&surface).expect_err("surface takes names"));
        assert!(e.contains("named arguments"), "{e}");
    }

    #[test]
    fn a_single_range_in_parentheses_reads_as_the_range_and_two_are_refused() {
        let two = L2_BASE.replace("twist=(0..110)", "twist=(0..110, 5..6)");
        let e = format!("{:?}", compile_to_scene(&two).expect_err("two ranges for twist"));
        assert!(e.contains("only lean and shift take a pair"), "{e}");
    }

    #[test]
    fn a_path_into_an_instance_names_its_part() {
        let src = r##"
material M { color = "#888888" }
thing Run(len=60) {
    part Footing { shape = box(width=len, height=4, depth=10), material = M }
    part Block   { shape = box(width=10, height=20, depth=8), material = M }
    relation { Block.bottom on Footing.top shift=(len / 2 - 5, 0) }
}
thing Pair {
    part A   { thing = Run(len=60) }
    part B   { thing = Run(len=40) }
    part Cap { shape = sphere(radius=3), material = M }
    relation {
        B.Footing.bottom on A.Footing.top shift=(0, 20)
        Cap.bottom on B.Block.top
    }
}
print Pair
"##;
        let b = part_at(src, "B.Footing");
        let a_top = 2.0;   // A.Footing is 4 high, centred at 0
        assert!((b[1] - (a_top + 2.0)).abs() < 1e-9, "B.Footing sits on A.Footing: {b:?}");
        let cap = part_at(src, "Cap");
        let block = part_at(src, "B.Block");
        assert!((cap[0] - block[0]).abs() < 1e-9 && cap[1] > block[1], "Cap on B.Block: {cap:?} {block:?}");
        let bad = src.replace("Cap.bottom on B.Block.top", "Cap.bottom on B.Block[0].top[1]");
        assert!(compile_to_scene(&bad).is_err(), "an index on the anchor is refused");
    }

    #[test]
    fn a_near_half_turn_aim_without_facing_is_noted_and_facing_silences_it() {
        let src = r##"
material M { color = "#888888" }
thing T {
    part Base { shape = box(width=40, height=10, depth=40), material = M }
    part Rod  { shape = capsule(height=30, radius=2), material = M }
    part Tip  { shape = capsule(height=10, radius=1), material = M }
    relation {
        Rod.bottom on Base.top aim=[0.5, -1000, 0.3]
        Tip.bottom on Base.top shift=(15, 0) aim=[30, 100, 0]
    }
}
print T
"##;
        let notes = solve_notes(src, None).expect("compiles");
        assert_eq!(notes.len(), 1, "only the near-antipodal aim is noted: {notes:?}");
        assert_eq!(notes[0].0, "T");
        assert!(notes[0].1.starts_with("'Rod' aims 179.") && notes[0].1.contains("add `facing=`"), "{}", notes[0].1);
        let faced = src.replace("aim=[0.5, -1000, 0.3]", "aim=[0.5, -1000, 0.3] facing=north");
        assert!(solve_notes(&faced, None).unwrap().is_empty(), "facing fixes the roll");
        // the note never changes the solve
        let scene_a = serde_json::to_string(&compile_to_scene(src).unwrap()).unwrap();
        assert!(scene_a.contains("Rod"));
    }

    // ── L3 item 6: a `let` point as an aim target ───────────────────────

    const L3_POINT: &str = r##"
material M { color = "#888888" }
thing Lamp(reach=40) {
    let p = [reach, 30, 10]
    part Base { shape = box(width=20, height=10, depth=20), material = M }
    part Arm  { shape = capsule(height=24, radius=2), material = M }
    part Head { shape = capsule(height=24, radius=2), material = M }
    relation {
        Arm.bottom  on Base.top aim=p
        Head.bottom on Base.top shift=(8, 0) aim=p
    }
    pose Look { Head aim=p }
}
print Lamp
"##;

    #[test]
    fn a_let_point_is_an_aim_target_exactly_like_the_literal_point() {
        let literal = L3_POINT.replace("aim=p", "aim=[reach, 30, 10]");
        let a = serde_json::to_string(&compile_to_scene(L3_POINT).expect("named point")).unwrap();
        let b = serde_json::to_string(&compile_to_scene(&literal).expect("literal point")).unwrap();
        assert_eq!(a, b, "the named point is the same aim");
        // …and it follows an instance's arguments
        let wide = L3_POINT.replace("print Lamp", "thing Row { part L { thing = Lamp(reach=80) } }\nprint Row");
        let wide_lit = literal.replace("print Lamp", "thing Row { part L { thing = Lamp(reach=80) } }\nprint Row");
        let a = serde_json::to_string(&compile_to_scene(&wide).expect("instanced named point")).unwrap();
        let b = serde_json::to_string(&compile_to_scene(&wide_lit).expect("instanced literal point")).unwrap();
        assert_eq!(a, b, "an instance's own arguments move the point");
        assert_ne!(
            serde_json::to_string(&compile_to_scene(L3_POINT).unwrap()).unwrap().replace("Lamp", "Row"),
            a, "reach=80 aims elsewhere than reach=40");
        // a pose's `aim=p` is the same point too
        let pa = serde_json::to_string(&compile_to_scene_posed(L3_POINT, Some("Look")).expect("posed")).unwrap();
        let pb = serde_json::to_string(&compile_to_scene_posed(&literal, Some("Look")).expect("posed literal")).unwrap();
        assert_eq!(pa, pb, "a pose line takes the named point");
    }

    #[test]
    fn a_name_that_is_not_a_point_keeps_the_aim_vocabulary_error() {
        let two = L3_POINT.replace("let p = [reach, 30, 10]", "let p = [reach, 30]");
        let e = format!("{:?}", compile_to_scene(&two).expect_err("a pair is not a point"));
        assert!(e.contains("'p' is neither a direction") && e.contains("a point `aim=[x, y, z]`"), "{e}");
    }

    #[test]
    fn aim_from_a_center_mate_names_the_oriented_point_that_fixes_it() {
        let src = r##"
material M { color = "#888888" }
thing T {
    part A { shape = box(width=10, height=10, depth=10), material = M }
    part B { shape = box(width=4, height=20, depth=4), material = M }
    relation { B.center on A.center aim=[0, 30, 40] }
}
print T
"##;
        let e = format!("{:?}", compile_to_scene(src).expect_err("orientation-free"));
        assert!(e.contains("`aim` needs an oriented anchor on both sides"), "{e}");
        assert!(e.contains("`B.point(x=0, y=0, z=0, nx=0, ny=1, nz=0)` and `A.point(x=0, y=0, z=0, nx=0, ny=1, nz=0)`"), "{e}");
        let fixed = src.replace("B.center on A.center",
            "B.bottom on A.point(x=0, y=0, z=0, nx=0, ny=1, nz=0)");
        assert!(compile_to_scene(&fixed).is_ok(), "the named fix compiles");
    }

    // ── L3 item 7: the instance's own solve error is the one reported ────

    #[test]
    fn an_unsolvable_instance_reports_the_solver_error_not_a_missing_anchor() {
        let src = r##"
material M { color = "#888888" }
thing Arm {
    part U { shape = box(width=4, height=20, depth=4), material = M }
    part L { shape = box(width=4, height=20, depth=4), material = M }
    relation { L.center on U.center aim=up }
}
thing Rig {
    part Base { shape = box(width=20, height=4, depth=20), material = M }
    part A { thing = Arm }
    relation { A.bottom on Base.top }
}
print Rig
"##;
        let e = format!("{:?}", compile_to_scene(src).expect_err("Arm does not solve"));
        assert!(!e.contains("has no anchor"), "the real cause, not a missing anchor: {e}");
        assert!(e.contains("instance 'A': needed 'A' placed to find its 'bottom'"), "{e}");
        assert!(e.contains("`aim` needs an oriented anchor on both sides"), "{e}");
    }

    // ── L3 item 9: `print A, B`; a printed thing that is also instanced ────

    #[test]
    fn print_takes_several_names_and_a_printed_instanced_thing_is_drawn() {
        let src = r##"
material M { color = "#888888" }
thing Demon { part H { shape = sphere(radius=3), material = M } }
thing Pair {
    part A { thing = Demon }
    part B { shape = box(width=4, height=4, depth=4), material = M }
    relation { B.bottom on A.top }
}
thing Lone { part L { shape = sphere(radius=2), material = M } }
print Lone
"##;
        let names = |s: &str| -> Vec<String> {
            compile_to_scene(s).expect("compiles").layers.iter().map(|l| l.thing.clone()).collect()
        };
        assert_eq!(names(src), ["Pair", "Lone"], "an instanced thing is not a layer of its own");
        let both = src.replace("print Lone", "print Demon, Lone detail=low");
        assert_eq!(names(&both), ["Demon", "Pair", "Lone"], "printing it makes it one");
        let bad = src.replace("print Lone", "print Lone, Nothing");
        let e = format!("{:?}", compile_to_scene(&bad).expect_err("unknown name"));
        assert!(e.contains("Nothing"), "{e}");
    }

    // ── L3 item 10: notes, and instanced constraints ───────────────────────

    #[test]
    fn a_no_change_animation_and_stacked_prints_are_noted() {
        let src = r##"
material M { color = "#888888" }
thing Door {
    part Slab { shape = box(width=10, height=20, depth=2), material = M }
    part Knob { shape = sphere(radius=1), material = M }
    relation { Knob.center on Slab.north }
    pose Open { Knob twist=10 }
    animate Swing loop { 0.0 rest  1.0 rest }
    animate Real  loop { 0.0 rest  1.0 Open }
}
thing Twin {
    part Slab { shape = box(width=10, height=20, depth=2), material = M }
}
print Door, Twin
"##;
        let notes = solve_notes(src, None).expect("compiles");
        let door: Vec<&str> = notes.iter().filter(|n| n.0 == "Door").map(|n| n.1.as_str()).collect();
        assert_eq!(door.len(), 1, "{notes:?}");
        assert!(door[0].starts_with("animation 'Swing' never changes: all 2 keys are pose 'rest'"), "{door:?}");
        let twin: Vec<&str> = notes.iter().filter(|n| n.0 == "Twin").map(|n| n.1.as_str()).collect();
        assert_eq!(twin.len(), 1, "{notes:?}");
        assert!(twin[0].starts_with("'Twin' is drawn on top of 'Door'"), "{twin:?}");
        // a thing mostly elsewhere is not on top of the door
        let far = r##"
material M { color = "#888888" }
thing Door {
    part Slab { shape = box(width=10, height=20, depth=2), material = M }
}
thing Far {
    part Pad  { shape = box(width=1, height=1, depth=1), material = M }
    part Ball { shape = sphere(radius=1), material = M }
    relation { Ball.bottom on Pad.top shift=(200, 0) }
}
print Door, Far
"##;
        let notes = solve_notes(far, None).expect("compiles");
        assert!(!notes.iter().any(|n| n.1.contains("on top of")), "{notes:?}");
    }

    #[test]
    fn a_constraint_in_an_instanced_thing_is_checked_under_each_instances_arguments() {
        let src = r##"
material M { color = "#888888" }
thing Arm(len=10) {
    part U { shape = box(width=4, height=len, depth=4), material = M }
    part H { shape = sphere(radius=3), material = M }
    relation { H.center on U.top gap=8 }
    constraint H above U
}
thing Rig {
    part Base { shape = box(width=20, height=4, depth=20), material = M }
    part A { thing = Arm(len=10) }
    relation { A.bottom on Base.top }
}
print Rig
"##;
        assert!(compile_to_scene(src).is_ok(), "the constraint holds");
        let bad = src.replace("gap=8", "gap=-7");
        let e = format!("{:?}", compile_to_scene(&bad).expect_err("H sinks into U"));
        assert!(e.contains("constraint violated: 'A.H' above 'A.U'"), "{e}");
        // nested: Cell holds a Rig; the constraint arrives with both prefixes
        let nested = bad.replace("print Rig", "thing Cell { part R { thing = Rig } }\nprint Cell");
        let e = format!("{:?}", compile_to_scene(&nested).expect_err("through two levels"));
        assert!(e.contains("'R.A.H' above 'R.A.U'"), "{e}");
    }
}
