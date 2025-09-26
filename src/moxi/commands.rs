use crate::types::{SceneGraph, Model, Instance, Transform3D, Voxel};

/// Built-in Moxi commands (SceneGraph edition)
pub fn list_commands() -> Vec<&'static str> {
    vec!["print", "clone", "translate", "rotate", "merge"]
}

pub fn do_print(scene: &SceneGraph) {
    println!("Scene has {} instances", scene.instances.len());
}

pub fn do_clone(scene: &mut SceneGraph, args: &[String]) {
    if args.is_empty() {
        // default: clone last
        if let Some(last) = scene.instances.last().cloned() {
            scene.instances.push(last);
            println!("Cloned last instance → total {}", scene.instances.len());
        }
    } else {
        let target = &args[0];
        if let Some(inst) = scene.instances.iter().find(|i| i.model.name == *target).cloned() {
            scene.instances.push(inst);
            println!("Cloned instance of '{}' → total {}", target, scene.instances.len());
        } else {
            println!("⚠️ No instance found with model name '{}'", target);
        }
    }
}

pub fn do_translate(scene: &mut SceneGraph, args: &[String]) {
    if args.len() < 3 {
        println!("⚠️ translate requires 3 args");
        return;
    }

    if let Some(last) = scene.instances.last_mut() {
        last.transform.dx += args[0].parse().unwrap_or(0);
        last.transform.dy += args[1].parse().unwrap_or(0);
        last.transform.dz += args[2].parse().unwrap_or(0);

        println!(
            "Translated last instance by ({}, {}, {})",
            args[0], args[1], args[2]
        );
    }
}

pub fn do_rotate(scene: &mut SceneGraph, args: &[String]) {
    if args.len() < 2 {
        println!("⚠️ rotate requires <axis> <turns>");
        return;
    }

    let axis = args[0].clone();
    let turns: i32 = args[1].parse().unwrap_or(1);

    if let Some(last) = scene.instances.last_mut() {
        last.transform.rotations.push((axis.clone(), turns));
        println!(
            "Rotated last instance around {} by {} quarter turns",
            axis, turns
        );
    }
}


pub fn do_merge(scene: &mut SceneGraph, args: &[String]) {
    if args.len() < 2 {
        println!("⚠️ merge requires at least 2 model names");
        return;
    }

    let mut merged_voxels: Vec<Voxel> = Vec::new();
    let mut names: Vec<String> = Vec::new();

    for name in args {
        if let Some(inst) = scene.instances.iter().find(|i| i.model.name == *name) {
            // Flatten *this* instance fully — includes resolved hex colors
            let sub_scene = SceneGraph { instances: vec![inst.clone()] };
            let voxels = sub_scene.flatten().voxels;
            merged_voxels.extend(voxels);
            names.push(name.clone());
        } else {
            println!("⚠️ merge: no instance found with model name '{}'", name);
        }
    }

    if merged_voxels.is_empty() {
        println!("⚠️ merge failed: no valid models found");
        return;
    }

    // New merged model keeps per-voxel hex colors
    let merged_name = names.join("_");
    let new_model = Model {
        name: merged_name.clone(),
        voxels: merged_voxels,
    };

    let new_instance = Instance {
        model: new_model,
        transform: Transform3D { dx: 0, dy: 0, dz: 0, rotations: vec![] },
    };

    scene.instances.push(new_instance);

    println!("✅ Merged models {:?} → new model '{}'", names, merged_name);
}
