use moxi_lib::lexer::Lexer;
use moxi_lib::parser::Parser;
use moxi_lib::resolver::Resolver;
use moxi_lib::geometry::{self, merge_parts};
use moxi_lib::relation_resolver::resolve_offsets;
use moxi_lib::types::grid_to_scene;
use moxi_lib::export::export_to_obj;

static SOURCE: &str = r#"
atom BONE    { color = ivory }
atom MUSCLE  { color = red }
atom VISCERA { color = maroon }

material Bone   { color = ivory,  voxel_atom = BONE }
material Muscle { color = red,    voxel_atom = MUSCLE }
material Organ  { color = maroon, voxel_atom = VISCERA }

entity Skeleton {
    part Skull   { shape = sphere(radius=4),                                    material = Bone }
    part Spine   { shape = cylinder(height=24, radius=0.8),                     material = Bone }
    part Ribcage { shape = shell(ellipsoid(rx=8, ry=10, rz=6), inner_offset=1), material = Bone }
    part Pelvis  { shape = ellipsoid(rx=7, ry=4, rz=5),                         material = Bone }

    relation {
        Skull   above    Spine
        Ribcage surrounds Spine
        Pelvis  below    Spine
    }

    constraint Skull above Spine

    resolve voxel_size = 1.0
}

print Skeleton detail=low
"#;

fn main() {
    // ── 1. Lex ────────────────────────────────────────────────────────────
    let (tokens, lex_errors) = Lexer::new(SOURCE).tokenize();
    for e in &lex_errors { eprintln!("lex error: {e}"); }

    // ── 2. Parse ──────────────────────────────────────────────────────────
    let (doc, parse_errors) = Parser::new(tokens).parse();
    for e in &parse_errors { eprintln!("parse error: {e}"); }

    // ── 3. Resolve ────────────────────────────────────────────────────────
    let (scene, resolve_errors) = Resolver::new().resolve(doc);
    for e in &resolve_errors { eprintln!("resolve error: {e}"); }
    if !resolve_errors.is_empty() {
        std::process::exit(1);
    }

    // ── 4. Compile each entity ────────────────────────────────────────────
    let compiled = geometry::compile(&scene, 1.0);

    for (ent, resolved_ent) in compiled.iter().zip(scene.entities.iter()) {
        println!("\nentity '{}'", ent.name);
        println!("  parts: {}", ent.parts.len());

        // ── 5. Relation resolver ──────────────────────────────────────────
        let offset_map = resolve_offsets(&ent.parts, &resolved_ent.relations);

        println!("  offsets:");
        for (name, off) in &offset_map {
            println!("    {name:12} → dx={:>4} dy={:>4} dz={:>4}",
                off.dx, off.dy, off.dz);
        }

        // Convert OffsetMap → vec for merge_parts
        let offsets_vec: Vec<(String, (i32, i32, i32))> = offset_map
            .iter()
            .map(|(name, off)| (name.clone(), (off.dx, off.dy, off.dz)))
            .collect();

        // ── 6. Merge parts with offsets applied ───────────────────────────
        let merged = merge_parts(&ent.parts, &offsets_vec);
        let (w, h, d) = merged.dims();
        println!("  merged grid: {}x{}x{}, {} voxels", w, h, d, merged.filled_count());

        // ── 7. Export OBJ ─────────────────────────────────────────────────
        let voxel_scene = grid_to_scene(&merged, &scene.atoms, (0, 0, 0));
        std::fs::create_dir_all("output").ok();
        let out_path = format!("output/{}", ent.name);
        if let Err(e) = export_to_obj(&voxel_scene, &out_path) {
            eprintln!("export error: {e}");
        }

        // ── 8. Launch Bevy viewer ─────────────────────────────────────────
        #[cfg(feature = "viewer")]
        moxi_lib::bevy_viewer::view_voxels_bevy(voxel_scene);
    }
}