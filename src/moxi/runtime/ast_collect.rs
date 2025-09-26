use std::collections::HashMap;
use super::super::parser::AstNode;

pub struct CollectedAst {
    pub models: Vec<ModelAst>,
    pub commands: Vec<(String, Vec<String>)>,
}

pub struct ModelAst {
    pub name: String,
    pub explicit_colors: HashMap<String, String>,
    pub layers: Vec<(usize, Vec<String>)>,
}

pub fn collect_ast(ast: &[AstNode]) -> CollectedAst {
    let mut models = Vec::new();
    let mut commands = Vec::new();
    let mut current_model: Option<ModelAst> = None;

    for node in ast {
        match node {
            AstNode::VoxelDecl { name, .. } => {
                if let Some(m) = current_model.take() {
                    models.push(m);
                }
                current_model = Some(ModelAst {
                    name: name.clone(),
                    explicit_colors: HashMap::new(),
                    layers: Vec::new(),
                });
            }
            AstNode::ColorDecl { symbol, color } => {
                if let Some(m) = current_model.as_mut() {
                    m.explicit_colors.insert(symbol.clone(), color.clone());
                }
            }
            AstNode::LayerDecl { z, rows } => {
                if let Some(m) = current_model.as_mut() {
                    m.layers.push((*z, rows.clone()));
                }
            }
            _ => {}
        }
    }

    if let Some(m) = current_model.take() {
        models.push(m);
    }

    CollectedAst { models, commands }
}
