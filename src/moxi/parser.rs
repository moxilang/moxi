use super::lexer::Token;

#[derive(Debug, Clone)]
pub enum AstNode {
    VoxelDecl { name: String, params: Vec<String>, body: Vec<AstNode> },

    Assignment { name: String, expr: Box<AstNode> },
    FunctionCall { name: String, args: Vec<AstNode> },

    Ident(String),
    StringLit(String),
    NumberLit(i32),
    ArrayLit(Vec<AstNode>),

    LayerDecl { z: usize, rows: Vec<String> },
    ColorDecl { symbol: String, color: String },

    AddLayer { x: i32, y: i32, z: i32, symbol: String },
    AddColor { symbol: String, color: String },

    ForLoop { var1: String, var2: String, iter1: String, iter2: String, body: Vec<AstNode> },
    ForRange { var: String, start: i32, end: i32, body: Vec<AstNode> },

    Print { target: Option<String> },
}

fn parse_expression(iter: &mut std::iter::Peekable<std::vec::IntoIter<Token>>) -> AstNode {
    let tok_opt = iter.peek().cloned();
    if let Some(tok) = tok_opt {
        match tok {
            Token::Ident(id) => {
                iter.next();
                if let Some(Token::LParen) = iter.peek() {
                    iter.next();
                    let mut args = Vec::new();
                    while let Some(tok2) = iter.next() {
                        match tok2 {
                            Token::Ident(arg) => args.push(AstNode::Ident(arg)),
                            Token::StringLit(s) => args.push(AstNode::StringLit(s)),
                            Token::NumberLit(n) => args.push(AstNode::NumberLit(n)),
                            Token::Comma => continue,
                            Token::RParen => break,
                            _ => {}
                        }
                    }
                    AstNode::FunctionCall { name: id, args }
                } else {
                    AstNode::Ident(id)
                }
            }
            Token::StringLit(s) => { iter.next(); AstNode::StringLit(s) }
            Token::NumberLit(n) => { iter.next(); AstNode::NumberLit(n) }
            Token::LBracket => {
                iter.next();
                let mut elems = Vec::new();
                while let Some(tok2) = iter.next() {
                    match tok2 {
                        Token::StringLit(s) => elems.push(AstNode::StringLit(s)),
                        Token::Ident(i) => elems.push(AstNode::Ident(i)),
                        Token::NumberLit(n) => elems.push(AstNode::NumberLit(n)),
                        Token::Comma => continue,
                        Token::RBracket => break,
                        _ => {}
                    }
                }
                AstNode::ArrayLit(elems)
            }
            _ => { iter.next(); AstNode::Ident("unknown".into()) }
        }
    } else {
        AstNode::Ident("empty".into())
    }
}



pub fn parse(tokens: Vec<Token>) -> Vec<AstNode> {
    let mut ast = Vec::new();
    let mut iter = tokens.into_iter().peekable();

    fn parse_voxel_body(iter: &mut std::iter::Peekable<std::vec::IntoIter<Token>>) -> Vec<AstNode> {
        let mut body = Vec::new();

        while let Some(tok) = iter.peek() {
            match tok {
                Token::RBrace => {
                    iter.next();
                    break;
                }

                // [Layer N] or [Colors]
                Token::LBracket => {
                    iter.next();
                    if let Some(Token::Ident(word)) = iter.next() {
                        if word == "Layer" {
                            if let Some(Token::NumberLit(z)) = iter.next() {
                                let _ = iter.next(); // RBracket
                                let mut rows = Vec::new();
                                while let Some(tok2) = iter.peek() {
                                    match tok2 {
                                        // Only accept plain identifiers as rows
                                        Token::Ident(row) => {
                                            if row == "add" || row == "for" {
                                                // stop if we hit a command
                                                break;
                                            }
                                            rows.push(row.clone());
                                            iter.next();
                                        }
                                        // stop if another block or end-of-voxel
                                        Token::LBracket | Token::RBrace => break,
                                        _ => {
                                            iter.next();
                                        }
                                    }
                                }
                                body.push(AstNode::LayerDecl {
                                    z: z as usize,
                                    rows,
                                });
                            }
                        } else if word == "Colors" {
                            let _ = iter.next(); // RBracket
                            while let Some(tok2) = iter.peek() {
                                match tok2 {
                                    Token::Ident(sym) => {
                                        let sym = sym.clone();
                                        iter.next();
                                        if let Some(Token::Colon) = iter.next() {
                                            if let Some(Token::Ident(color)) = iter.next() {
                                                body.push(AstNode::ColorDecl {
                                                    symbol: sym,
                                                    color,
                                                });
                                            }
                                        }
                                    }
                                    Token::RBrace | Token::LBracket => break,
                                    _ => {
                                        iter.next();
                                    }
                                }
                            }
                        }
                    }
                }

                // add Layers(x,y,z){sym} or add Colors { ... }
                Token::Ident(word) if word == "add" => {
                    iter.next();
                    if let Some(Token::Ident(kind)) = iter.next() {
                        if kind == "Layers" {
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
                            let mut symbol = String::new();
                            if let Some(Token::LBrace) = iter.next() {
                                if let Some(Token::Ident(sym)) = iter.next() {
                                    symbol = sym;
                                }
                                let _ = iter.next(); // RBrace
                            }
                            if nums.len() == 3 {
                                body.push(AstNode::AddLayer {
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
                                                    body.push(AstNode::AddColor {
                                                        symbol: sym,
                                                        color,
                                                    });
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
                        }
                    }
                }

                // for i in range(...) or for a,b in arr1, arr2
                Token::Keyword(word) if word == "for" => {
                    iter.next();
                    let var1 = if let Some(Token::Ident(v)) = iter.next() {
                        v
                    } else {
                        "".into()
                    };
                    let mut var2 = String::new();
                    if let Some(Token::Comma) = iter.peek() {
                        iter.next();
                        if let Some(Token::Ident(v)) = iter.next() {
                            var2 = v;
                        }
                    }
                    let _ = iter.next(); // in

                    // detect range(start,end) vs arrays
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
                                body.push(AstNode::ForRange {
                                    var: var1,
                                    start: nums[0],
                                    end: nums[1],
                                    body: inner,
                                });
                            }
                        } else {
                            let iter1 = id;
                            let _ = iter.next(); // comma
                            let iter2 = if let Some(Token::Ident(v)) = iter.next() {
                                v
                            } else {
                                "".into()
                            };
                            let _ = iter.next(); // colon
                            let _ = iter.next(); // LBrace
                            let inner = parse_voxel_body(iter);
                            body.push(AstNode::ForLoop {
                                var1,
                                var2,
                                iter1,
                                iter2,
                                body: inner,
                            });
                        }
                    }
                }

                _ => {
                    iter.next();
                }
            }
        }

        body
    }

    while let Some(token) = iter.next() {
        match token {
            Token::Keyword(ref k) if k == "voxel" => {
                if let Some(Token::Ident(name)) = iter.next() {
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
                    if let Some(Token::LBrace) = iter.next() {
                        let body = parse_voxel_body(&mut iter);
                        ast.push(AstNode::VoxelDecl { name, params, body });
                    }
                }
            }
                        
            Token::Ident(var) => {
                if let Some(Token::Equals) = iter.peek() {
                    iter.next(); // '='
                    let expr = parse_expression(&mut iter);
                    ast.push(AstNode::Assignment { name: var, expr: Box::new(expr) });
                }
            }

            Token::Keyword(ref k) if k == "print" => {
                let mut target = None;
                if let Some(Token::Ident(id)) = iter.peek() {
                    target = Some(id.clone());
                    iter.next();
                }
                ast.push(AstNode::Print { target });
            }
            _ => {}
        }
    }

    ast
}
