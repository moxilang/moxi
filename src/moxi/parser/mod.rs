//! Moxi Parser
//! Splits into: ast.rs, expr.rs, voxel.rs, stmt.rs

mod ast;
mod expr;
mod voxel;
mod stmt;

pub use ast::AstNode;
pub use expr::parse_expression;
pub use voxel::{parse_voxel, parse_voxel_body};
pub use stmt::{parse_assignment, parse_print};

use super::lexer::Token;

/// Entry point: parse whole program into AST
pub fn parse(tokens: Vec<Token>) -> Vec<AstNode> {
    let mut ast = Vec::new();
    let mut iter = tokens.into_iter().peekable();

    while let Some(token) = iter.peek() {
        match token {
            Token::Keyword(k) if k == "voxel" => {
                let node = voxel::parse_voxel(&mut iter);
                ast.push(node);
            }
            Token::Ident(_) => {
                if let Some(node) = stmt::parse_assignment(&mut iter) {
                    ast.push(node);
                } else {
                    iter.next(); // skip if not a valid assignment
                }
            }
            Token::Keyword(k) if k == "print" => {
                let node = stmt::parse_print(&mut iter);
                ast.push(node);
            }
            _ => {
                iter.next(); // skip unknown
            }
        }
    }

    ast
}
