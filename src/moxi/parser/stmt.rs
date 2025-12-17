use std::iter::Peekable;
use std::vec::IntoIter;

use super::super::lexer::Token;
use super::ast::AstNode;
use super::expr::parse_expression;

/// Parse `name = expr`
pub fn parse_assignment(iter: &mut Peekable<IntoIter<Token>>) -> Option<AstNode> {
    // variable name
    let name = if let Some(Token::Ident(var)) = iter.next() {
        var
    } else {
        return None;
    };

    // expect '='
    if let Some(Token::Equals) = iter.peek() {
        iter.next(); // consume '='
        let expr = parse_expression(iter);
        return Some(AstNode::Assignment {
            name,
            expr: Box::new(expr),
        });
    }

    None
}

/// Parse `print [target]`
pub fn parse_print(iter: &mut Peekable<IntoIter<Token>>) -> AstNode {
    iter.next(); // consume "print"
    let mut target = None;

    if let Some(Token::Ident(id)) = iter.peek() {
        target = Some(id.clone());
        iter.next();
    }

    AstNode::Print { target }
}

pub fn parse_atom(iter: &mut Peekable<IntoIter<Token>>) -> AstNode {
    iter.next(); // consume "atom"

    let name = if let Some(Token::Ident(id)) = iter.next() {
        id
    } else {
        "<missing_atom>".into()
    };

    let mut props = Vec::new();

    if let Some(Token::LBrace) = iter.peek() {
        iter.next(); // consume '{'

        while let Some(tok) = iter.peek() {
            match tok {
                Token::Ident(key) => {
                    let key = key.clone();
                    iter.next();

                    if let Some(Token::Equals) = iter.next() {
                        if let Some(Token::Ident(val)) = iter.next() {
                            props.push((key, val));
                        }
                    }
                }
                Token::RBrace => {
                    iter.next();
                    break;
                }
                _ => {
                    iter.next();
                }
            }
        }
    }

    AstNode::AtomDecl { name, props }
}

pub fn parse_legend(iter: &mut Peekable<IntoIter<Token>>) -> AstNode {
    iter.next(); // consume "legend"

    let mut mappings = Vec::new();

    if let Some(Token::LBrace) = iter.peek() {
        iter.next();

        while let Some(tok) = iter.peek() {
            match tok {
                Token::Ident(glyph) => {
                    let glyph = glyph.clone();
                    iter.next();

                    if let Some(Token::Equals) = iter.next() {
                        if let Some(Token::Ident(atom)) = iter.next() {
                            mappings.push((glyph, atom));
                        }
                    }
                }
                Token::RBrace => {
                    iter.next();
                    break;
                }
                _ => {
                    iter.next();
                }
            }
        }
    }

    AstNode::LegendDecl { mappings }
}
