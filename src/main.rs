use clap::{Parser, Subcommand};
use moxi_lib::lexer::Lexer;
use moxi_lib::parser::Parser as MoxiParser;
use moxi_lib::resolver::Resolver;
use moxi_lib::geometry::{self, rasterize_entity, CompiledEntity};
use moxi_lib::frame::Frame;
use moxi_lib::frame_resolver::{check_relation_constraint, resolve_frames};
use moxi_lib::generator::run_generators;
use moxi_lib::types::{grid_to_scene, Voxel, VoxelScene};
use moxi_lib::export::export_to_obj;
use moxi_lib::ast::{ConstraintExpr, TopLevel};

// ── CLI definition ─────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name    = "moxi",
    about   = "Moxi v2 — semantic spatial description language",
    version = "0.2.0",
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compile a .md script and export to OBJ
    Compile {
        /// Path to the .md script
        script: String,

        /// Output directory (default: output/)
        #[arg(short, long, default_value = "output")]
        out: String,
    },

    /// Compile and open the 3D viewer
    View {
        /// Path to the .md script
        script: String,
    },

    /// Check a .md script for errors without producing output
    Check {
        /// Path to the .md script
        script: String,
    },
}

// ── Entry point ────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Compile { script, out } => {
            let source = read_script(&script);
            let scene  = compile_scene(&source, &script);
            run_export(&scene, &out);
        }
        Command::View { script } => {
            let source = read_script(&script);
            let scene  = compile_scene(&source, &script);
            let voxel_scene = build_world_scene(&scene);

            #[cfg(feature = "viewer")]
            moxi_lib::bevy_viewer::view_voxels_bevy(voxel_scene);

            #[cfg(not(feature = "viewer"))]
            {
                let _ = voxel_scene;
                eprintln!("viewer not enabled — rebuild with: cargo run --features viewer -- view <script>");
            }
        }
        Command::Check { script } => {
            let source = read_script(&script);
            check_only(&source, &script);
        }
    }
}

// ── Script loading ─────────────────────────────────────────────────────────

fn read_script(path: &str) -> String {
    if !path.ends_with(".md") {
        eprintln!("warning: Moxi scripts should have a .md extension (got '{path}')");
    }
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: cannot read '{path}': {e}");
        std::process::exit(1);
    })
}

// ── Compiled scene ─────────────────────────────────────────────────────────

struct CompiledScene {
    resolved:   moxi_lib::resolver::ResolvedScene,
    compiled:   Vec<moxi_lib::geometry::CompiledEntity>,
    generators: Vec<moxi_lib::ast::GeneratorDecl>,
}

fn compile_scene(source: &str, path: &str) -> CompiledScene {
    let (tokens, lex_errors) = Lexer::new(source).tokenize();
    for e in &lex_errors { eprintln!("[lex] {e}"); }

    let (doc, parse_errors) = MoxiParser::new(tokens).parse();
    for e in &parse_errors { eprintln!("[parse] {e}"); }

    // Extract generators before consuming doc
    let generators: Vec<_> = doc.items.iter().filter_map(|item| {
        if let TopLevel::GeneratorDecl(g) = item { Some(g.clone()) } else { None }
    }).collect();

    let (resolved, resolve_errors) = Resolver::new().resolve(doc);
    for e in &resolve_errors { eprintln!("[resolve] {e}"); }

    let total_errors = lex_errors.len() + parse_errors.len() + resolve_errors.len();
    if total_errors > 0 {
        eprintln!("{total_errors} error(s) in '{path}' — aborting");
        std::process::exit(1);
    }

    let compiled = geometry::compile(&resolved, 1.0);

    println!("✓ compiled '{path}'");
    for ent in &compiled {
        println!("  entity '{}' — {} parts", ent.name, ent.parts.len());
    }

    CompiledScene { resolved, compiled, generators }
}

// ── World scene builder ────────────────────────────────────────────────────
//
// Finds a terrain entity, runs generators, merges everything into one
// VoxelScene.  Falls back to rendering all entities stacked if no terrain
// is found (e.g. skeleton.md has no generators).

// viceroy: Phase B1 — frames flow straight into the universal rasterizer.
// The realize/snap step is gone: rasterize_entity tests contains(F⁻¹·p)
// per voxel, so ANY solved rotation is exact. This function is the single
// place frames meet voxels: it solves the entity's frames, gates on
// declared constraints (half a voxel of tolerance), and pairs each shaped
// part with its atom id for the rasterizer.
fn solved_parts(
    resolved_ent: &moxi_lib::resolver::ResolvedEntity,
    compiled_ent: &CompiledEntity,
    voxel_size:   f64,
) -> Vec<(String, moxi_lib::ast::ShapeExpr, u16, Frame)> {
    let parts: Vec<(String, moxi_lib::ast::ShapeExpr)> = resolved_ent.parts.iter()
        .filter_map(|p| p.shape.clone().map(|s| (p.name.clone(), s)))
        .collect();

    let frames = match resolve_frames(&parts, &resolved_ent.relations) {
        Ok(f) => f,
        Err(errs) => {
            for e in &errs { eprintln!("[place] {e}"); }
            std::process::exit(1);
        }
    };

    // Constraint gate: violations abort the compile with the expected vs
    // actual numbers, before rasterization.
    let shape_of: std::collections::HashMap<&str, &moxi_lib::ast::ShapeExpr> =
        parts.iter().map(|(n, s)| (n.as_str(), s)).collect();
    for con in &resolved_ent.constraints {
        if let ConstraintExpr::Relation(rel) = &con.expr {
            if let Err(e) = check_relation_constraint(rel, &shape_of, &frames, voxel_size / 2.0) {
                eprintln!("[constraint] {e}");
                std::process::exit(1);
            }
        }
    }

    // Atom ids from compilation, matched by part name.
    let atom_of: std::collections::HashMap<&str, u16> = compiled_ent.parts.iter()
        .map(|cp| (cp.name.as_str(), cp.atom_id))
        .collect();

    parts.into_iter().map(|(name, shape)| {
        let frame = frames[&name];
        let atom  = atom_of.get(name.as_str()).copied().unwrap_or(1);
        (name, shape, atom, frame)
    }).collect()
}

fn build_world_scene(scene: &CompiledScene) -> VoxelScene {
    let mut all_voxels: Vec<Voxel> = Vec::new();

    // Generator target names — these are placed by generators, not directly
    let generator_targets: std::collections::HashSet<&str> = scene.generators
        .iter().map(|g| g.scatter_target.name.as_str()).collect();

    // Primary terrain = first entity with a heightfield part (generators run against it)
    let primary_terrain_name = scene.resolved.entities.iter().find(|e| {
        e.parts.iter().any(|p| matches!(&p.shape,
            Some(moxi_lib::ast::ShapeExpr::Heightfield { .. })
        ))
    }).map(|e| e.name.as_str());

    let mut primary_terrain_grid = None;

    // First pass: compile primary terrain to get its center offset
    let mut terrain_center_offset: (i32, i32, i32) = (0, 0, 0);
    if let Some(ref pname) = primary_terrain_name {
        if let Some((ent, resolved_ent)) = scene.compiled.iter()
            .zip(scene.resolved.entities.iter())
            .find(|(e, _)| e.name.as_str() == *pname)
        {
            let sp   = solved_parts(resolved_ent, ent, ent.voxel_size);
            let grid = rasterize_entity(&sp, ent.voxel_size);
            let (w, _, d) = grid.dims();
            terrain_center_offset = (-(w as i32 / 2), 0, -(d as i32 / 2));
            primary_terrain_grid = Some(grid);
        }
    }

    // Render every entity in declaration order except generator targets
    for (ent, resolved_ent) in scene.compiled.iter().zip(scene.resolved.entities.iter()) {
        if generator_targets.contains(ent.name.as_str()) {
            continue;
        }

        // Entities used as instance templates are components of other
        // entities, not world layers — their geometry appears wherever
        // they were instanced.
        if scene.resolved.instanced.contains(ent.name.as_str()) {
            continue;
        }

        // Skip primary terrain — already compiled above
        if Some(ent.name.as_str()) == primary_terrain_name.as_deref()
            && primary_terrain_grid.is_some()
        {
            let grid = primary_terrain_grid.as_ref().unwrap();
            println!("  layer '{}': {}x{}x{}, {} voxels",
                ent.name, grid.dims().0, grid.dims().1, grid.dims().2,
                grid.filled_count());
            all_voxels.extend(
                grid_to_scene(grid, &scene.resolved.atoms, terrain_center_offset).voxels
            );
            continue;
        }

        let sp   = solved_parts(resolved_ent, ent, ent.voxel_size);
        let grid = rasterize_entity(&sp, ent.voxel_size);

        println!("  layer '{}': {}x{}x{}, {} voxels",
            ent.name, grid.dims().0, grid.dims().1, grid.dims().2,
            grid.filled_count());

        // Center this layer at world origin.
        // For flat layers (ocean, sand) push them down so top face sits at y=0,
        // leaving y>0 for terrain to grow into without being swallowed.
        let (gw, gh, gd) = grid.dims();
        let is_heightfield = resolved_ent.parts.iter().any(|p| matches!(&p.shape,
            Some(moxi_lib::ast::ShapeExpr::Heightfield { .. })
        ));
        let y_off = if is_heightfield { 0 } else { -(gh as i32 - 1) };
        let layer_offset = (-(gw as i32 / 2), y_off, -(gd as i32 / 2));

        all_voxels.extend(
            grid_to_scene(&grid, &scene.resolved.atoms, layer_offset).voxels
        );
    }

    // Run generators against primary terrain
    if !scene.generators.is_empty() {
        if let Some(ref terrain_grid) = primary_terrain_grid {
            let placements = run_generators(terrain_grid, &scene.generators);
            println!("  placed {} instances", placements.len());

            for placement in &placements {
                if let Some(target_ent) = scene.compiled.iter()
                    .find(|e| e.name == placement.target_name)
                {
                    let target_resolved = scene.resolved.entities.iter()
                        .find(|e| e.name == target_ent.name).unwrap();

                    let sp   = solved_parts(target_resolved, target_ent, target_ent.voxel_size);
                    let grid = rasterize_entity(&sp, target_ent.voxel_size);

                    let (tw, _, td) = grid.dims();
                    let world_off = (
                        placement.x - tw as i32 / 2 + terrain_center_offset.0,
                        placement.y + 1,
                        placement.z - td as i32 / 2 + terrain_center_offset.2,
                    );
                    all_voxels.extend(
                        grid_to_scene(&grid, &scene.resolved.atoms, world_off).voxels
                    );
                }
            }
        } else {
            eprintln!("warning: generators declared but no heightfield terrain found");
        }
    }

    println!("  total: {} voxels", all_voxels.len());
    VoxelScene::new(all_voxels)
}

// ── Export ─────────────────────────────────────────────────────────────────

fn run_export(scene: &CompiledScene, out_dir: &str) {
    std::fs::create_dir_all(out_dir).ok();
    let world = build_world_scene(scene);
    let path  = format!("{out_dir}/world");
    if let Err(e) = export_to_obj(&world, &path) {
        eprintln!("export error: {e}");
    }
}

// ── Check only ─────────────────────────────────────────────────────────────

fn check_only(source: &str, path: &str) {
    let (tokens, lex_errors) = Lexer::new(source).tokenize();
    let (doc, parse_errors)  = MoxiParser::new(tokens).parse();
    let (_, resolve_errors)  = Resolver::new().resolve(doc);

    let total = lex_errors.len() + parse_errors.len() + resolve_errors.len();
    for e in lex_errors.iter().chain(parse_errors.iter()).chain(resolve_errors.iter()) {
        eprintln!("{e}");
    }
    if total == 0 {
        println!("✓ '{path}' is valid");
    } else {
        eprintln!("{total} error(s)");
        std::process::exit(1);
    }
}
