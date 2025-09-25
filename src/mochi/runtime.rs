use super::parser::AstNode;

pub fn run(ast: Vec<AstNode>) {
    for node in ast {
        println!("Executing node: {:?}", node);
    }
}
