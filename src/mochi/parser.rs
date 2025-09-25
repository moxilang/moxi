use super::lexer::Token;

#[derive(Debug, Clone)]
pub enum AstNode {
    VoxelDecl { name: String },
    LayerDecl { z: usize, rows: Vec<String> },
    ColorDecl { symbol: String, color: String },
    Command { name: String, args: Vec<String> }, 
}

pub fn parse(tokens: Vec<Token>) -> Vec<AstNode> {
    let mut ast = Vec::new();
    let mut iter = tokens.into_iter().peekable();

    let mut current_layer: Option<(usize, Vec<String>)> = None;
    let mut in_colors = false;

    while let Some(token) = iter.next() {
        match token {
            // voxel Name { ... }
            Token::Keyword(ref k) if k == "voxel" => {
                if let Some(Token::Ident(name)) = iter.next() {
                    ast.push(AstNode::VoxelDecl { name });
                }
            }

            // Start of [Layer N] or [Colors]
            Token::LBracket => {
                if let Some(Token::Ident(word)) = iter.next() {
                    if word == "Layer" {
                        if let Some(Token::Ident(n_str)) = iter.next() {
                            if let Ok(z) = n_str.parse::<usize>() {
                                // flush previous layer if still open
                                if let Some((z_prev, rows)) = current_layer.take() {
                                    ast.push(AstNode::LayerDecl { z: z_prev, rows });
                                }
                                current_layer = Some((z, Vec::new()));
                            }
                        }
                    } else if word == "Colors" {
                        // flush any open layer before starting Colors
                        if let Some((z_prev, rows)) = current_layer.take() {
                            ast.push(AstNode::LayerDecl { z: z_prev, rows });
                        }
                        in_colors = true;
                    }
                }
                // eat the closing bracket
                let _ = iter.next();
            }

            // Rows inside a [Layer]
            Token::Ident(row) if current_layer.is_some() && !in_colors => {
                if let Some((_z, ref mut rows)) = current_layer {
                    rows.push(row);
                }
            }

            // Color mappings inside [Colors]
            Token::Ident(symbol) if in_colors => {
                if let Some(Token::Colon) = iter.next() {
                    if let Some(Token::Ident(color)) = iter.next() {
                        ast.push(AstNode::ColorDecl { symbol, color });
                    }
                }
            }

            Token::EOF => {
                if let Some((z, rows)) = current_layer.take() {
                    ast.push(AstNode::LayerDecl { z, rows });
                }
                break;
            }
            _ => {}
        }
    }

    ast
}
