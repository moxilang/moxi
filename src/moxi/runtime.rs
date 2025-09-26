use crate::types::{Voxel, VoxelScene, Model, SceneGraph, Instance, Value, Transform3D};
use crate::colors::default_colors;
use super::parser::AstNode;
use super::commands::{do_print, do_clone, do_translate, do_rotate, do_merge};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use unicode_segmentation::UnicodeSegmentation;



/// Container for intermediate AST info
struct CollectedAst {
    models: Vec<ModelAst>,
    commands: Vec<(String, Vec<String>)>,
}

struct ModelAst {
    name: String,
    explicit_colors: HashMap<String, String>,
    layers: Vec<(usize, Vec<String>)>,
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Ident(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => format!("{:?}", v),
    }
}


/// Public entrypoint
pub fn build_scene(ast: Vec<AstNode>) -> SceneGraph {
    let collected = collect_ast(&ast);

    let mut scene = SceneGraph { instances: Vec::new() };

    for m in &collected.models {
        let model = expand_layers(m);
        scene.instances.push(Instance {
            model,
            transform: Transform3D { dx: 0, dy: 0, dz: 0, rotations: vec![] },
        });
    }

    apply_commands(&mut scene, &collected.commands);
    scene
}

/// Pass 1: collect colors, layers, and commands
fn collect_ast(ast: &[AstNode]) -> CollectedAst {
    let mut models = Vec::new();
    let mut commands = Vec::new();

    let mut current_model: Option<ModelAst> = None;

    for node in ast {
        match node {
            AstNode::VoxelDecl { name, .. } => {
                if let Some(m) = current_model.take() {
                    models.push(m);
                }
                current_model = Some(ModelAst {
                    name: name.clone(),
                    explicit_colors: HashMap::new(),
                    layers: Vec::new(),
                });
            }
            AstNode::ColorDecl { symbol, color } => {
                if let Some(m) = current_model.as_mut() {
                    m.explicit_colors.insert(symbol.clone(), color.clone());
                }
            }
            AstNode::LayerDecl { z, rows } => {
                if let Some(m) = current_model.as_mut() {
                    m.layers.push((*z, rows.clone()));
                }
            }
            // old Command variant no longer exists; skip FunctionCall etc. here
            _ => {
                // ignore assignments, function calls, literals, etc.
            }
        }
    }

    if let Some(m) = current_model.take() {
        models.push(m);
    }

    CollectedAst { models, commands }
}



/// Pass 2: expand layers → voxels using explicit + builtins
fn expand_layers(m: &ModelAst) -> Model {
    let mut voxels = Vec::new();
    let builtins = default_colors();

    for (z, rows) in &m.layers {
        for (y, row) in rows.iter().enumerate() {
            for (x, symbol) in unicode_segmentation::UnicodeSegmentation::graphemes(row.as_str(), true).enumerate() {
                if symbol == "." || symbol == " " {
                    continue;
                }

                // resolve symbol using *this model’s* color map first
                let raw = m.explicit_colors
                    .get(symbol)
                    .cloned()
                    .or_else(|| builtins.get(symbol).cloned())
                    .unwrap_or_else(|| default_color_for_symbol(symbol));

                let hex = builtins.get(&raw).cloned().unwrap_or(raw);

                voxels.push(Voxel {
                    x: x as i32,
                    y: y as i32,
                    z: *z as i32,
                    color: hex,
                });
            }
        }
    }

    Model { name: m.name.clone(), voxels }
}


/// Deterministic fallback color generator for unknown symbols
fn default_color_for_symbol(sym: &str) -> String {
    let mut hasher = DefaultHasher::new();
    sym.hash(&mut hasher);
    let h = hasher.finish();
    format!("#{:06x}", (h as u32) & 0xffffff)
}


fn apply_commands(scene: &mut SceneGraph, commands: &[(String, Vec<String>)]) {
    for (name, args) in commands {
        match name.as_str() {
            "print" => do_print(scene),
            "clone" => do_clone(scene, args),
            "translate" => do_translate(scene, args),
            "rotate" => do_rotate(scene, args),
            "merge" => do_merge(scene, args),
            _ => println!("Unknown command: {}", name),
        }
    }
}


/// Translate a voxel scene
pub fn translate(scene: &VoxelScene, dx: i32, dy: i32, dz: i32) -> VoxelScene {
    let voxels = scene.voxels.iter().map(|v| Voxel {
        x: v.x + dx,
        y: v.y + dy,
        z: v.z + dz,
        color: v.color.clone(),
    }).collect();
    VoxelScene { voxels }
}

/// Rotate a voxel scene 90° multiples around an axis
pub fn rotate(scene: &VoxelScene, axis: &str, turns: i32) -> VoxelScene {
    let mut voxels = Vec::new();
    let turns = ((turns % 4) + 4) % 4; // normalize to 0..3

    for v in &scene.voxels {
        let (mut x, mut y, mut z) = (v.x, v.y, v.z);
        for _ in 0..turns {
            match axis {
                "x" => { let ny = -z; let nz = y; y = ny; z = nz; }
                "y" => { let nx = z; let nz = -x; x = nx; z = nz; }
                "z" => { let nx = -y; let ny = x; x = nx; y = ny; }
                _ => {}
            }
        }
        voxels.push(Voxel {
            x, y, z,
            color: v.color.clone(),
        });
    }

    let mut rotated = VoxelScene { voxels };
    rotated.normalize(); // <- shift back into positive space
    rotated
}



/// Merge multiple voxel scenes
pub fn merge(scenes: Vec<VoxelScene>) -> VoxelScene {
    let mut voxels = Vec::new();
    for s in scenes {
        voxels.extend(s.voxels);
    }
    VoxelScene { voxels }
}


pub fn run(ast: Vec<AstNode>) -> VoxelScene {
    let scene_graph = build_scene(ast);
    scene_graph.flatten()
}


pub fn eval(ast: Vec<AstNode>) -> SceneGraph {
    let mut env: HashMap<String, Value> = HashMap::new();
    let mut scene = SceneGraph { instances: Vec::new() };

    for node in ast {
        match node {
            // store voxel model definition
            AstNode::VoxelDecl { name, params, body } => {
                env.insert(name.clone(), Value::ModelDef { params, body });
            }

            // assignment: handle instantiation
            AstNode::Assignment { name, expr } => {
                let value = eval_expr(*expr, &env, &mut scene);

                let final_val = match &value {
                    // `foo = Foo` → instantiate immediately if Foo has no params
                    Value::String(s) if env.contains_key(s) => {
                        if let Some(Value::ModelDef { params, body }) = env.get(s) {
                            if params.is_empty() {
                                let model = eval_model_body(body, &env);
                                let inst = Instance { model: model.clone(), transform: Transform3D::default() };
                                scene.instances.push(inst.clone());
                                Value::Instance(inst)
                            } else {
                                // keep reference only
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

            // print command
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

            // bare identifier used as statement → auto-instantiate if model has no params
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

fn eval_expr(expr: AstNode, env: &HashMap<String, Value>, scene: &mut SceneGraph) -> Value {
    match expr {
        AstNode::Ident(name) => {
            // if it’s a known model, return its name as a string for Assignment logic
            if env.contains_key(&name) {
                Value::String(name)
            } else {
                Value::String(name)
            }
        }

        AstNode::StringLit(s) => Value::String(s),
        AstNode::NumberLit(n) => Value::Number(n),

        AstNode::ArrayLit(elems) => {
            let mut vals = Vec::new();
            for e in elems {
                vals.push(eval_expr(e, env, scene));
            }
            Value::Array(vals)
        }

        AstNode::KVArgs(pairs) => {
            let mut map = HashMap::new();
            for (k, v) in pairs {
                map.insert(k, eval_expr(v, env, scene));
            }
            Value::Map(map)  // Add this new Value variant in types.rs
        }


        AstNode::FunctionCall { name, args } => {
            // built-in functions (merge, translate, etc.)
            match name.as_str() {
                "merge" => {
                    let mut merged_voxels = Vec::new();
                    let mut names = Vec::new();
                    for arg in args {
                        if let Value::Instance(inst) = eval_expr(arg, env, scene) {
                            let sub_scene = SceneGraph { instances: vec![inst.clone()] };
                            merged_voxels.extend(sub_scene.flatten().voxels);
                            names.push(inst.model.name);
                        }
                    }
                    let model = Model { name: names.join("_"), voxels: merged_voxels };
                    let inst = Instance { model: model.clone(), transform: Transform3D::default() };
                    scene.instances.push(inst.clone());
                    return Value::Instance(inst);
                }

                "translate" => {
                    if args.len() == 2 {
                        let inst_val = eval_expr(args[0].clone(), env, scene);
                        let kv_val = eval_expr(args[1].clone(), env, scene);

                        if let (Value::Instance(mut inst), Value::Map(kvs)) = (inst_val, kv_val) {
                            let dx = match kvs.get("x") { Some(Value::Number(n)) => *n, _ => 0 };
                            let dy = match kvs.get("y") { Some(Value::Number(n)) => *n, _ => 0 };
                            let dz = match kvs.get("z") { Some(Value::Number(n)) => *n, _ => 0 };

                            inst.transform.dx += dx;
                            inst.transform.dy += dy;
                            inst.transform.dz += dz;

                            scene.instances.push(inst.clone());
                            return Value::Instance(inst);
                        }
                    }
                    return Value::String("translate_failed".into());
                }

                _ => {}
            }

            // user-defined voxel instantiation
            if let Some(Value::ModelDef { params, body }) = env.get(&name) {
                let mut local_env = env.clone();

                // bind args
                for (param, arg_expr) in params.iter().zip(args.into_iter()) {
                    let val = eval_expr(arg_expr, &local_env, scene);
                    local_env.insert(param.clone(), val);
                }

                let model = eval_model_body(body, &local_env);
                let inst = Instance { model: model.clone(), transform: Transform3D::default() };
                scene.instances.push(inst.clone());
                return Value::Instance(inst);
            }

            // fallback
            Value::String(format!("call_{}", name))
        }

        _ => Value::String("unhandled_expr".into()),
    }
}


fn eval_model_body(body: &[AstNode], env: &HashMap<String, Value>) -> Model {
    let mut pending_layers: Vec<(usize, Vec<String>)> = Vec::new();
    let mut voxels = Vec::new();
    let mut colors = default_colors(); // start with built-ins

    // Pass 1: collect colors and store raw layers
    for node in body {
        match node {
            AstNode::ColorDecl { symbol, color } => {
                // resolve against built-ins
                let resolved = default_colors()
                    .get(color)
                    .cloned()
                    .unwrap_or(color.clone());
                colors.insert(symbol.clone(), resolved);
            }

            AstNode::AddColor { symbol, color } => {
                let mut resolved_name = color.clone();

                // try to resolve variable
                if let Some(val) = env.get(color) {
                    resolved_name = match val {
                        Value::Array(vs) if !vs.is_empty() => value_to_string(&vs[0]),
                        _ => value_to_string(val),
                    };
                }

                // second lookup if resolved_name itself is a variable in env
                if let Some(val2) = env.get(&resolved_name) {
                    resolved_name = match val2 {
                        Value::Array(vs) if !vs.is_empty() => value_to_string(&vs[0]),
                        _ => value_to_string(val2),
                    };
                }

                // resolve to hex
                let final_hex = default_colors()
                    .get(&resolved_name)
                    .cloned()
                    .unwrap_or(resolved_name);

                colors.insert(symbol.clone(), final_hex);
            }

            AstNode::LayerDecl { z, rows } => {
                pending_layers.push((*z, rows.clone()));
            }

            AstNode::AddLayer { x, y, z, symbol } => {
                let resolved = colors
                    .get(symbol)
                    .cloned()
                    .unwrap_or("#888888".into());
                voxels.push(Voxel { x: *x, y: *y, z: *z, color: resolved });
            }

            // support procedural loops
            AstNode::ForLoop { var1, var2, iter1, iter2, body: inner } => {
                if let (Some(Value::Array(v1)), Some(Value::Array(v2))) =
                    (env.get(iter1), env.get(iter2))
                {
                    for (a, b) in v1.iter().zip(v2.iter()) {
                        let mut local_env = env.clone();
                        local_env.insert(var1.clone(), Value::String(value_to_string(a)));
                        local_env.insert(var2.clone(), Value::String(value_to_string(b)));
                        let sub_model = eval_model_body(inner, &local_env);
                        voxels.extend(sub_model.voxels);
                    }
                }
            }

            AstNode::ForRange { var, start, end, body: inner } => {
                for i in *start..*end {
                    let mut local_env = env.clone();
                    local_env.insert(var.clone(), Value::Number(i));
                    let sub_model = eval_model_body(inner, &local_env);
                    voxels.extend(sub_model.voxels);
                }
            }

            _ => {}
        }
    }

    // Pass 2: expand stored ASCII layers using the resolved color map
    for (z, rows) in pending_layers {
        for (y, row) in rows.iter().enumerate() {
            for (x, sym) in row.chars().enumerate() {
                if sym == '.' || sym == ' ' { continue; }
                let key = sym.to_string();
                let resolved = colors
                    .get(&key)
                    .cloned()
                    .unwrap_or("#888888".into());
                voxels.push(Voxel { x: x as i32, y: y as i32, z: z as i32, color: resolved });
            }
        }
    }

    Model { name: "anonymous".into(), voxels }
}
