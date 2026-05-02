use clap::{Parser, Subcommand, ValueEnum};
use moxi_lib::lexer::Lexer;
use moxi_lib::parser::Parser as MoxiParser;
use moxi_lib::resolver::Resolver;
use moxi_lib::geometry::{self, merge_parts};
use moxi_lib::relation_resolver::resolve_offsets;
use moxi_lib::generator::run_generators;
use moxi_lib::types::{grid_to_scene, Voxel, VoxelScene};
use moxi_lib::export::export_to_obj;
use moxi_lib::ast::TopLevel;

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

fn build_world_scene(scene: &CompiledScene) -> VoxelScene {
    // Try to find a heightfield-based terrain entity
    // Find terrain: any entity whose resolved parts contain a heightfield shape
    let terrain_ent = scene.compiled.iter().find(|e| {
        scene.resolved.entities.iter()
            .find(|re| re.name == e.name)
            .map(|re| re.parts.iter().any(|p| matches!(&p.shape,
                Some(moxi_lib::ast::ShapeExpr::Heightfield { .. })
            )))
            .unwrap_or(false)
    });

    let mut all_voxels: Vec<Voxel> = Vec::new();

    if let Some(terrain_ent) = terrain_ent {
        let terrain_resolved = scene.resolved.entities.iter()
            .find(|e| e.name == terrain_ent.name).unwrap();

        // Compile terrain
        let t_offsets = resolve_offsets(&terrain_ent.parts, &terrain_resolved.relations);
        let t_offsets_vec: Vec<_> = t_offsets.iter()
            .map(|(n, o)| (n.clone(), (o.dx, o.dy, o.dz))).collect();
        let terrain_grid = merge_parts(&terrain_ent.parts, &t_offsets_vec);

        println!("  terrain: {}x{}x{}, {} voxels",
            terrain_grid.dims().0, terrain_grid.dims().1,
            terrain_grid.dims().2, terrain_grid.filled_count());

        // Terrain voxels
        all_voxels.extend(
            grid_to_scene(&terrain_grid, &scene.resolved.atoms, (0,0,0)).voxels
        );

        // Run generators
        if !scene.generators.is_empty() {
            let placements = run_generators(&terrain_grid, &scene.generators);
            println!("  placed {} instances", placements.len());

            // Build a grid per unique target entity
            for placement in &placements {
                if let Some(target_ent) = scene.compiled.iter()
                    .find(|e| e.name == placement.target_name)
                {
                    let target_resolved = scene.resolved.entities.iter()
                        .find(|e| e.name == target_ent.name).unwrap();

                    let off = resolve_offsets(&target_ent.parts, &target_resolved.relations);
                    let off_vec: Vec<_> = off.iter()
                        .map(|(n,o)| (n.clone(),(o.dx,o.dy,o.dz))).collect();
                    let grid = merge_parts(&target_ent.parts, &off_vec);

                    let (tw, _, td) = grid.dims();
                    let world_off = (
                        placement.x - tw as i32 / 2,
                        placement.y + 1,
                        placement.z - td as i32 / 2,
                    );
                    all_voxels.extend(
                        grid_to_scene(&grid, &scene.resolved.atoms, world_off).voxels
                    );
                }
            }
        }
    } else {
        // No terrain — just render all entities with relations applied
        for (ent, resolved_ent) in scene.compiled.iter()
            .zip(scene.resolved.entities.iter())
        {
            let offsets = resolve_offsets(&ent.parts, &resolved_ent.relations);
            let offsets_vec: Vec<_> = offsets.iter()
                .map(|(n,o)| (n.clone(),(o.dx,o.dy,o.dz))).collect();
            let grid = merge_parts(&ent.parts, &offsets_vec);
            all_voxels.extend(
                grid_to_scene(&grid, &scene.resolved.atoms, (0,0,0)).voxels
            );
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