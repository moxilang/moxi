// src/bevy_viewer.rs
// Requires the `viewer` feature:  cargo run --features viewer
//
// The viewer draws the SCENE, not a voxel grid. A sphere in Moxi is a
// sphere here: one primitive mesh with the part's solved frame as its
// Transform. Detail is no longer capped by voxel size.
//
// Shapes with no closed form — blob and heightfield (noise-defined), cone
// (no Bevy 0.13 primitive), and every CSG compound — fall back to voxel
// sampling FOR THAT PART. Sampled parts of one layer are merged with
// declaration-order overwrite, matching `rasterize_entity`.
//
// Parts are drawn at their solved world frames, full stop. The viewer
// applies no offsets: a scene is a thing whose parts are placed by
// relation, so the relations decide where everything sits. The three
// centering conventions this file used to reproduce — centering in x/z,
// sinking non-heightfield layers below y=0, and lifting each print layer
// to break coplanar z-fighting — existed only because separate prints had
// nothing relating them, and they are gone.

#[cfg(feature = "viewer")]
mod inner {
    use bevy::input::mouse::{MouseMotion, MouseWheel};
    use bevy::input::ButtonInput;
    use bevy::prelude::*;
    use bevy::render::mesh::{Mesh, PrimitiveTopology};
    use bevy::render::render_asset::RenderAssetUsages;
    use bevy::window::PresentMode;
    use std::collections::HashMap;

    use crate::anchors::analytic_extents;
    use crate::ast::ShapeExpr;
    use crate::frame::{Frame, Vec3 as MVec3};
    use crate::scene::{Scene, Shape};

    #[derive(Component)]
    struct OrbitCamera;

    #[derive(Resource)]
    struct SceneRes(Scene);

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

    // ── Entry point ────────────────────────────────────────────────────

    pub fn view_scene_bevy(scene: Scene) {
        App::new()
            .insert_resource(SceneRes(scene))
            .add_plugins(DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Moxi 3D Preview".into(),
                    resolution: (1280., 800.).into(),
                    present_mode: PresentMode::AutoVsync,
                    ..default()
                }),
                ..default()
            }))
            .add_systems(Startup, (setup_scene, spawn_parts).chain())
            .add_systems(Update, orbit_camera_system)
            .run();
    }

    // ── Analytic bounds ────────────────────────────────────────────────

    /// World AABB of a shape under a frame: its analytic extents' eight
    /// corners transformed. Used for camera framing, layer offsets, and the
    /// sampler's region — never a voxel grid.
    fn world_corners(shape: &ShapeExpr, frame: &Frame) -> (MVec3, MVec3) {
        let e = analytic_extents(shape);
        let mut min = MVec3::new(f64::MAX, f64::MAX, f64::MAX);
        let mut max = MVec3::new(f64::MIN, f64::MIN, f64::MIN);
        for &cx in &[e.min.x, e.max.x] {
            for &cy in &[e.min.y, e.max.y] {
                for &cz in &[e.min.z, e.max.z] {
                    let p = frame.apply_point(MVec3::new(cx, cy, cz));
                    min = MVec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
                    max = MVec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
                }
            }
        }
        (min, max)
    }

    fn merge((a_min, a_max): (MVec3, MVec3), (b_min, b_max): (MVec3, MVec3)) -> (MVec3, MVec3) {
        (
            MVec3::new(a_min.x.min(b_min.x), a_min.y.min(b_min.y), a_min.z.min(b_min.z)),
            MVec3::new(a_max.x.max(b_max.x), a_max.y.max(b_max.y), a_max.z.max(b_max.z)),
        )
    }

    const EMPTY: (MVec3, MVec3) = (
        MVec3 { x: f64::MAX, y: f64::MAX, z: f64::MAX },
        MVec3 { x: f64::MIN, y: f64::MIN, z: f64::MIN },
    );

    fn scene_bounds(scene: &Scene) -> (MVec3, MVec3) {
        let mut acc = EMPTY;
        for layer in scene.layers.iter() {
            for p in &layer.parts {
                acc = merge(acc, world_corners(&p.shape.to_expr(), &p.frame.to_frame()));
            }
        }
        if acc.0.x == f64::MAX { (MVec3::ZERO, MVec3::ZERO) } else { acc }
    }

    fn setup_scene(mut commands: Commands, scene: Res<SceneRes>) {
        let (min, max) = scene_bounds(&scene.0);
        let center = Vec3::new(
            ((min.x + max.x) * 0.5) as f32,
            ((min.y + max.y) * 0.5) as f32,
            ((min.z + max.z) * 0.5) as f32,
        );
        let span = ((max.x - min.x).max(max.y - min.y).max(max.z - min.z)) as f32;
        let radius = (span * 1.5).max(30.0);

        commands.insert_resource(CameraController {
            radius,
            yaw:    0.5,
            pitch:  0.4,
            target: center,
        });

        commands.spawn(DirectionalLightBundle {
            directional_light: DirectionalLight {
                illuminance: 15_000.0,
                shadows_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(-1.0, 2.0, 1.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        });

        commands.spawn(DirectionalLightBundle {
            directional_light: DirectionalLight {
                illuminance: 4_000.0,
                shadows_enabled: false,
                ..default()
            },
            transform: Transform::from_xyz(1.0, 0.5, -1.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        });

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

    // ── Frame → Transform ──────────────────────────────────────────────

    /// Moxi's `Mat3` is row-major (`m.0[row][col]`); Bevy's `Mat3` is built
    /// from COLUMNS. Column i is (m[0][i], m[1][i], m[2][i]). Getting this
    /// backwards transposes every rotation, which looks plausible on
    /// symmetric parts and wrong on everything else — hence one conversion,
    /// in one place.
    fn transform_of(f: &Frame, local_offset: Vec3, scale: Vec3) -> Transform {
        let m = f.rot.0;
        let basis = Mat3::from_cols(
            Vec3::new(m[0][0] as f32, m[1][0] as f32, m[2][0] as f32),
            Vec3::new(m[0][1] as f32, m[1][1] as f32, m[2][1] as f32),
            Vec3::new(m[0][2] as f32, m[1][2] as f32, m[2][2] as f32),
        );
        let pos = Vec3::new(f.pos.x as f32, f.pos.y as f32, f.pos.z as f32);

        Transform {
            translation: pos + basis * local_offset,
            rotation:    Quat::from_mat3(&basis),
            scale,
        }
    }

    // ── Spawning ───────────────────────────────────────────────────────

    fn spawn_parts(
        mut commands:  Commands,
        mut meshes:    ResMut<Assets<Mesh>>,
        mut materials: ResMut<Assets<StandardMaterial>>,
        scene:         Res<SceneRes>,
    ) {
        let mut mat_cache: HashMap<String, Handle<StandardMaterial>> = HashMap::new();
        let mut primitives = 0usize;
        let mut sampled    = 0usize;

        let mut material_for = |color: &str, materials: &mut ResMut<Assets<StandardMaterial>>| {
            mat_cache
                .entry(color.to_string())
                .or_insert_with(|| materials.add(StandardMaterial {
                    base_color: parse_hex_color(color),
                    perceptual_roughness: 0.85,
                    metallic: 0.0,
                    ..default()
                }))
                .clone()
        };

        for layer in scene.0.layers.iter() {
            let vs = layer.voxel_size;

            // Sampled parts of THIS layer merge into one grid, later parts
            // overwriting earlier — convention 3 within a thing, exactly
            // as `rasterize_entity` does it.

            for part in &layer.parts {
                let frame = part.frame.to_frame();

                match primitive_mesh(&part.shape) {
                    Some((mesh, local_offset, scale)) => {
                        primitives += 1;
                        let handle = material_for(&part.color, &mut materials);
                        commands.spawn(PbrBundle {
                            mesh: meshes.add(mesh),
                            material: handle,
                            transform: transform_of(&frame, local_offset, scale),
                            ..default()
                        });
                    }
                    None => {
                        // No closed-form primitive: mesh the distance field.
                        // Smooth on curves, and it is what makes a mug body,
                        // a blob crown and a heightfield stop being cubes.
                        sampled += 1;
                        let expr = part.shape.to_expr();
                        let cell = crate::mesh::auto_cell(&expr, vs);
                        let tri  = crate::mesh::surface_nets(&expr, cell, vs);
                        if tri.indices.is_empty() { continue; }
                        let handle = material_for(&part.color, &mut materials);
                        commands.spawn(PbrBundle {
                            mesh: meshes.add(tri_to_bevy(&tri)),
                            material: handle,
                            transform: transform_of(&frame, Vec3::ZERO, Vec3::ONE),
                            ..default()
                        });
                    }
                }
            }
        }

        println!("  {primitives} primitive part(s), {sampled} meshed part(s)");
    }

    fn tri_to_bevy(tri: &crate::mesh::TriMesh) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, tri.positions.clone());
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL,   tri.normals.clone());
        mesh.insert_indices(bevy::render::mesh::Indices::U32(tri.indices.clone()));
        mesh
    }

    /// The mesh, a LOCAL-space origin correction, and a scale.
    ///
    /// Origin conventions differ and this is the only place that knows it:
    /// Moxi cylinders have their base at the origin with the axis along
    /// +Y, while Bevy's `Cylinder` is centered — hence the half-height
    /// offset. Spheres and boxes are centered in both. An ellipsoid is a
    /// unit sphere under non-uniform scale, which is exact.
    ///
    /// A `shell` renders as its OUTER surface: the hollow is invisible
    /// from outside, so drawing the inner shape's primitive is exact for
    /// viewing purposes. (A cutaway view would need the sampler.)
    ///
    /// `None` means "no closed form here" — noise-defined shapes, cone
    /// (absent from Bevy 0.13's primitives), and every CSG compound.
    fn primitive_mesh(shape: &Shape) -> Option<(Mesh, Vec3, Vec3)> {
        match shape {
            Shape::Sphere { radius } => Some((
                Sphere::new(*radius as f32).mesh().ico(4).ok()?,
                Vec3::ZERO,
                Vec3::ONE,
            )),
            Shape::Ellipsoid { rx, ry, rz } => Some((
                Sphere::new(1.0).mesh().ico(4).ok()?,
                Vec3::ZERO,
                Vec3::new(*rx as f32, *ry as f32, *rz as f32),
            )),
            // A rounded box has no Bevy primitive; send it to the mesher
            // so the fillets actually appear, rather than drawing a sharp
            // cuboid that silently contradicts the distance field.
            Shape::Box { width, height, depth, round } if *round <= 0.0 => Some((
                Cuboid::new(*width as f32, *height as f32, *depth as f32).into(),
                Vec3::ZERO,
                Vec3::ONE,
            )),
            // Bevy's Capsule3d is centered on its straight segment; Moxi's
            // has its base at the origin, so the offset is half the
            // segment length.
            Shape::Capsule { height, radius } => Some((
                Capsule3d::new(*radius as f32, *height as f32).mesh().latitudes(16).longitudes(24).build(),
                Vec3::new(0.0, *height as f32 * 0.5, 0.0),
                Vec3::ONE,
            )),
            Shape::Torus { major_radius, minor_radius } => Some((
                Torus::new(
                    (*major_radius - *minor_radius) as f32,
                    (*major_radius + *minor_radius) as f32,
                ).into(),
                Vec3::ZERO,
                Vec3::ONE,
            )),
            Shape::Cylinder { height, radius } => Some((
                Cylinder::new(*radius as f32, *height as f32).into(),
                Vec3::new(0.0, *height as f32 * 0.5, 0.0),
                Vec3::ONE,
            )),
            Shape::Shell { inner, .. } => primitive_mesh(inner),
            _ => None,
        }
    }

    // ── Camera system ──────────────────────────────────────────────────

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

        if pan.length_squared() > 0.0 {
            let right = Vec3::new(controller.yaw.sin(), 0.0, -controller.yaw.cos());
            controller.target -= right * pan.x * pan_speed * 0.1;
            controller.target += Vec3::Y * pan.y * pan_speed * 0.1;
        }

        controller.yaw   += orbit.x * 0.005;
        controller.pitch += orbit.y * 0.005;
        let max_pitch = std::f32::consts::FRAC_PI_2 - 0.05;
        controller.pitch = controller.pitch.clamp(-max_pitch, max_pitch);

        for ev in scroll_evr.read() {
            controller.radius -= ev.y * controller.radius * 0.08;
            controller.radius  = controller.radius.clamp(2.0, 500.0);
        }

        let x = controller.radius * controller.yaw.cos() * controller.pitch.cos();
        let y = controller.radius * controller.pitch.sin();
        let z = controller.radius * controller.yaw.sin() * controller.pitch.cos();

        for mut t in query.iter_mut() {
            t.translation = controller.target + Vec3::new(x, y, z);
            t.look_at(controller.target, Vec3::Y);
        }
    }

    // ── Color helpers ──────────────────────────────────────────────────

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
pub use inner::view_scene_bevy;

#[cfg(not(feature = "viewer"))]
pub fn view_scene_bevy(_scene: crate::scene::Scene) {
    eprintln!("viewer feature not enabled — rebuild with: cargo run --features viewer");
}