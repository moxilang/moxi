use crate::types::*;
use minifb::{Key, Window, WindowOptions};

const WIDTH: usize = 640;
const HEIGHT: usize = 480;
const SCALE: usize = 20;

pub fn view_voxels(scene: &VoxelScene) -> anyhow::Result<()> {
    let mut buffer = vec![0u32; WIDTH * HEIGHT];
    let mut window = Window::new("MochiVox Viewer - ESC to exit", WIDTH, HEIGHT, WindowOptions::default())?;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        buffer.fill(0x000000); // clear

        for voxel in &scene.voxels {
            let screen_x = voxel.x * SCALE + 100;
            let screen_y = (voxel.y + voxel.z) * SCALE + 100;

            println!("Drawing voxel with color: {}", voxel.color);
            draw_rect(&mut buffer, screen_x, screen_y, SCALE, SCALE, parse_color(&voxel.color));
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
