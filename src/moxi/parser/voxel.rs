use std::iter::Peekable;
use std::vec::IntoIter;

use super::super::lexer::Token;
use super::ast::AstNode;
use super::expr::parse_expression;

/// Parse a complete voxel declaration: `voxel Name(params) { body }`
pub fn parse_voxel(iter: &mut Peekable<IntoIter<Token>>) -> AstNode {
    // consume "voxel"
    iter.next();

    // voxel name
    let name = match iter.next() {
        Some(Token::Ident(n)) => n,
        _ => "<missing_name>".into(),
    };

    // optional params
    let mut params = Vec::new();
    if let Some(Token::LParen) = iter.peek() {
        iter.next();
        while let Some(tok) = iter.next() {
            match tok {
                Token::Ident(p) => params.push(p),
                Token::Comma => continue,
                Token::RParen => break,
                _ => break,
            }
        }
    }

    // body
    let mut body = Vec::new();
    if let Some(Token::LBrace) = iter.peek() {
        iter.next();
        body = parse_voxel_body(iter);
    }

    AstNode::VoxelDecl { name, params, body }
}

/// Parse everything inside `{ ... }` of a voxel
pub fn parse_voxel_body(iter: &mut Peekable<IntoIter<Token>>) -> Vec<AstNode> {
    let mut body = Vec::new();

    while let Some(tok) = iter.peek() {
        match tok {
            Token::RBrace => { iter.next(); break; }
            Token::LBracket => {
                let colors = parse_bracket_block(iter);
                body.extend(colors);
            }
            Token::Ident(word) if word == "add" => {
                let adds = parse_add(iter);
                body.extend(adds);
            }
            Token::Keyword(word) if word == "for" => body.push(parse_for(iter)),
            _ => { iter.next(); }
        }
    }

    body
}

/// Handle `[Layer N] ...` and `[Colors] ...`
fn parse_bracket_block(iter: &mut Peekable<IntoIter<Token>>) -> Vec<AstNode> {
    let mut nodes = Vec::new();
    iter.next(); // consume '['

    if let Some(Token::Ident(word)) = iter.next() {
        if word == "Layer" {
            // e.g. [Layer 0]
            let z = if let Some(Token::NumberLit(n)) = iter.next() { n as usize } else { 0 };
            let _ = iter.next(); // RBracket
            let mut rows = Vec::new();

            while let Some(tok2) = iter.peek() {
                match tok2 {
                    Token::Ident(row) => {
                        if row == "add" || row == "for" { break; }
                        rows.push(row.clone());
                        iter.next();
                    }
                    Token::LBracket | Token::RBrace => break,
                    _ => { iter.next(); }
                }
            }

            nodes.push(AstNode::LayerDecl { z, rows });
        } else if word == "Colors" {
            let _ = iter.next(); // RBracket
            while let Some(tok2) = iter.peek() {
                match tok2 {
                    Token::Ident(sym) => {
                        let sym = sym.clone();
                        iter.next();
                        if let Some(Token::Colon) = iter.next() {
                            if let Some(Token::Ident(color)) = iter.next() {
                                nodes.push(AstNode::ColorDecl { symbol: sym, color });
                            }
                        }
                    }
                    Token::RBrace | Token::LBracket => break,
                    _ => { iter.next(); }
                }
            }
        }
    }

    nodes
}

/// Handle `add Layers(...) {sym}` and `add Colors { ... }`
fn parse_add(iter: &mut Peekable<IntoIter<Token>>) -> Vec<AstNode> {
    let mut nodes = Vec::new();
    iter.next(); // consume "add"

    if let Some(Token::Ident(kind)) = iter.next() {
        if kind == "Layers" {
            // parse numbers inside (...)
            let mut nums = Vec::new();
            if let Some(Token::LParen) = iter.next() {
                while let Some(tok2) = iter.next() {
                    match tok2 {
                        Token::NumberLit(n) => nums.push(n),
                        Token::Comma => continue,
                        Token::RParen => break,
                        _ => {}
                    }
                }
            }

            // parse symbol inside {...}
            let mut symbol = String::new();
            if let Some(Token::LBrace) = iter.next() {
                if let Some(Token::Ident(sym)) = iter.next() {
                    symbol = sym;
                }
                let _ = iter.next(); // RBrace
            }

            if nums.len() == 3 {
                nodes.push(AstNode::AddLayer {
                    x: nums[0],
                    y: nums[1],
                    z: nums[2],
                    symbol,
                });
            }
        } else if kind == "Colors" {
            if let Some(Token::LBrace) = iter.next() {
                while let Some(tok2) = iter.peek() {
                    match tok2 {
                        Token::Ident(sym) => {
                            let sym = sym.clone();
                            iter.next();
                            if let Some(Token::Colon) = iter.next() {
                                if let Some(Token::Ident(color)) = iter.next() {
                                    nodes.push(AstNode::AddColor { symbol: sym, color });
                                }
                            }
                        }
                        Token::RBrace => { iter.next(); break; }
                        _ => { iter.next(); }
                    }
                }
            }
        }
    }

    nodes
}

/// Handle `for ... in ... { ... }`
fn parse_for(iter: &mut Peekable<IntoIter<Token>>) -> AstNode {
    iter.next(); // consume "for"

    let var1 = if let Some(Token::Ident(v)) = iter.next() { v } else { "".into() };
    let mut var2 = String::new();
    if let Some(Token::Comma) = iter.peek() {
        iter.next();
        if let Some(Token::Ident(v)) = iter.next() { var2 = v; }
    }

    let _ = iter.next(); // consume "in"

    if let Some(Token::Ident(id)) = iter.next() {
        if id == "range" {
            let _ = iter.next(); // LParen
            let mut nums = Vec::new();
            while let Some(tok2) = iter.next() {
                match tok2 {
                    Token::NumberLit(n) => nums.push(n),
                    Token::Comma => continue,
                    Token::RParen => break,
                    _ => {}
                }
            }

            let _ = iter.next(); // colon
            let _ = iter.next(); // LBrace
            let inner = parse_voxel_body(iter);

            if nums.len() == 2 {
                AstNode::ForRange { var: var1, start: nums[0], end: nums[1], body: inner }
            } else {
                AstNode::Ident("invalid_range".into())
            }
        } else {
            // for a,b in arr1, arr2
            let iter1 = id;
            let _ = iter.next(); // comma
            let iter2 = if let Some(Token::Ident(v)) = iter.next() { v } else { "".into() };
            let _ = iter.next(); // colon
            let _ = iter.next(); // LBrace
            let inner = parse_voxel_body(iter);

            AstNode::ForLoop { var1, var2, iter1, iter2, body: inner }
        }
    } else {
        AstNode::Ident("invalid_for".into())
    }
}
