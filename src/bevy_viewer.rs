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
            unlit: true,   // 👈 makes it ignore lighting, THIS is what we want 
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
