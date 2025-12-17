use std::collections::HashMap;
use crate::types::{Model, SceneGraph, Instance, Value, Transform3D};
use super::super::parser::AstNode;
use super::model_eval::eval_model_body;

/// Evaluate a program AST into a SceneGraph
pub fn eval(ast: Vec<AstNode>) -> SceneGraph {
    let mut env: HashMap<String, Value> = HashMap::new();
    let mut scene = SceneGraph { instances: Vec::new() };

    for node in ast {
        match node {
            AstNode::AtomDecl { name, props } => {
                let mut map = HashMap::new();
                for (k, v) in props {
                    map.insert(k.clone(), v.clone());
                }
                env.insert(
                    name.clone(),
                    Value::Atom {
                        name,
                        props: map,
                    },
                );
            }

            AstNode::VoxelDecl { name, params, body } => {
                env.insert(name.clone(), Value::ModelDef { params, body });
            }

            AstNode::Assignment { name, expr } => {
                let value = eval_expr(*expr, &env, &mut scene);

                let final_val = match &value {
                    // auto-instantiate if it's a known model name
                    Value::String(s) if env.contains_key(s) => {
                        if let Some(Value::ModelDef { params, body }) = env.get(s) {
                            if params.is_empty() {
                                let model = eval_model_body(body, &env);
                                let inst = Instance { model: model.clone(), transform: Transform3D::default() };
                                scene.instances.push(inst.clone());
                                Value::Instance(inst)
                            } else {
                                value
                            }
                        } else {
                            value
                        }
                    }
                    Value::Instance(_) => value,
                    _ => value,
                };

                env.insert(name, final_val);
            }

            AstNode::Print { target } => {
                if let Some(var) = target {
                    if let Some(v) = env.get(&var) {
                        println!("{} = {:?}", var, v);
                    } else {
                        println!("⚠️ Unknown identifier '{}'", var);
                    }
                } else {
                    println!("Scene has {} instances", scene.instances.len());
                }
            }

            AstNode::Ident(id) => {
                if let Some(Value::ModelDef { params, body }) = env.get(&id) {
                    if params.is_empty() {
                        let model = eval_model_body(body, &env);
                        let inst = Instance { model: model.clone(), transform: Transform3D::default() };
                        scene.instances.push(inst.clone());
                        println!("Instantiated '{}' → total {}", id, scene.instances.len());
                    }
                }
            }

            _ => {}
        }
    }

    scene
}

/// Evaluate a single expression into a Value
pub fn eval_expr(expr: AstNode, env: &HashMap<String, Value>, scene: &mut SceneGraph) -> Value {
    match expr {
        AstNode::Ident(name) => Value::String(name),

        AstNode::StringLit(s) => Value::String(s),
        AstNode::NumberLit(n) => Value::Number(n),

        AstNode::ArrayLit(elems) => {
            let vals = elems.into_iter().map(|e| eval_expr(e, env, scene)).collect();
            Value::Array(vals)
        }

        AstNode::KVArgs(pairs) => {
            let mut map = HashMap::new();
            for (k, v) in pairs {
                map.insert(k, eval_expr(v, env, scene));
            }
            Value::Map(map)
        }

        AstNode::FunctionCall { name, args } => {
            match name.as_str() {
                "merge" => {
                    let mut instances = Vec::new();

                    for arg in args {
                        let val = eval_expr(arg, env, scene);
                        match val {
                            Value::Instance(inst) => instances.push(inst),
                            Value::String(s) => {
                                if let Some(Value::ModelDef { params, body }) = env.get(&s) {
                                    if params.is_empty() {
                                        let model = eval_model_body(body, env);
                                        instances.push(Instance { model, transform: Transform3D::default() });
                                    }
                                } else if let Some(Value::Instance(inst_ref)) = env.get(&s) {
                                    instances.push(inst_ref.clone());
                                }
                            }
                            _ => {}
                        }
                    }

                    if !instances.is_empty() {
                        let merged_voxels = instances.iter()
                            .flat_map(|i| SceneGraph { instances: vec![i.clone()] }.flatten().voxels)
                            .collect();
                        let model = Model { name: "merged".into(), voxels: merged_voxels };
                        let inst = Instance { model: model.clone(), transform: Transform3D::default() };
                        scene.instances.push(inst.clone());
                        return Value::Instance(inst);
                    }
                    return Value::String("merge_failed".into());
                }

                "translate" => {
                    if args.len() == 2 {
                        let inst_val = eval_expr(args[0].clone(), env, scene);
                        let kv_val = eval_expr(args[1].clone(), env, scene);

                        if let (Value::Instance(mut inst), Value::Map(kvs)) = (inst_val.clone(), kv_val.clone()) {
                            inst.transform.dx += kvs.get("x").and_then(|v| match v { Value::Number(n) => Some(*n), _ => None }).unwrap_or(0);
                            inst.transform.dy += kvs.get("y").and_then(|v| match v { Value::Number(n) => Some(*n), _ => None }).unwrap_or(0);
                            inst.transform.dz += kvs.get("z").and_then(|v| match v { Value::Number(n) => Some(*n), _ => None }).unwrap_or(0);

                            scene.instances.push(inst.clone());
                            return Value::Instance(inst);
                        }

                        if let (Value::String(name), Value::Map(kvs)) = (inst_val, kv_val) {
                            if let Some(Value::Instance(inst_ref)) = env.get(&name) {
                                let mut inst = inst_ref.clone();
                                inst.transform.dx += kvs.get("x").and_then(|v| match v { Value::Number(n) => Some(*n), _ => None }).unwrap_or(0);
                                inst.transform.dy += kvs.get("y").and_then(|v| match v { Value::Number(n) => Some(*n), _ => None }).unwrap_or(0);
                                inst.transform.dz += kvs.get("z").and_then(|v| match v { Value::Number(n) => Some(*n), _ => None }).unwrap_or(0);

                                scene.instances.push(inst.clone());
                                return Value::Instance(inst);
                            }
                        }
                    }
                    return Value::String("translate_failed".into());
                }

                _ => {}
            }

            if let Some(Value::ModelDef { params, body }) = env.get(&name) {
                let mut local_env = env.clone();

                for (param, arg_expr) in params.iter().zip(args.into_iter()) {
                    let val = eval_expr(arg_expr, &local_env, scene);
                    local_env.insert(param.clone(), val);
                }

                let model = eval_model_body(body, &local_env);
                let inst = Instance { model: model.clone(), transform: Transform3D::default() };
                scene.instances.push(inst.clone());
                return Value::Instance(inst);
            }

            Value::String(format!("call_{}", name))
        }

        _ => Value::String("unhandled_expr".into()),
    }
}
