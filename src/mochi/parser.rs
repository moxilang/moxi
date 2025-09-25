use super::lexer::Token;

#[derive(Debug, Clone)]
pub enum AstNode {
    VoxelDecl { name: String },
    LayerDecl { z: usize, rows: Vec<String> },
    ColorDecl { symbol: String, color: String },
}

pub fn parse(tokens: Vec<Token>) -> Vec<AstNode> {
    let mut ast = Vec::new();
    let mut iter = tokens.into_iter().peekable();

    let mut current_layer: Option<(usize, Vec<String>)> = None;
    let mut in_colors = false;

    while let Some(token) = iter.next() {
        match token {
            Token::Keyword(ref k) if k == "voxel" => {
                if let Some(Token::Ident(name)) = iter.next() {
                    ast.push(AstNode::VoxelDecl { name });
                }
            }

            // Start of [Layer N]
            Token::LBracket => {
                if let Some(Token::Ident(word)) = iter.next() {
                    if word == "Layer" {
                        if let Some(Token::Ident(n_str)) = iter.next() {
                            if let Ok(z) = n_str.parse::<usize>() {
                                current_layer = Some((z, Vec::new()));
                            }
                        }
                    } else if word == "Colors" {
                        in_colors = true;
                    }
                }
                // eat the RBracket after Layer/Colors
                let _ = iter.next();
            }

            // Collect rows into current_layer
            Token::Ident(row) if current_layer.is_some() && !in_colors => {
                if let Some((_z, ref mut rows)) = current_layer {
                    rows.push(row);
                }
            }

            // If we see another [ starting a new block, flush the old layer first
            Token::LBrace | Token::RBrace | Token::LBracket => {
                if let Some((z, rows)) = current_layer.take() {
                    ast.push(AstNode::LayerDecl { z, rows });
                }
                // reprocess this token on the next loop
            }

            // Handle Colors mappings
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
