use clap::Parser;

mod parser;
use parser::*;

mod types;
// use types::*;

mod export;
use export::*;

mod viewer;
use viewer::*;

mod colors;


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

    let scene = parse_mochi_file(&cli.input)?;

    println!("Parsed {} voxels:", scene.voxels.len());

    if let Some(output_path) = &cli.output {
        export_to_obj(&scene, &output_path)?;
        println!("Exported .obj to {}", output_path);
    } else {
        for v in &scene.voxels {
            println!("({}, {}, {}) -> {}", v.x, v.y, v.z, v.color);
        }
    }

    if cli.preview {
        view_voxels(&scene)?;
    } else if let Some(output_path) = &cli.output {
        export_to_obj(&scene, &output_path)?;
        println!("Exported .obj to {}", output_path);
    } else {
        for v in &scene.voxels {
            println!("({}, {}, {}) -> {}", v.x, v.y, v.z, v.color);
        }
    }


    Ok(())
}
