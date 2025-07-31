use bevy::prelude::*;
use crate::types::*;
use bevy::window::PresentMode;
use bevy::math::primitives::Cuboid;

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
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(10.0, 10.0, 20.0)
            .looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y),
        ..default()
    });
    commands.spawn(PointLightBundle {
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });
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
            ..default()
        });

        commands.spawn(PbrBundle {
            mesh: meshes.add(Mesh::from(Cuboid {
                half_size: Vec3::splat(0.5),
            })),
            material,
            transform: Transform::from_xyz(
                voxel.x as f32,
                voxel.y as f32,
                voxel.z as f32,
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
