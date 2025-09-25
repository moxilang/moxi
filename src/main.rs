use clap::Parser;

mod types;
mod export;
mod bevy_viewer;
mod colors;

mod mochi;      // new DSL
mod legacy;     // optional, not default

/// Mochi: Build with squish. Render with rage.
#[derive(Parser)]
#[command(name = "mochi")]
#[command(about = "Voxel programming language & CLI", long_about = None)]
struct Cli {
    /// Input .mochi script
    #[arg(short, long)]
    input: String,

    /// Optional legacy parser flag
    #[arg(long)]
    legacy: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let source = std::fs::read_to_string(&cli.input)?;

    if cli.legacy {
        // Optional legacy mode
        let scene = legacy::voxgrid::parse_voxgrid_file(&cli.input)?;
        println!("(LEGACY) Parsed {} voxels", scene.voxels.len());
        return Ok(());
    }

    // Default: run through Mochi DSL
    let tokens = mochi::lexer::lex(&source);
    let ast = mochi::parser::parse(tokens);
    mochi::runtime::run(ast);

    Ok(())
}
