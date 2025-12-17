use std::collections::HashMap;

use crate::types::{Model, SceneGraph, Instance, Value, Transform3D};
use crate::moxi::parser::AstNode;
use super::model_eval::eval_model_body;
use super::eval::eval_expr;

/// Instantiate a model definition into a scene instance
pub fn instantiate_model(
    name: &str,
    body: &[AstNode],
    env: &HashMap<String, Value>,
    scene: &mut SceneGraph,
) -> Instance {
    let model = eval_model_body(name, body, env);
    let inst = Instance {
        model,
        transform: Transform3D::default(),
    };
    scene.instances.push(inst.clone());
    inst
}

/// Try to resolve a Value into an Instance
pub fn resolve_to_instance(
    val: Value,
    env: &HashMap<String, Value>,
    scene: &mut SceneGraph,
) -> Option<Instance> {
    match val {
        Value::Instance(inst) => Some(inst),

        Value::String(name) => match env.get(&name) {
            Some(Value::Instance(inst)) => Some(inst.clone()),

            Some(Value::ModelDef { params, body }) if params.is_empty() => {
                Some(instantiate_model(&name, body, env, scene))
            }

            _ => None,
        },

        _ => None,
    }
}

/// Apply translation KV args to an instance
pub fn apply_translation(
    mut inst: Instance,
    kvs: &HashMap<String, Value>,
) -> Instance {
    if let Some(Value::Number(x)) = kvs.get("x") {
        inst.transform.dx += *x;
    }
    if let Some(Value::Number(y)) = kvs.get("y") {
        inst.transform.dy += *y;
    }
    if let Some(Value::Number(z)) = kvs.get("z") {
        inst.transform.dz += *z;
    }
    inst
}

/// Built-in: merge(...)
pub fn eval_merge(
    args: Vec<AstNode>,
    env: &HashMap<String, Value>,
    scene: &mut SceneGraph,
) -> Value {
    let mut instances = Vec::new();

    for arg in args {
        let val = eval_expr(arg, env, scene);
        if let Some(inst) = resolve_to_instance(val, env, scene) {
            instances.push(inst);
        }
    }

    if instances.is_empty() {
        return Value::String("merge_failed".into());
    }

    let voxels = instances
        .iter()
        .flat_map(|i| {
            SceneGraph {
                instances: vec![i.clone()],
            }
            .resolve_voxels()
            .voxels
        })
        .collect();

    let model = Model {
        name: "merged".into(),
        voxels,
    };

    let inst = Instance {
        model,
        transform: Transform3D::default(),
    };

    scene.instances.push(inst.clone());
    Value::Instance(inst)
}

/// Built-in: translate(...)
pub fn eval_translate(
    args: Vec<AstNode>,
    env: &HashMap<String, Value>,
    scene: &mut SceneGraph,
) -> Value {
    if args.len() != 2 {
        return Value::String("translate_failed".into());
    }

    let inst_val = eval_expr(args[0].clone(), env, scene);
    let kv_val = eval_expr(args[1].clone(), env, scene);

    if let (Some(inst), Value::Map(kvs)) =
        (resolve_to_instance(inst_val, env, scene), kv_val)
    {
        let inst = apply_translation(inst, &kvs);
        scene.instances.push(inst.clone());
        return Value::Instance(inst);
    }

    Value::String("translate_failed".into())
}

/// Model call fallback
pub fn eval_model_call(
    name: &str,
    args: Vec<AstNode>,
    env: &HashMap<String, Value>,
    scene: &mut SceneGraph,
) -> Value {
    if let Some(Value::ModelDef { params, body }) = env.get(name) {
        let mut local_env = env.clone();

        for (param, arg_expr) in params.iter().zip(args.into_iter()) {
            let val = eval_expr(arg_expr, &local_env, scene);
            local_env.insert(param.clone(), val);
        }

        let inst = instantiate_model(name, body, &local_env, scene);
        return Value::Instance(inst);
    }

    Value::String(format!("call_{}", name))
}
