use crate::error::Span;

/// Every token kind the Moxi lexer can produce.
///
/// Design rule: keywords are their own variants, not `Ident("atom")`.
/// This makes the parser's match arms exhaustive and unambiguous.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // ── Literals ───────────────────────────────────────────────────────────
    /// An identifier or user-defined name:  `Tree`, `Trunk`, `BONE`
    Ident(String),
    /// An integer literal:  `42`, `200`
    Int(i64),
    /// A floating-point literal:  `3.14`, `0.5`
    Float(f64),
    /// A quoted string:  `"hello"`
    StringLit(String),

    // ── v1 keywords (assembly layer) ───────────────────────────────────────
    Atom,
    Legend,
    Voxel,
    Translate,
    Merge,
    Print,

    // ── v2 keywords (semantic layer) ───────────────────────────────────────
    Entity,
    Part,
    Relation,
    Constraint,
    Shape,
    Material,
    Generator,
    World,
    Refine,
    Detail,
    Biome,
    Terrain,
    Water,
    Resolve,
    Scatter,
    Over,
    Where,
    Avoid,
    Parts,
    Attach,

    // ── Built-in shape names ───────────────────────────────────────────────
    Box_,        // `box` is a Rust keyword, trailing underscore
    Sphere,
    Cylinder,
    Cone,
    Ellipsoid,
    Blob,
    Heightfield,
    Shell,
    Extrude,

    // ── Built-in relation keywords ─────────────────────────────────────────
    Inside,
    Outside,
    AdjacentTo,
    Above,
    Below,
    LeftOf,
    RightOf,
    InFrontOf,
    Behind,
    SymmetricAcross,
    AttachedTo,
    Touch,
    Surrounds,

    // ── Punctuation ────────────────────────────────────────────────────────
    LBrace,    // {
    RBrace,    // }
    LParen,    // (
    RParen,    // )
    LBracket,  // [
    RBracket,  // ]
    Comma,     // ,
    Dot,       // .
    Eq,        // =
    EqEq,      // ==
    Neq,       // !=
    Lt,        // <
    Gt,        // >
    LtEq,      // <=
    GtEq,      // >=
    And,       // and
    Or,        // or
    Not,       // not
    Plus,      // +
    Minus,     // -
    Star,      // *
    Slash,     // /

    // ── Special ────────────────────────────────────────────────────────────
    /// The newline in a voxel layer grid row (significant whitespace)
    LayerRow(String),
    /// End of file
    Eof,
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenKind::Ident(s)     => write!(f, "{s}"),
            TokenKind::Int(n)       => write!(f, "{n}"),
            TokenKind::Float(n)     => write!(f, "{n}"),
            TokenKind::StringLit(s) => write!(f, "\"{s}\""),
            TokenKind::Eof          => write!(f, "<eof>"),
            other                   => write!(f, "{other:?}"),
        }
    }
}

/// A token with its source location attached.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}