// src/wasm.rs — the browser-native target.
//
// Build:  wasm-pack build --features wasm --no-default-features
//    or:  cargo build --target wasm32-unknown-unknown --features wasm
//
// One compiler, every target: the SAME Rust pipeline that runs the CLI
// runs in the browser. The TypeScript + Three.js layer only renders the
// JSON this returns — it never re-implements Moxi semantics, so web and
// native output stay voxel-identical by construction.

use wasm_bindgen::prelude::*;

/// Compile Moxi source. Returns the wire JSON:
///   {"ok":true,"total":…,"bounds":…,"layers":[…],"voxels":[…]}
/// | {"ok":false,"errors":[{"stage","message","line","col"}…]}
#[wasm_bindgen]
pub fn compile_moxi(source: &str) -> String {
    crate::pipeline::compile_to_json(source)
}

/// Compiler version, for cache-busting on the web side.
#[wasm_bindgen]
pub fn moxi_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
