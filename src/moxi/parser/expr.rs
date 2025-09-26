use std::iter::Peekable;
use std::vec::IntoIter;

use super::super::lexer::Token;
use super::ast::AstNode;

/// Parse a comma-separated argument list (used by (), [])
fn parse_arg_list(
    iter: &mut Peekable<IntoIter<Token>>,
    end_token: Token,
) -> Vec<AstNode> {
    let mut elems = Vec::new();
    let mut kvs = Vec::new();

    while let Some(tok2) = iter.peek() {
        match tok2.clone() {
            Token::Ident(id) => {
                iter.next();
                if let Some(Token::Equals) = iter.peek() {
                    iter.next();
                    let val = parse_expression(iter);
                    kvs.push((id, val));
                } else {
                    elems.push(AstNode::Ident(id));
                }
            }
            Token::StringLit(s) => {
                elems.push(AstNode::StringLit(s.clone()));
                iter.next();
            }
            Token::NumberLit(n) => {
                elems.push(AstNode::NumberLit(n));
                iter.next();
            }
            Token::Comma => {
                iter.next();
            }
            tok if tok == end_token => {
                iter.next();
                break;
            }
            _ => {
                iter.next();
            }
        }
    }

    if !kvs.is_empty() {
        elems.push(AstNode::KVArgs(kvs));
    }

    elems
}

/// Parse any expression: ident, literal, array, function call, kvargs
pub fn parse_expression(iter: &mut Peekable<IntoIter<Token>>) -> AstNode {
    match iter.peek().cloned() {
        Some(Token::Ident(id)) => {
            iter.next();
            if let Some(Token::LParen) = iter.peek() {
                iter.next(); // consume '('
                let elems = parse_arg_list(iter, Token::RParen);
                AstNode::FunctionCall { name: id, args: elems }
            } else {
                AstNode::Ident(id)
            }
        }

        Some(Token::StringLit(s)) => {
            iter.next();
            AstNode::StringLit(s)
        }
        Some(Token::NumberLit(n)) => {
            iter.next();
            AstNode::NumberLit(n)
        }

        Some(Token::LBracket) => {
            iter.next(); // consume '['
            let elems = parse_arg_list(iter, Token::RBracket);
            AstNode::ArrayLit(elems)
        }

        Some(Token::LParen) => {
            iter.next(); // consume '('
            let elems = parse_arg_list(iter, Token::RParen);
            if elems.len() == 1 {
                elems.into_iter().next().unwrap()
            } else {
                AstNode::ArrayLit(elems)
            }
        }

        _ => {
            iter.next();
            AstNode::Ident("unknown".into())
        }
    }
}
