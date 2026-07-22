pub mod error;
pub mod lexer;
pub mod ast;
pub mod parser;
pub mod resolver;
pub mod geometry;
pub mod voxel;

pub mod colors;
pub mod geom;
pub mod frame;
pub mod anchors;
pub mod frame_resolver;
pub mod types;
pub mod export;
pub mod bevy_viewer;
pub mod generator;

pub mod pipeline;

// Plain C-ABI wasm exports — no wasm-bindgen toolchain required.
// Compiled everywhere (the ABI is target-agnostic and unit-tested
// natively); only meaningful when built for wasm32-unknown-unknown.
pub mod wasm_abi;

#[cfg(feature = "wasm")]
pub mod wasm;
