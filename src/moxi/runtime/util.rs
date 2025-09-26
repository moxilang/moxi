use crate::types::Value;
use crate::types::SceneGraph;
use crate::moxi::commands::{do_print, do_clone, do_translate, do_rotate, do_merge};

pub fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Ident(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => format!("{:?}", v),
    }
}

pub fn apply_commands(scene: &mut SceneGraph, commands: &[(String, Vec<String>)]) {
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
