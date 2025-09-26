use clap::Parser;

mod types;
mod colors;
mod export;
mod bevy_viewer;

mod moxi;

use crate::bevy_viewer::view_voxels_bevy;
use crate::export::export_to_obj;
use crate::moxi::runtime::{translate, merge};

/// Moxi Programming Language: Build with squish. Render with rage.
#[derive(Parser)]
#[command(name = "moxi")]
#[command(about = "voxel engine and programming language", long_about = None)]
struct Cli {
    /// Input file to parse and render
    #[arg(short, long)]
    input: String,

    /// Output file to export as .obj
    #[arg(short, long)]
    output: Option<String>,

    /// Show a preview window
    #[arg(long)]
    preview: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let source = std::fs::read_to_string(&cli.input)?;

    // Parse DSL → AST → SceneGraph
    let tokens = moxi::lexer::lex(&source);
    let ast = moxi::parser::parse(tokens);
    println!("AST: {:?}", ast);   // debug

    let scene_graph = moxi::runtime::eval(ast);

    // flatten to VoxelScene for rendering/export
    let scene = scene_graph.flatten();

    println!("Built scene with {} voxels", scene.voxels.len());

    if cli.preview {
        view_voxels_bevy(scene.clone()); // ✅ expects VoxelScene
    }

    if let Some(output_path) = cli.output {
        export_to_obj(&scene, &output_path)?; // ✅ expects &VoxelScene
        println!("Exported .obj to {}", output_path);
    }

    Ok(())
}
