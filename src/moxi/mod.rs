//! Moxi DSL
//!
//! This module contains the Moxi programming language implementation.
//! It is split into four parts:
//! - `lexer`: turn source text into tokens
//! - `parser`: turn tokens into an AST
//! - `runtime`: execute AST into voxel models
//! - `commands`: built-in operations (translate, rotate, merge, etc.)

pub mod lexer;
pub mod parser;
pub mod runtime;
