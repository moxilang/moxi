use clap::Parser;

mod types;
mod colors;
mod export;
mod bevy_viewer;

mod mochi {
    pub mod lexer;
    pub mod parser;
    pub mod runtime;
}

use crate::bevy_viewer::view_voxels_bevy;
use crate::export::export_to_obj;
use crate::mochi::runtime::{translate, merge};

/// MochiVox: Build with squish. Render with rage.
#[derive(Parser)]
#[command(name = "mochivox")]
#[command(about = "Cute voxel engine and CLI", long_about = None)]
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

    // Parse DSL → AST → Scene
    let tokens = mochi::lexer::lex(&source);
    let ast = mochi::parser::parse(tokens);
    let scene = mochi::runtime::build_scene(ast);

    println!("Built scene with {} voxels", scene.voxels.len());

    // ✅ Test translate + merge manually
    let clone1 = translate(&scene, 5, 0, 0);
    let clone2 = translate(&scene, 0, 0, 5);
    let clone3 = translate(&scene, -5, 0, 0);

    let forest = merge(vec![scene.clone(), clone1, clone2, clone3]);

    println!("Forest has {} voxels", forest.voxels.len());

    if cli.preview {
        view_voxels_bevy(forest.clone());
    }

    if let Some(output_path) = cli.output {
        export_to_obj(&forest, &output_path)?;
        println!("Exported .obj to {}", output_path);
    }

    // ✅ Comment out (the conventional flow)
    // if cli.preview {
    //     view_voxels_bevy(scene.clone());
    // }

    // if let Some(output_path) = cli.output {
    //     export_to_obj(&scene, &output_path)?;
    //     println!("Exported .obj to {}", output_path);
    // }




    Ok(())
}
