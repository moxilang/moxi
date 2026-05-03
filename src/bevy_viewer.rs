// src/bevy_viewer.rs
// Requires the `viewer` feature:  cargo run --features viewer
//
// Performance: all voxels are merged into a single mesh per color group.
// This reduces draw calls from N_voxels to N_colors — typically 2-6.
//
// Shading: unlit removed, normals generated, directional light added.
// Middle-mouse or right-mouse to orbit, scroll to zoom, middle-drag to pan.

#[cfg(feature = "viewer")]
mod inner {
    use bevy::input::mouse::{MouseMotion, MouseWheel};
    use bevy::input::ButtonInput;
    use bevy::prelude::*;
    use bevy::render::mesh::{Mesh, PrimitiveTopology};
    use bevy::render::render_asset::RenderAssetUsages;
    use bevy::window::PresentMode;
    use std::collections::HashMap;

    use crate::types::VoxelScene;

    #[derive(Component)]
    struct OrbitCamera;

    #[derive(Resource)]
    struct CameraController {
        pub radius: f32,
        pub yaw:    f32,
        pub pitch:  f32,
        pub target: Vec3,
    }

    impl Default for CameraController {
        fn default() -> Self {
            Self { radius: 80.0, yaw: 0.5, pitch: 0.4, target: Vec3::ZERO }
        }
    }

    // ── Entry point ────────────────────────────────────────────────────────

    pub fn view_voxels_bevy(scene: VoxelScene) {
        App::new()
            .insert_resource(scene)
            .add_plugins(DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Moxi 3D Preview".into(),
                    resolution: (1280., 800.).into(),
                    present_mode: PresentMode::AutoVsync,
                    ..default()
                }),
                ..default()
            }))
            .add_systems(Startup, (setup_scene, spawn_merged_meshes))
            .add_systems(Update, orbit_camera_system)
            .run();
    }

    // ── Setup ──────────────────────────────────────────────────────────────

    fn setup_scene(mut commands: Commands, scene: Res<VoxelScene>) {
        let center = scene.center();
        let radius = (scene.max_dim() * 1.5).max(30.0);

        commands.insert_resource(CameraController {
            radius,
            yaw:    0.5,
            pitch:  0.4,
            target: center,
        });

        // Sun — directional light from upper-left gives nice depth shading
        commands.spawn(DirectionalLightBundle {
            directional_light: DirectionalLight {
                illuminance: 15_000.0,
                shadows_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(-1.0, 2.0, 1.0)
                .looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        });

        // Soft fill light from opposite side
        commands.spawn(DirectionalLightBundle {
            directional_light: DirectionalLight {
                illuminance: 4_000.0,
                shadows_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(1.0, 0.5, -1.0)
                .looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        });

        // Camera
        commands.spawn((
            Camera3dBundle {
                transform: Transform::from_xyz(
                    center.x,
                    center.y + radius * 0.5,
                    center.z + radius,
                ).looking_at(center, Vec3::Y),
                ..default()
            },
            OrbitCamera,
        ));
    }

    // ── Mesh builder ───────────────────────────────────────────────────────
    //
    // Group voxels by color, build one merged mesh per color group.
    // A mesh with 10,000 voxels = 60,000 quads but still ONE draw call.

    fn spawn_merged_meshes(
        mut commands:  Commands,
        mut meshes:    ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        scene:         Res<VoxelScene>,
    ) {
        // Group voxel positions by hex color
        let mut by_color: HashMap<String, Vec<Vec3>> = HashMap::new();
        for voxel in &scene.voxels {
            by_color
                .entry(voxel.color.clone())
                .or_default()
                .push(Vec3::new(voxel.x as f32, voxel.y as f32, voxel.z as f32));
        }

        for (hex, positions) in &by_color {
            let mesh   = build_voxel_mesh(positions);
            let color  = parse_hex_color(hex);

            let handle = meshes.add(mesh);
            let mat    = materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.85,
                metallic: 0.0,
                ..default()
            });

            commands.spawn(PbrBundle {
                mesh: handle,
                material: mat,
                ..default()
            });
        }

        println!("  spawned {} color groups (draw calls)", by_color.len());
    }

    /// Build a single merged Mesh from a list of voxel center positions.
    /// Each voxel contributes 6 faces × 4 vertices = 24 vertices,
    /// and 6 faces × 2 triangles × 3 indices = 36 indices.
    fn build_voxel_mesh(positions: &[Vec3]) -> Mesh {
        let mut verts:   Vec<[f32; 3]> = Vec::with_capacity(positions.len() * 24);
        let mut normals: Vec<[f32; 3]> = Vec::with_capacity(positions.len() * 24);
        let mut indices: Vec<u32>      = Vec::with_capacity(positions.len() * 36);

        // 6 faces of a unit cube, vertices CCW when viewed from outside.
        // Bevy uses right-handed Y-up. For each face we list 4 corners
        // in counter-clockwise order as seen from the outward normal direction.
        const FACES: [([f32;3], [[f32;3];4]); 6] = [
            // +X  viewed from +X looking toward -X
            ([ 1., 0., 0.], [[ 1.,-1., 1.],[ 1.,-1.,-1.],[ 1., 1.,-1.],[ 1., 1., 1.]]),
            // -X  viewed from -X looking toward +X
            ([-1., 0., 0.], [[-1.,-1.,-1.],[-1.,-1., 1.],[-1., 1., 1.],[-1., 1.,-1.]]),
            // +Y  viewed from +Y looking toward -Y
            ([ 0., 1., 0.], [[-1., 1., 1.],[ 1., 1., 1.],[ 1., 1.,-1.],[-1., 1.,-1.]]),
            // -Y  viewed from -Y looking toward +Y
            ([ 0.,-1., 0.], [[-1.,-1.,-1.],[ 1.,-1.,-1.],[ 1.,-1., 1.],[-1.,-1., 1.]]),
            // +Z  viewed from +Z looking toward -Z
            ([ 0., 0., 1.], [[-1.,-1., 1.],[ 1.,-1., 1.],[ 1., 1., 1.],[-1., 1., 1.]]),
            // -Z  viewed from -Z looking toward +Z
            ([ 0., 0.,-1.], [[ 1.,-1.,-1.],[-1.,-1.,-1.],[-1., 1.,-1.],[ 1., 1.,-1.]]),
        ];

        for &pos in positions {
            let base = verts.len() as u32;

            for (normal, corners) in &FACES {
                let face_base = verts.len() as u32;

                for corner in corners {
                    verts.push([
                        pos.x + corner[0] * 0.5,
                        pos.y + corner[1] * 0.5,
                        pos.z + corner[2] * 0.5,
                    ]);
                    normals.push(*normal);
                }

                // Two triangles per face (quad split)
                indices.extend_from_slice(&[
                    face_base,     face_base + 1, face_base + 2,
                    face_base,     face_base + 2, face_base + 3,
                ]);
            }

            let _ = base; // suppress warning
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, verts);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL,   normals);
        mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));
        mesh
    }

    // ── Camera system ──────────────────────────────────────────────────────

    fn orbit_camera_system(
        mut mouse_evr:  EventReader<MouseMotion>,
        mut scroll_evr: EventReader<MouseWheel>,
        keys:           Res<ButtonInput<KeyCode>>,
        buttons:        Res<ButtonInput<MouseButton>>,
        mut controller: ResMut<CameraController>,
        mut query:      Query<&mut Transform, With<OrbitCamera>>,
    ) {
        let mut orbit = Vec2::ZERO;
        let mut pan   = Vec2::ZERO;

        for ev in mouse_evr.read() {
            if buttons.pressed(MouseButton::Left) || buttons.pressed(MouseButton::Right) {
                orbit += ev.delta;
            }
            if buttons.pressed(MouseButton::Middle) {
                pan += ev.delta;
            }
        }

        // Keyboard pan — works on trackpad too
        let pan_speed = controller.radius * 0.02;
        if keys.pressed(KeyCode::ArrowLeft)  || keys.pressed(KeyCode::KeyA) {
            let right = Vec3::new(controller.yaw.sin(), 0.0, -controller.yaw.cos());
            controller.target -= right * pan_speed;
        }
        if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
            let right = Vec3::new(controller.yaw.sin(), 0.0, -controller.yaw.cos());
            controller.target += right * pan_speed;
        }
        if keys.pressed(KeyCode::ArrowUp)    || keys.pressed(KeyCode::KeyW) {
            controller.target.y += pan_speed;
        }
        if keys.pressed(KeyCode::ArrowDown)  || keys.pressed(KeyCode::KeyS) {
            controller.target.y -= pan_speed;
        }

        // Mouse pan
        if pan.length_squared() > 0.0 {
            let right = Vec3::new(controller.yaw.sin(), 0.0, -controller.yaw.cos());
            controller.target -= right * pan.x * pan_speed * 0.1;
            controller.target += Vec3::Y * pan.y * pan_speed * 0.1;
        }

        // Orbit
        controller.yaw   += orbit.x * 0.005;
        controller.pitch += orbit.y * 0.005;
        let max_pitch = std::f32::consts::FRAC_PI_2 - 0.05;
        controller.pitch = controller.pitch.clamp(-max_pitch, max_pitch);

        // Zoom
        for ev in scroll_evr.read() {
            controller.radius -= ev.y * controller.radius * 0.08;
            controller.radius  = controller.radius.clamp(2.0, 500.0);
        }

        // Spherical → Cartesian
        let x = controller.radius * controller.yaw.cos() * controller.pitch.cos();
        let y = controller.radius * controller.pitch.sin();
        let z = controller.radius * controller.yaw.sin() * controller.pitch.cos();

        for mut t in query.iter_mut() {
            t.translation = controller.target + Vec3::new(x, y, z);
            t.look_at(controller.target, Vec3::Y);
        }
    }

    // ── Color helpers ──────────────────────────────────────────────────────

    fn parse_hex_color(hex: &str) -> Color {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 { return Color::rgb_u8(255, 0, 255); }
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
        Color::rgb_u8(r, g, b)
    }
}

#[cfg(feature = "viewer")]
pub use inner::view_voxels_bevy;

#[cfg(not(feature = "viewer"))]
pub fn view_voxels_bevy(_scene: crate::types::VoxelScene) {
    eprintln!("viewer feature not enabled — rebuild with: cargo run --features viewer");
}