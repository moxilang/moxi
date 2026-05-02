// src/bevy_viewer.rs
// Requires the `viewer` feature:  cargo run --features viewer
//
// On your machine:  cargo run --features viewer
// In the sandbox (Rust 1.75): cargo build  (Bevy excluded)

#[cfg(feature = "viewer")]
mod inner {
    use bevy::input::mouse::{MouseMotion, MouseWheel};
    use bevy::input::ButtonInput;
    use bevy::math::primitives::Cuboid;
    use bevy::prelude::*;
    use bevy::window::PresentMode;
    use crate::types::VoxelScene;

    #[derive(Component)]
    struct OrbitCamera;

    #[derive(Resource)]
    struct CameraController {
        pub radius: f32,
        pub yaw:    f32,
        pub pitch:  f32,
    }

    impl Default for CameraController {
        fn default() -> Self {
            Self { radius: 8.0, yaw: 0.5, pitch: 0.5 }
        }
    }

    pub fn view_voxels_bevy(scene: VoxelScene) {
        App::new()
            .insert_resource(scene)
            .add_plugins(DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Moxi 3D Preview".into(),
                    resolution: (1024., 768.).into(),
                    present_mode: PresentMode::AutoVsync,
                    ..default()
                }),
                ..default()
            }))
            .add_systems(Startup, (setup_camera, spawn_voxels))
            .add_systems(Update, orbit_camera_system)
            .run();
    }

    fn setup_camera(mut commands: Commands, scene: Res<VoxelScene>) {
        let center = scene.center();
        let radius = (scene.max_dim() * 2.0).max(10.0);

        commands.insert_resource(CameraController { radius, yaw: 0.5, pitch: 0.5 });

        commands.spawn(DirectionalLightBundle {
            directional_light: DirectionalLight { illuminance: 10_000.0, ..default() },
            transform: Transform::from_xyz(1.0, 2.0, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        });

        commands.spawn((
            Camera3dBundle {
                transform: Transform::from_xyz(0.0, radius * 0.5, radius)
                    .looking_at(center, Vec3::Y),
                ..default()
            },
            OrbitCamera,
        ));
    }

    fn spawn_voxels(
        mut commands:  Commands,
        mut meshes:    ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        scene:         Res<VoxelScene>,
    ) {
        let cube_mesh = meshes.add(Mesh::from(Cuboid { half_size: Vec3::splat(0.5) }));

        for voxel in &scene.voxels {
            let color    = parse_hex_color(&voxel.color);
            let material = materials.add(StandardMaterial {
                base_color: color,
                unlit: true,
                ..default()
            });
            commands.spawn(PbrBundle {
                mesh: cube_mesh.clone(),
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

    fn orbit_camera_system(
        mut mouse_evr:  EventReader<MouseMotion>,
        mut scroll_evr: EventReader<MouseWheel>,
        buttons:        Res<ButtonInput<MouseButton>>,
        mut controller: ResMut<CameraController>,
        mut query:      Query<&mut Transform, With<OrbitCamera>>,
    ) {
        let mut delta = Vec2::ZERO;
        if buttons.pressed(MouseButton::Left) || buttons.pressed(MouseButton::Right) {
            for ev in mouse_evr.read() { delta += ev.delta; }
        }
        controller.yaw   += delta.x * 0.005;
        controller.pitch += delta.y * 0.005;
        let max_pitch = std::f32::consts::FRAC_PI_2 - 0.05;
        controller.pitch = controller.pitch.clamp(-max_pitch, max_pitch);
        for ev in scroll_evr.read() {
            controller.radius -= ev.y * 0.5;
            controller.radius  = controller.radius.clamp(2.0, 200.0);
        }
        let x = controller.radius * controller.yaw.cos() * controller.pitch.cos();
        let y = controller.radius * controller.pitch.sin();
        let z = controller.radius * controller.yaw.sin() * controller.pitch.cos();
        for mut t in query.iter_mut() {
            t.translation = Vec3::new(x, y, z);
            t.look_at(Vec3::ZERO, Vec3::Y);
        }
    }

    fn parse_hex_color(hex: &str) -> Color {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 { return Color::rgb_u8(255, 0, 255); }
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
        Color::rgb_u8(r, g, b)
    }
}

// Public re-export so callers don't need to know about the feature flag
#[cfg(feature = "viewer")]
pub use inner::view_voxels_bevy;

// Stub when viewer feature is off — lets main.rs compile unconditionally
#[cfg(not(feature = "viewer"))]
pub fn view_voxels_bevy(_scene: crate::types::VoxelScene) {
    eprintln!("viewer feature not enabled — rebuild with: cargo run --features viewer");
}