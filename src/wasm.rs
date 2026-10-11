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

/// Compile a script that imports other files (Issue J). `files_json` is a
/// `{"path": "text", …}` map holding the main file and every file it
/// imports, paths relative to the main file's folder. Same wire JSON as
/// `compile_moxi`; an error names the imported file it is in (`file`).
#[wasm_bindgen]
pub fn compile_moxi_files(main: &str, files_json: &str) -> String {
    let files: std::collections::HashMap<String, String> = match serde_json::from_str(files_json) {
        Ok(f) => f,
        Err(e) => return serde_json::json!({ "ok": false, "errors": [{
            "stage": "import", "message": format!("files must be a JSON object of path → text: {e}"),
            "line": null, "col": null }] }).to_string(),
    };
    let sources = crate::imports::MapSources(files);
    crate::pipeline::compile_to_json(crate::imports::Program::Files { main, sources: &sources })
}

/// Compiler version, for cache-busting on the web side.
#[wasm_bindgen]
pub fn moxi_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
