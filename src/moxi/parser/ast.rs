#[derive(Debug, Clone)]
pub enum AstNode {
    // === New ===
    AtomDecl {
        name: String,
        props: Vec<(String, String)>, // e.g. ("color", "brown")
    },

    // Models
    VoxelDecl { name: String, params: Vec<String>, body: Vec<AstNode> },

    // Expressions & calls
    Assignment { name: String, expr: Box<AstNode> },
    FunctionCall { name: String, args: Vec<AstNode> },

    Ident(String),
    StringLit(String),
    NumberLit(i32),
    ArrayLit(Vec<AstNode>),
    KVArgs(Vec<(String, AstNode)>),

    // Voxel internals
    LayerDecl { z: usize, rows: Vec<String> },
    ColorDecl { symbol: String, color: String },

    AddLayer { x: i32, y: i32, z: i32, symbol: String },
    AddColor { symbol: String, color: String },

    // Loops
    ForLoop { var1: String, var2: String, iter1: String, iter2: String, body: Vec<AstNode> },
    ForRange { var: String, start: i32, end: i32, body: Vec<AstNode> },

    // Commands
    Print { target: Option<String> },
}
