use crate::types::{SceneGraph, Instance, Transform3D};

/// Built-in Moxi commands (SceneGraph edition)
pub fn list_commands() -> Vec<&'static str> {
    vec!["print", "clone", "translate", "rotate", "merge"]
}

pub fn do_print(scene: &SceneGraph) {
    println!("Scene has {} instances", scene.instances.len());
}

pub fn do_clone(scene: &mut SceneGraph) {
    if let Some(last) = scene.instances.last().cloned() {
        scene.instances.push(last);
        println!(
            "Cloned last instance → total {} instances",
            scene.instances.len()
        );
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

pub fn do_merge(scene: &SceneGraph) {
    println!("Merge (noop): scene has {} instances", scene.instances.len());
}
