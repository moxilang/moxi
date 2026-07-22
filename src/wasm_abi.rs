// src/wasm_abi.rs
//
// WebAssembly exports, wasm-bindgen-FREE. Build is just:
//
//   rustup target add wasm32-unknown-unknown
//   cargo build --release --lib --target wasm32-unknown-unknown --no-default-features
//
// Export names and calling convention deliberately match the sandbox's
// raw-ABI loader (src/lib/moxi/wasm.ts), so dropping the .wasm into
// /public/moxi is the ONLY step — no frontend change:
//
//   memory                              : WebAssembly.Memory
//   moxi_alloc(len) -> ptr              : writable buffer (alias: alloc)
//   compile_moxi(ptr, len) -> ptr       : NUL-terminated UTF-8 JSON
//   moxi_free(ptr, len)                 : frees an input buffer (alias: dealloc)
//   moxi_free_result(ptr)               : frees a returned string
//
// A length-prefixed variant (`moxi_compile`) is also exported for hosts
// that prefer it. JSON never contains a NUL byte, so NUL-termination is
// unambiguous.

#![allow(clippy::missing_safety_doc)]

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

/// Length bookkeeping so `moxi_free_result(ptr)` can free without a length.
fn results() -> &'static Mutex<HashMap<usize, usize>> {
    static R: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

#[no_mangle]
pub extern "C" fn moxi_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Alias so a host that looks for `alloc` finds it too.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    moxi_alloc(len)
}

#[no_mangle]
pub unsafe extern "C" fn moxi_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
    }
}

#[no_mangle]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    moxi_free(ptr, len)
}

fn compile_to_cstring(bytes: &[u8]) -> *mut u8 {
    let json = match std::str::from_utf8(bytes) {
        Ok(src) => crate::pipeline::compile_to_json(src),
        Err(_) => "{\"ok\":false,\"errors\":[{\"stage\":\"input\",\
                    \"message\":\"source was not valid UTF-8\",\
                    \"line\":null,\"col\":null}]}"
            .to_string(),
    };

    let mut out = json.into_bytes();
    out.push(0); // NUL terminator
    out.shrink_to_fit();
    let len = out.len();
    let ptr = out.as_mut_ptr();
    std::mem::forget(out);
    results().lock().unwrap().insert(ptr as usize, len);
    ptr
}

/// The export the sandbox loader calls.
#[no_mangle]
pub unsafe extern "C" fn compile_moxi(ptr: *const u8, len: usize) -> *mut u8 {
    compile_to_cstring(std::slice::from_raw_parts(ptr, len))
}

#[no_mangle]
pub unsafe extern "C" fn moxi_free_result(ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    let len = results().lock().unwrap().remove(&(ptr as usize)).unwrap_or(0);
    if len > 0 {
        drop(Vec::from_raw_parts(ptr, 0, len));
    }
}

/// Length-prefixed variant: [u32 LE json_len][json…]. Free with
/// `moxi_free(ptr, 4 + json_len)`.
#[no_mangle]
pub unsafe extern "C" fn moxi_compile(ptr: *const u8, len: usize) -> *mut u8 {
    let bytes = std::slice::from_raw_parts(ptr, len);
    let json = match std::str::from_utf8(bytes) {
        Ok(src) => crate::pipeline::compile_to_json(src),
        Err(_) => "{\"ok\":false,\"errors\":[{\"stage\":\"input\",\
                    \"message\":\"source was not valid UTF-8\",\
                    \"line\":null,\"col\":null}]}"
            .to_string(),
    };
    let json = json.into_bytes();
    let mut out = Vec::<u8>::with_capacity(4 + json.len());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    let p = out.as_mut_ptr();
    std::mem::forget(out);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "atom A { color = red }\n\
                       material M { color = red, voxel_atom = A }\n\
                       entity E { part P { shape = sphere(radius=2), material = M } \
                       resolve voxel_size = 1.0 }\nprint E detail=low\n";

    /// Exactly the protocol src/lib/moxi/wasm.ts performs: alloc, write,
    /// compile_moxi, read until NUL, free both.
    #[test]
    fn sandbox_loader_protocol() {
        unsafe {
            let sp = moxi_alloc(SRC.len());
            std::ptr::copy_nonoverlapping(SRC.as_ptr(), sp, SRC.len());

            let rp = compile_moxi(sp, SRC.len());
            moxi_free(sp, SRC.len());
            assert!(!rp.is_null());

            // readCString: scan to the NUL byte.
            let mut end = 0usize;
            while *rp.add(end) != 0 {
                end += 1;
            }
            let json = std::str::from_utf8(std::slice::from_raw_parts(rp, end))
                .unwrap()
                .to_string();
            moxi_free_result(rp);

            assert!(json.contains("\"ok\":true"), "got: {json}");
            assert!(json.contains("\"voxels\""));
            assert!(!json.contains('\0'));
        }
    }

    #[test]
    fn length_prefixed_variant_and_errors() {
        unsafe {
            let bad = "entity { nope";
            let sp = moxi_alloc(bad.len());
            std::ptr::copy_nonoverlapping(bad.as_ptr(), sp, bad.len());
            let rp = moxi_compile(sp, bad.len());
            moxi_free(sp, bad.len());

            let mut lb = [0u8; 4];
            std::ptr::copy_nonoverlapping(rp, lb.as_mut_ptr(), 4);
            let jl = u32::from_le_bytes(lb) as usize;
            let j = std::str::from_utf8(std::slice::from_raw_parts(rp.add(4), jl))
                .unwrap()
                .to_string();
            moxi_free(rp, 4 + jl);
            assert!(j.contains("\"ok\":false"));
        }
    }
}
