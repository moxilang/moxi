use crate::types::{Voxel, VoxelScene};

/// Clone a voxel scene (deep copy)
pub fn clone_scene(scene: &VoxelScene) -> VoxelScene {
    VoxelScene { voxels: scene.voxels.clone() }
}

/// Built-in Moxi commands
pub fn list_commands() -> Vec<&'static str> {
    vec!["print", "clone", "translate", "rotate", "merge", "grid"]
}

pub fn do_print(scene: &VoxelScene) {
    println!("Scene has {} voxels", scene.voxels.len());
}

pub fn do_clone(scene: &mut VoxelScene) -> usize {
    let copy = clone_scene(scene);
    let size = copy.voxels.len();
    scene.voxels.extend(copy.voxels);
    println!(
        "Cloned scene: added {} voxels, now {} total",
        size, scene.voxels.len()
    );
    size
}

pub fn do_translate(scene: &mut VoxelScene, args: &[String], count: usize) {
    if args.len() < 3 { return; }

    let dx: i32 = args[0].parse().unwrap_or(0);
    let dy: i32 = args[1].parse().unwrap_or(0);
    let dz: i32 = args[2].parse().unwrap_or(0);

    let n = scene.voxels.len();
    let start = n.saturating_sub(count);

    for v in &mut scene.voxels[start..] {
        v.x += dx;
        v.y += dy;
        v.z += dz;
    }

    println!("Translated last {} voxels by ({}, {}, {})", count, dx, dy, dz);
}

pub fn do_rotate(scene: &mut VoxelScene, args: &[String], count: usize) {
    if args.len() < 2 { return; }

    let axis = args[0].as_str();
    let turns: i32 = args[1].parse().unwrap_or(1);

    let n = scene.voxels.len();
    let start = n.saturating_sub(count);

    let rotated: Vec<_> = scene.voxels[start..]
        .iter()
        .map(|v| {
            let (mut x, mut y, mut z) = (v.x, v.y, v.z);
            for _ in 0..((turns % 4 + 4) % 4) {
                match axis {
                    "x" => { let ny = -z; let nz = y; y = ny; z = nz; }
                    "y" => { let nx = z; let nz = -x; x = nx; z = nz; }
                    "z" => { let nx = -y; let ny = x; x = nx; y = ny; }
                    _ => {}
                }
            }
            Voxel { x, y, z, color: v.color.clone() }
        })
        .collect();

    scene.voxels.truncate(start);
    scene.voxels.extend(rotated);
    scene.normalize();

    println!(
        "Rotated last {} voxels around {} by {} quarter turns",
        count, axis, turns
    );
}

pub fn do_merge(scene: &VoxelScene) {
    println!("Merge: scene has {} voxels", scene.voxels.len());
}
