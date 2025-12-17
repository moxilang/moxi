mod ast;
mod expr;
mod voxel;
mod stmt;

pub use ast::AstNode;
use super::lexer::Token;

/// Entry point: parse whole program into AST
pub fn parse(tokens: Vec<Token>) -> Vec<AstNode> {
    let mut ast = Vec::new();
    let mut iter = tokens.into_iter().peekable();

    while let Some(token) = iter.peek() {
        match token {
            Token::Keyword(k) if k == "atom" => {
                let node = stmt::parse_atom(&mut iter);
                ast.push(node);
            }
            Token::Keyword(k) if k == "voxel" => {
                let node = voxel::parse_voxel(&mut iter);
                ast.push(node);
            }
            Token::Ident(_) => {
                if let Some(node) = stmt::parse_assignment(&mut iter) {
                    ast.push(node);
                } else {
                    iter.next();
                }
            }
            Token::Keyword(k) if k == "print" => {
                let node = stmt::parse_print(&mut iter);
                ast.push(node);
            }
            _ => {
                iter.next();
            }
        }
    }

    ast
}
