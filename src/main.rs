use clap::{Parser, Subcommand};
use moxi_lib::export::export_to_obj;
use moxi_lib::pipeline::{self, CompileError};
use moxi_lib::types::VoxelScene;

// ── CLI definition ─────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name    = "moxi",
    about   = "Moxi v2 — semantic spatial description language",
    version,
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

    /// Compile to structured JSON on stdout (the web/LLM surface):
    /// {"ok":true,"voxels":…} or {"ok":false,"errors":…}
    Json {
        /// Path to the .md script
        script: String,
    },

    /// Emit the entire grammar surface as JSON, derived from the compiler's
    /// own tables — not a script command, takes no file.
    Spec,

    /// Compile to the solved scene — shapes, frames, colors, no voxels.
    /// The canonical IR every backend derives from.
    Scene {
        /// Path to the .md script
        script: String,
    },

    /// Render SKILL.md from docs/skill_preamble.md plus the compiler's own
    /// tables. With --check, compare against the committed file instead of
    /// printing (this is what CI runs) and exit non-zero if stale.
    Skill {
        #[arg(long)]
        check: bool,
    },
}

// ── Entry point ────────────────────────────────────────────────────────────
//
// main is a THIN shell: every command goes through pipeline::compile_source,
// the same pure function the WASM target exports. Errors are data there;
// only here do they become stderr text and exit codes.

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Compile { script, out } => {
            let world = compile_or_exit(&read_script(&script), &script);
            report(&script, &world);
            std::fs::create_dir_all(&out).ok();
            let path = format!("{out}/world");
            let scene = VoxelScene::new(world.voxels);
            if let Err(e) = export_to_obj(&scene, &path) {
                eprintln!("export error: {e}");
                std::process::exit(1);
            }
        }

        Command::View { script } => {
            // The viewer consumes the SCENE, not voxels: a sphere draws as
            // a sphere. `compile_source` still runs first so the voxel
            // report stays available and both paths are exercised.
            let source = read_script(&script);
            let world  = compile_or_exit(&source, &script);
            report(&script, &world);

            match pipeline::compile_to_scene(&source) {
                Ok(scene) => moxi_lib::bevy_viewer::view_scene_bevy(scene),
                Err(errors) => {
                    print_errors(&errors);
                    eprintln!("{} error(s)", errors.len());
                    std::process::exit(1);
                }
            }
        }

        Command::Check { script } => {
            // Full check: front end AND placement/constraint stages.
            match pipeline::compile_source(&read_script(&script)) {
                Ok(_) => println!("✓ '{script}' is valid"),
                Err(errors) => {
                    print_errors(&errors);
                    eprintln!("{} error(s)", errors.len());
                    std::process::exit(1);
                }
            }
        }

        Command::Json { script } => {
            // The machine surface: JSON on stdout either way; the exit
            // code tells shells/servers whether compilation succeeded.
            let source = read_script(&script);
            let json = pipeline::compile_to_json(&source);
            let ok = json.contains("\"ok\":true");
            println!("{json}");
            if !ok {
                std::process::exit(1);
            }
        }

        Command::Spec => {
            println!("{}", moxi_lib::spec::to_json_pretty());
        }

        Command::Scene { script } => {
            match pipeline::compile_to_scene(&read_script(&script)) {
                Ok(scene) => println!("{}", scene.to_json_pretty()),
                Err(errors) => {
                    print_errors(&errors);
                    eprintln!("{} error(s)", errors.len());
                    std::process::exit(1);
                }
            }
        }

        Command::Skill { check } => {
            let preamble = std::fs::read_to_string("docs/skill_preamble.md").unwrap_or_else(|e| {
                eprintln!("error: cannot read docs/skill_preamble.md: {e}");
                std::process::exit(1);
            });
            let rendered = moxi_lib::skill::render(&preamble);

            if check {
                let committed = std::fs::read_to_string("SKILL.md").unwrap_or_default();
                // `moxi skill > SKILL.md` writes through println!, so the
                // file carries a trailing newline the rendered string does
                // not. Compare with trailing newlines normalized away, or
                // this check fails immediately after a correct regeneration
                // and CI is red forever.
                if rendered.trim_end_matches('\n') != committed.trim_end_matches('\n') {
                    eprintln!("SKILL.md is stale — run `moxi skill > SKILL.md` and commit the result.");
                    std::process::exit(1);
                }
                println!("SKILL.md is up to date.");
            } else {
                println!("{rendered}");
            }
        }
    }
}
// ── Helpers ────────────────────────────────────────────────────────────────

fn read_script(path: &str) -> String {
    if !path.ends_with(".md") {
        eprintln!("warning: Moxi scripts should have a .md extension (got '{path}')");
    }
    std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error: cannot read '{path}': {e}");
        std::process::exit(1);
    })
}

fn compile_or_exit(source: &str, path: &str) -> pipeline::WorldOutput {
    match pipeline::compile_source(source) {
        Ok(world) => world,
        Err(errors) => {
            print_errors(&errors);
            eprintln!("{} error(s) in '{path}' — aborting", errors.len());
            std::process::exit(1);
        }
    }
}

fn print_errors(errors: &[CompileError]) {
    for e in errors {
        match (e.line, e.col) {
            (Some(l), Some(c)) => eprintln!("[{}] {} ({}:{})", e.stage, e.message, l, c),
            _                  => eprintln!("[{}] {}", e.stage, e.message),
        }
    }
}

fn report(path: &str, world: &pipeline::WorldOutput) {
    println!("✓ compiled '{path}'");
    for layer in &world.layers {
        println!("  layer '{}': {}x{}x{}, {} voxels",
            layer.name, layer.dims[0], layer.dims[1], layer.dims[2], layer.voxels);
    }
    println!("  total: {} voxels", world.total);
}
