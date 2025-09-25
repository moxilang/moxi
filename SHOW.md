# ✨ Project Snapshot: src

*Made with [Yggdrasil](https://crates.io/crates/yggdrasil)*  

*This document contains two sections:*
- **Files** → index of all paths.
- **File Contents** → full text for each file under `### <path>`.

## 📄 Files
- [src/colors.rs (17 lines)](#src-colors-rs)
- [src/export.rs (71 lines)](#src-export-rs)
- [src/bevy_viewer.rs (147 lines)](#src-bevy_viewer-rs)
- [src/parser.rs (79 lines)](#src-parser-rs)
- [src/viewer.rs (67 lines)](#src-viewer-rs)
- [src/main.rs (67 lines)](#src-main-rs)
- [src/types.rs (22 lines)](#src-types-rs)

====
- [📦 Total LOC (470 lines)](#)


---

## 📑 File Contents

<a id="src-colors-rs"></a>
### <src/colors.rs>
```
use std::collections::HashMap;

/// Returns a HashMap of built-in color names to hex values.
pub fn default_colors() -> HashMap<String, String> {
    HashMap::from([
        ("red".to_string(), "#ff0000".to_string()),
        ("orange".to_string(), "#ffa500".to_string()),
        ("yellow".to_string(), "#ffff00".to_string()),
        ("green".to_string(), "#00ff00".to_string()),
        ("blue".to_string(), "#0000ff".to_string()),
        ("purple".to_string(), "#800080".to_string()),
        ("white".to_string(), "#ffffff".to_string()),
        ("black".to_string(), "#000000".to_string()),
        ("brown".to_string(), "#8b4513".to_string()),
        ("mochi-pink".to_string(), "#fcb7b7".to_string()),
    ])
}
```

<a id="src-export-rs"></a>
### <src/export.rs>
```
use crate::types::*;
use std::fs::File;
use std::io::{BufWriter, Write};

use std::path::Path;

pub fn export_to_obj(scene: &VoxelScene, path: &str) -> anyhow::Result<()> {
    // Create parent directory if needed
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut file = BufWriter::new(File::create(path)?);
    let mut vertex_offset = 1;

    for voxel in &scene.voxels {
        let x = voxel.x as f32;
        let y = voxel.y as f32;
        let z = voxel.z as f32;

        let cube = generate_cube_vertices(x, y, z);

        for (vx, vy, vz) in &cube.vertices {
            writeln!(file, "v {} {} {}", vx, vy, vz)?;
        }

        for face in &cube.faces {
            writeln!(
                file,
                "f {} {} {} {}",
                vertex_offset + face[0],
                vertex_offset + face[1],
                vertex_offset + face[2],
                vertex_offset + face[3],
            )?;
        }

        vertex_offset += 8;
    }

    Ok(())
}

struct Cube {
    vertices: Vec<(f32, f32, f32)>,
    faces: Vec<[usize; 4]>,
}

fn generate_cube_vertices(x: f32, y: f32, z: f32) -> Cube {
    let vertices = vec![
        (x, y, z),
        (x + 1.0, y, z),
        (x + 1.0, y + 1.0, z),
        (x, y + 1.0, z),
        (x, y, z + 1.0),
        (x + 1.0, y, z + 1.0),
        (x + 1.0, y + 1.0, z + 1.0),
        (x, y + 1.0, z + 1.0),
    ];

    let faces = vec![
        [0, 1, 2, 3], // bottom
        [4, 5, 6, 7], // top
        [0, 1, 5, 4], // front
        [2, 3, 7, 6], // back
        [1, 2, 6, 5], // right
        [0, 3, 7, 4], // left
    ];

    Cube { vertices, faces }
}
```

<a id="src-bevy_viewer-rs"></a>
### <src/bevy_viewer.rs>
```
use bevy::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::input::ButtonInput;
use bevy::math::primitives::Cuboid;
use bevy::window::PresentMode;
use crate::types::VoxelScene;

#[derive(Component)]
struct OrbitCamera;

#[derive(Resource)]
struct CameraController {
    pub radius: f32,
    pub yaw: f32,
    pub pitch: f32,
    // pub rotating: bool,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            radius: 8.0,
            yaw: 0.5,
            pitch: 0.5,
            // rotating: false,
        }
    }
}



pub fn view_voxels_bevy(scene: VoxelScene) {
    App::new()
        .insert_resource(scene)
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "MochiVox 3D Preview".into(),
                resolution: (800., 600.).into(),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, (setup_camera, spawn_voxels))
        .add_systems(Update, orbit_camera_system)

        .run();
}


fn setup_camera(mut commands: Commands) {
    commands.insert_resource(CameraController::default());

    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 0.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        OrbitCamera,
    ));
}

fn orbit_camera_system(
    mut mouse_evr: EventReader<MouseMotion>,
    mut scroll_evr: EventReader<MouseWheel>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut controller: ResMut<CameraController>,
    mut query: Query<&mut Transform, With<OrbitCamera>>,
) {
    let mut delta = Vec2::ZERO;

    // Rotate only when holding left mouse
    if buttons.pressed(MouseButton::Left) {
        for ev in mouse_evr.read() {
            delta += ev.delta;
        }
    }

    // Sensitivity tweak (smaller = smoother)
    let sensitivity = 0.005;
    controller.yaw += delta.x * sensitivity;
    controller.pitch += delta.y * sensitivity; // 👈 invert to match OrbitControls feel

    // Clamp pitch (avoid flipping upside down)
    let max_pitch = std::f32::consts::FRAC_PI_2 - 0.05; // ~89°
    let min_pitch = -max_pitch;
    controller.pitch = controller.pitch.clamp(min_pitch, max_pitch);

    // Zoom (scroll wheel)
    for ev in scroll_evr.read() {
        controller.radius -= ev.y * 0.5;
        controller.radius = controller.radius.clamp(2.0, 50.0);
    }

    // Convert spherical coords to Cartesian
    let x = controller.radius * controller.yaw.cos() * controller.pitch.cos();
    let y = controller.radius * controller.pitch.sin();
    let z = controller.radius * controller.yaw.sin() * controller.pitch.cos();

    for mut transform in query.iter_mut() {
        transform.translation = Vec3::new(x, y, z);
        transform.look_at(Vec3::ZERO, Vec3::Y);
    }
}



fn spawn_voxels(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    scene: Res<VoxelScene>,
) {
    for voxel in &scene.voxels {
        let color = parse_color(&voxel.color);
        let material = materials.add(StandardMaterial {
            base_color: color,
            unlit: true,   // 👈 makes it ignore lighting
            ..default()
        });

        commands.spawn(PbrBundle {
            mesh: meshes.add(Mesh::from(Cuboid {
                half_size: Vec3::splat(0.5),
            })),
            material,
            transform: Transform::from_xyz(
                voxel.x as f32,  // keep X as-is
                voxel.z as f32,  // map Z → Y (depth)
                voxel.y as f32,  // map Y → Z (up)
            ),

            ..default()
        });
    }
}

fn parse_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Color::rgb_u8(255, 0, 255); // hot pink for failure
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    Color::rgb_u8(r, g, b)
}
```

<a id="src-parser-rs"></a>
### <src/parser.rs>
```
use crate::types::*;
use crate::colors::default_colors;
use std::collections::HashMap;
use std::fs;


pub fn parse_mochi_file(path: &str) -> anyhow::Result<VoxelScene> {
    let contents = fs::read_to_string(path)?;
    let mut voxels = Vec::new();
    let mut color_map: ColorMap = HashMap::new();
    let mut layers: Vec<Vec<String>> = Vec::new();
    let mut current_layer: Vec<String> = Vec::new();
    let mut _z = 0;
    let mut in_colors = false;

    for line in contents.lines() {
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        if line.starts_with("[Layer") {
            if !current_layer.is_empty() {
                layers.push(current_layer);
                current_layer = Vec::new();
                _z += 1;
            }
            in_colors = false;
        } else if line.starts_with("[Colors]") {
            if !current_layer.is_empty() {
                layers.push(current_layer.clone());
            }
            in_colors = true;
        } else if in_colors {
            if let Some((k, v)) = line.split_once(':') {
                color_map.insert(k.trim().to_string(), v.trim().to_string());
            }
        } else {
            current_layer.push(line.to_string());
        }
    }

    if !current_layer.is_empty() {
        layers.push(current_layer.clone());
    }

    // Convert layers to voxels
    for (z_idx, layer) in layers.iter().enumerate() {
        for (y, row) in layer.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                if ch == '.' || ch == ' ' {
                    continue;
                }
                let key = ch.to_string();

                let fallback_colors = default_colors();
                let raw_color = color_map.get(&key)
                    .or_else(|| fallback_colors.get(&key))
                    .cloned()
                    .unwrap_or_else(|| "#888888".to_string());

                // If it's a name like "green", map it again through fallback_colors
                let color = fallback_colors.get(&raw_color).cloned().unwrap_or(raw_color);



                voxels.push(Voxel {
                    x,
                    y,
                    z: z_idx,
                    color,
                });
            }
        }
    }

    Ok(VoxelScene { voxels })
}
```

<a id="src-viewer-rs"></a>
### <src/viewer.rs>
```
use crate::types::*;
use minifb::{Key, Window, WindowOptions};

const WIDTH: usize = 640;
const HEIGHT: usize = 480;
const TILE_WIDTH: usize = 32;
const TILE_HEIGHT: usize = 16;

pub fn view_voxels(scene: &VoxelScene) -> anyhow::Result<()> {
    let mut buffer = vec![0u32; WIDTH * HEIGHT];
    let mut window = Window::new("MochiVox Viewer - ESC to exit", WIDTH, HEIGHT, WindowOptions::default())?;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        buffer.fill(0x000000); // clear

        const TILE_WIDTH: usize = 32;
        const TILE_HEIGHT: usize = 16;
        let center_x = WIDTH / 2;
        let center_y = HEIGHT / 4;

        // Clone and sort
        let mut sorted_voxels = scene.voxels.clone();
        sorted_voxels.sort_by_key(|v| v.x + v.y + v.z);

        for voxel in &sorted_voxels {
            let x = voxel.x as isize;
            let y = voxel.y as isize;
            let z = voxel.z as isize;

            let screen_x = ((x - z) * TILE_WIDTH as isize / 2 + center_x as isize) as usize;
            let screen_y = ((x + z) * TILE_HEIGHT as isize / 2 - y * TILE_HEIGHT as isize + center_y as isize) as usize;

            println!("Drawing voxel with color: {}", voxel.color);
            draw_rect(&mut buffer, screen_x, screen_y, TILE_WIDTH, TILE_HEIGHT, parse_color(&voxel.color));
        }

        window.update_with_buffer(&buffer, WIDTH, HEIGHT)?;
    }

    Ok(())
}

fn draw_rect(buffer: &mut [u32], x: usize, y: usize, w: usize, h: usize, color: u32) {
    for dy in 0..h {
        for dx in 0..w {
            let px = x + dx;
            let py = y + dy;
            if px < WIDTH && py < HEIGHT {
                buffer[py * WIDTH + px] = color;
            }
        }
    }
}

fn parse_color(hex: &str) -> u32 {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        println!("⚠️ Bad color input: '{}'", hex);
        return 0xff00ff; // hot pink for error, why not
    }

    let r = u32::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u32::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u32::from_str_radix(&hex[4..6], 16).unwrap_or(255);

    (r << 16) | (g << 8) | b
}
```

<a id="src-main-rs"></a>
### <src/main.rs>
```
use clap::Parser;

mod parser;
use parser::*;

mod types;
// use types::*;

mod export;
use export::*;

mod bevy_viewer;
use bevy_viewer::*;

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
        view_voxels_bevy(scene.clone()); // pass clone since it consumes
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
```

<a id="src-types-rs"></a>
### <src/types.rs>
```
use std::collections::HashMap;

/// Represents a single voxel with a position and a color.
#[derive(Debug, Clone)]
pub struct Voxel {
    pub x: usize,
    pub y: usize,
    pub z: usize,
    pub color: String,
}

/// Holds the entire voxel scene.
use bevy::prelude::Resource;

#[derive(Debug, Clone, Resource)]
pub struct VoxelScene {
    pub voxels: Vec<Voxel>,
}


/// Map symbol to color name or hex
pub type ColorMap = HashMap<String, String>;
```

