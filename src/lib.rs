pub mod error;
pub mod lexer;
pub mod ast;
pub mod parser;
pub mod resolver;
pub mod shapes;
pub mod tube;
pub mod mark;
#[cfg(test)]
mod span_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod bind_tests;
#[cfg(test)]
mod thin_tests;
#[cfg(test)]
mod tube_end_tests;
#[cfg(test)]
mod one_error_tests;
#[cfg(test)]
mod weight_tests;
#[cfg(test)]
mod mark_tests;
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

pub mod imports;
pub mod pipeline;
pub mod spec;
pub mod skill;
pub mod bench;
pub mod scene;
pub mod value;
pub mod mesh;
pub mod joints;
pub mod gltf;
pub mod shader;
pub mod report;

// Plain C-ABI wasm exports — no wasm-bindgen toolchain required.
// Compiled everywhere (the ABI is target-agnostic and unit-tested
// natively); only meaningful when built for wasm32-unknown-unknown.
pub mod wasm_abi;

#[cfg(feature = "wasm")]
pub mod wasm;
