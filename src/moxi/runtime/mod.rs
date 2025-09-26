pub mod ast_collect;
pub mod expand;
pub mod builtins;
pub mod eval;
pub mod model_eval;
pub mod util;

use crate::types::{SceneGraph, VoxelScene, Instance, Transform3D};
use super::parser::AstNode;

pub use ast_collect::{collect_ast, CollectedAst, ModelAst};
pub use expand::{expand_layers, default_color_for_symbol};
pub use builtins::{translate, rotate, merge};
pub use eval::{eval, eval_expr};
pub use model_eval::eval_model_body;
pub use util::{value_to_string, apply_commands};

/// Public entrypoint for flattening evaluation
pub fn run(ast: Vec<AstNode>) -> VoxelScene {
    let scene_graph = build_scene(ast);
    scene_graph.flatten()
}

/// Build a scene by expanding voxel models and applying commands
pub fn build_scene(ast: Vec<AstNode>) -> SceneGraph {
    let collected = collect_ast(&ast);

    let mut scene = SceneGraph { instances: Vec::new() };

    for m in &collected.models {
        let model = expand_layers(m);
        scene.instances.push(Instance {
            model,
            transform: Transform3D {
                dx: 0,
                dy: 0,
                dz: 0,
                rotations: vec![],
            },
        });
    }

    apply_commands(&mut scene, &collected.commands);
    scene
}
