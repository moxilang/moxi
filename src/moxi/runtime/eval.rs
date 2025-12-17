use std::collections::HashMap;

use crate::types::{SceneGraph, Value};
use crate::moxi::parser::AstNode;

use super::helpers::{
    eval_merge,
    eval_translate,
    eval_model_call,
    instantiate_model,
};


/// Evaluate a program AST into a SceneGraph
pub fn eval(ast: Vec<AstNode>) -> SceneGraph {
    let mut env: HashMap<String, Value> = HashMap::new();
    let mut scene = SceneGraph { instances: Vec::new() };

    for node in ast {
        match node {
            AstNode::AtomDecl { name, props } => {
                let mut map = HashMap::new();
                for (k, v) in props {
                    map.insert(k, v);
                }
                env.insert(name.clone(), Value::Atom { name, props: map });
            }

            AstNode::VoxelDecl { name, params, body } => {
                env.insert(name, Value::ModelDef { params, body });
            }

            AstNode::Assignment { name, expr } => {
                let value = eval_expr(*expr, &env, &mut scene);

                let final_val = match &value {
                    Value::String(s) => {
                        if let Some(Value::ModelDef { params, body }) = env.get(s) {
                            if params.is_empty() {
                                Value::Instance(
                                    instantiate_model(s, body, &env, &mut scene)
                                )
                            } else {
                                value
                            }
                        } else {
                            value
                        }
                    }
                    _ => value,
                };

                env.insert(name, final_val);
            }

            AstNode::Print { target } => {
                if let Some(var) = target {
                    match env.get(&var) {
                        Some(v) => println!("{} = {:?}", var, v),
                        None => println!("⚠️ Unknown identifier '{}'", var),
                    }
                } else {
                    println!("Scene has {} instances", scene.instances.len());
                }
            }

            AstNode::Ident(id) => {
                if let Some(Value::ModelDef { params, body }) = env.get(&id) {
                    if params.is_empty() {
                        instantiate_model(&id, body, &env, &mut scene);
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
pub fn eval_expr(
    expr: AstNode,
    env: &HashMap<String, Value>,
    scene: &mut SceneGraph,
) -> Value {
    match expr {
        AstNode::Ident(name) => Value::String(name),
        AstNode::StringLit(s) => Value::String(s),
        AstNode::NumberLit(n) => Value::Number(n),

        AstNode::ArrayLit(elems) => {
            Value::Array(
                elems.into_iter()
                    .map(|e| eval_expr(e, env, scene))
                    .collect()
            )
        }

        AstNode::KVArgs(pairs) => {
            let mut map = HashMap::new();
            for (k, v) in pairs {
                map.insert(k, eval_expr(v, env, scene));
            }
            Value::Map(map)
        }

        AstNode::FunctionCall { name, args } => {
            eval_function_call(&name, args, env, scene)
        }

        _ => Value::String("unhandled_expr".into()),
    }
}


fn eval_function_call(
    name: &str,
    args: Vec<AstNode>,
    env: &HashMap<String, Value>,
    scene: &mut SceneGraph,
) -> Value {
    match name {
        "merge" => eval_merge(args, env, scene),
        "translate" => eval_translate(args, env, scene),
        _ => eval_model_call(name, args, env, scene),
    }
}
