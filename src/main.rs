use clap::Parser;

mod types;
mod colors;
mod export;
mod bevy_viewer;
mod geom;

mod moxi;

use crate::bevy_viewer::view_voxels_bevy;
use crate::export::export_to_obj;

/// Moxi Programming Language: Build with squish. Render with rage.
#[derive(Parser)]
#[command(name = "moxi")]
#[command(about = "voxel engine and programming language", long_about = None)]
struct Cli {
    /// Input file to parse and render
    input: String,

    /// Output file to export as .obj
    #[arg(short, long)]
    output: Option<String>,

    /// Disable preview window
    #[arg(long, default_value_t = false)]
    no_show: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let source = std::fs::read_to_string(&cli.input)?;

    // Parse DSL → AST → SceneGraph
    let tokens = moxi::lexer::lex(&source);
    let ast = moxi::parser::parse(tokens);
    println!("AST: {:?}", ast);   // debug

    let scene_graph = moxi::runtime::eval(ast);
    let scene = scene_graph.resolve_voxels();

    println!("Built scene with {} voxels", scene.voxels.len());

    // Default = show preview, unless `--no-show`
    if !cli.no_show {
        view_voxels_bevy(scene.clone());
    }

    if let Some(output_path) = cli.output {
        export_to_obj(&scene, &output_path)?;
        println!("Exported .obj to {}", output_path);
    }

    Ok(())
}
