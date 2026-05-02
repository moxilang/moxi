use crate::error::Span;

/// A name with the source location where it was written.
#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

// ── Top-level ──────────────────────────────────────────────────────────────

/// A complete parsed `.mi` file.
#[derive(Debug, Clone)]
pub struct Document {
    pub items: Vec<TopLevel>,
}

/// Everything that can appear at the top level of a Moxi file.
#[derive(Debug, Clone)]
pub enum TopLevel {
    // v1 assembly layer
    AtomDecl(AtomDecl),
    VoxelDecl(VoxelDecl),

    // v2 semantic layer
    MaterialDecl(MaterialDecl),
    EntityDecl(EntityDecl),
    GeneratorDecl(GeneratorDecl),
    WorldDecl(WorldDecl),

    // Statements
    PrintStmt(PrintStmt),
    RefineStmt(RefineStmt),
}

// ── v1 assembly layer ──────────────────────────────────────────────────────

/// `atom BONE { color = ivory }`
#[derive(Debug, Clone)]
pub struct AtomDecl {
    pub name: Ident,
    pub props: Vec<Prop>,
    pub span: Span,
}

/// `voxel PalmTree { legend { … } [Layer 0] … }`
#[derive(Debug, Clone)]
pub struct VoxelDecl {
    pub name: Ident,
    pub legend: Vec<LegendEntry>,
    pub layers: Vec<VoxelLayer>,
    pub span: Span,
}

/// A single glyph → atom mapping inside a `legend` block.
#[derive(Debug, Clone)]
pub struct LegendEntry {
    pub glyph: char,
    pub atom: Ident,
}

/// One horizontal layer in a voxel block, with its index and rows.
#[derive(Debug, Clone)]
pub struct VoxelLayer {
    pub index: i64,
    pub rows: Vec<String>,
}

// ── v2 semantic layer ──────────────────────────────────────────────────────

/// `material Bark { color = brown, roughness = high, voxel_atom = TRUNK }`
#[derive(Debug, Clone)]
pub struct MaterialDecl {
    pub name: Ident,
    pub props: Vec<Prop>,
    pub span: Span,
}

/// `entity HumanBody { parts = […], … constraints … }`
#[derive(Debug, Clone)]
pub struct EntityDecl {
    pub name: Ident,
    pub parts: Vec<PartDecl>,
    pub relations: Vec<RelationStmt>,
    pub constraints: Vec<ConstraintStmt>,
    pub resolve: Option<ResolveOpts>,
    pub span: Span,
}

/// `part Skull { shape = sphere(radius=4), material = Bone }`
#[derive(Debug, Clone)]
pub struct PartDecl {
    pub name: Ident,
    pub shape: Option<ShapeExpr>,
    pub material: Option<Ident>,
    pub anchor: Option<Ident>,
    pub attach_to: Option<AttachSpec>,
    pub span: Span,
}

/// `attach_to = top_of(Trunk)`
#[derive(Debug, Clone)]
pub struct AttachSpec {
    pub anchor_fn: String,
    pub target: Ident,
}

// ── Shape expressions ──────────────────────────────────────────────────────

/// Any shape primitive with its named arguments.
#[derive(Debug, Clone)]
pub enum ShapeExpr {
    Box_     { args: Vec<NamedArg> },
    Sphere   { args: Vec<NamedArg> },
    Cylinder { args: Vec<NamedArg> },
    Cone     { args: Vec<NamedArg> },
    Ellipsoid{ args: Vec<NamedArg> },
    Blob     { args: Vec<NamedArg> },
    Heightfield { args: Vec<NamedArg> },
    Shell    { inner: Box<ShapeExpr>, args: Vec<NamedArg> },
    Extrude  { profile: Box<ShapeExpr>, args: Vec<NamedArg> },
}

/// A `key = value` argument inside a shape call.
#[derive(Debug, Clone)]
pub struct NamedArg {
    pub key: String,
    pub value: Expr,
}

// ── Relation and constraint statements ────────────────────────────────────

/// One spatial relationship between two named parts.
#[derive(Debug, Clone)]
pub struct RelationStmt {
    pub subject: Ident,
    pub predicate: RelationKind,
    pub object: Ident,
    pub qualifiers: Vec<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RelationKind {
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
}

/// A hard rule that the resolved geometry must satisfy.
#[derive(Debug, Clone)]
pub struct ConstraintStmt {
    pub expr: ConstraintExpr,
    pub span: Span,
}

/// Constraints are either spatial relations or numeric bounds.
#[derive(Debug, Clone)]
pub enum ConstraintExpr {
    Relation(RelationStmt),
    Bound { name: Ident, op: CmpOp, value: Expr },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CmpOp { Lt, Gt, LtEq, GtEq, Eq, Neq }

// ── Generator ─────────────────────────────────────────────────────────────

/// `generator Forest { scatter Tree count=200 over=Terrain where=… }`
#[derive(Debug, Clone)]
pub struct GeneratorDecl {
    pub name: Ident,
    pub scatter_target: Ident,
    pub props: Vec<Prop>,
    pub span: Span,
}

// ── World ──────────────────────────────────────────────────────────────────

/// `world TropicalIsland { … }`
#[derive(Debug, Clone)]
pub struct WorldDecl {
    pub name: Ident,
    pub scale: Option<Ident>,
    pub sea_level: Option<Expr>,
    pub terrain: Option<TerrainBlock>,
    pub biomes: Vec<BiomeBlock>,
    pub water: Option<WaterBlock>,
    pub resolve: Option<ResolveOpts>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct TerrainBlock {
    pub base: ShapeExpr,
    pub max_elevation: Option<Expr>,
    pub edge_falloff: Option<Ident>,
}

#[derive(Debug, Clone)]
pub struct BiomeBlock {
    pub name: Ident,
    pub condition: Expr,
    pub surface_material: Option<Ident>,
    pub generator: Option<Ident>,
}

#[derive(Debug, Clone)]
pub struct WaterBlock {
    pub level: Expr,
    pub material: Option<Ident>,
    pub depth_material: Option<Ident>,
}

/// `resolve voxel_size = 1.0`
#[derive(Debug, Clone)]
pub struct ResolveOpts {
    pub voxel_size: f64,
}

// ── Statements ─────────────────────────────────────────────────────────────

/// `print HumanBody detail=low`
#[derive(Debug, Clone)]
pub struct PrintStmt {
    pub target: Ident,
    pub detail: Option<DetailLevel>,
    pub span: Span,
}

/// `refine HumanBody.Chest detail=medium`
#[derive(Debug, Clone)]
pub struct RefineStmt {
    pub path: Vec<Ident>,
    pub detail: DetailLevel,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DetailLevel {
    Sketch,
    Low,
    Medium,
    High,
}

// ── Shared primitives ──────────────────────────────────────────────────────

/// A generic `key = value` property (used in atom, material, generator).
#[derive(Debug, Clone)]
pub struct Prop {
    pub key: String,
    pub value: Expr,
    pub span: Span,
}

/// Any value expression in Moxi.
#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Str(String),
    Ident(Ident),
    /// `elevation < 30 and slope < 25`
    BinOp { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    /// `not x`
    Not(Box<Expr>),
    /// `noise(scale=0.1)`
    Call { name: String, args: Vec<NamedArg> },
    /// `[Tree, Leaf]`
    List(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    And, Or,
    Add, Sub, Mul, Div,
    Lt, Gt, LtEq, GtEq, Eq, Neq,
}