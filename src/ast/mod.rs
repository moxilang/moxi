use crate::error::Span;
use crate::geom::Axis;

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
///
/// `WorldDecl` is boxed: at ~440 bytes it is more than twice the next-largest
/// variant, and every element of a `Vec<TopLevel>` would otherwise pay for it.
#[derive(Debug, Clone)]
pub enum TopLevel {
    // v1 assembly layer
    AtomDecl(AtomDecl),
    VoxelDecl(VoxelDecl),

    // v2 semantic layer
    MaterialDecl(MaterialDecl),
    EntityDecl(EntityDecl),
    GeneratorDecl(GeneratorDecl),
    WorldDecl(Box<WorldDecl>),

    // Statements
    PrintStmt(PrintStmt),
    RefineStmt(RefineStmt),
    FnDecl(FnDecl),
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

/// `entity HumanBody(scale=1) { part … relation { … } anchor … resolve … }`
///
/// `params` declare entity parameters with REQUIRED default values —
/// `entity Arm(length=9, girth=0.8)`. Parameters substitute into shape
/// arguments and relation anchor arguments; instances override them:
/// `part RightArm { entity = Arm(length=12) }`.
///
/// `anchors` are the entity's EXPORTED sockets — named frames on internal
/// parts that placements outside the entity may reference when this entity
/// is instanced (`RightArm.socket on Ribcage.east`).
#[derive(Debug, Clone)]
pub struct EntityDecl {
    pub name: Ident,
    pub params: Vec<Prop>,
    /// `let NAME = expr` bindings, in declaration order. Each sees the
    /// parameters and every earlier `let`. Re-evaluated per instance when
    /// parameters are overridden.
    pub lets: Vec<Prop>,
    pub parts: Vec<PartDecl>,
    pub relations: Vec<Placement>,
    pub constraints: Vec<ConstraintStmt>,
    pub anchors: Vec<AnchorDecl>,
    pub resolve: Option<ResolveOpts>,
    /// Phase E3: `for` blocks, unrolled by the resolver.
    pub loops: Vec<ForBlock>,
    /// Phase E3: index expressions for indexed names (`RibR[i]`). The
    /// parser stores each in this table and leaves a marker `[#k]` in the
    /// name; the resolver folds marker k once the loop variable is bound
    /// and rewrites the name to `RibR[3]`. This keeps indices out of every
    /// other AST type.
    pub index_exprs: Vec<Expr>,
    /// Living models, step 2: named poses — qualifier overrides on this
    /// thing's own mates (DOC-20261004-living-models-design §3.1).
    pub poses: Vec<PoseDecl>,
    pub span: Span,
}

// ── Poses ──────────────────────────────────────────────────────────────

/// `pose Smash { ArmR lean=(110, 10)  HandR twist=20  for k in 1..n { … } }`
///
/// A pose is the thing with some mate qualifiers set to other values,
/// solved again. Semantics are textual substitution: the solved frames of
/// `pose P` equal those of the script with P's values written into the
/// mate lines.
#[derive(Debug, Clone)]
pub struct PoseDecl {
    pub name:  Ident,
    pub lines: Vec<PoseLine>,
    pub loops: Vec<PoseFor>,
    pub span:  Span,
}

/// One line of a pose: a part (indexed names carry `[#k]` markers into
/// the thing's index table, as everywhere else) and what it sets.
#[derive(Debug, Clone)]
pub struct PoseLine {
    pub part: Ident,
    pub set:  PoseSet,
}

#[derive(Debug, Clone)]
pub enum PoseSet {
    /// `ArmR lean=(110, 10) twist=20` (boxed: clippy's
    /// `large_enum_variant` is a gate)
    Quals(Box<PoseQuals>),
    /// `WingFR pose=Up` — an instance takes one of its thing's poses.
    Pose(Ident),
}

/// The qualifiers a pose may set; `None` = keep the mate's own value.
#[derive(Debug, Clone, Default)]
pub struct PoseQuals {
    pub twist: Option<Expr>,
    pub pitch: Option<Expr>,
    pub gap:   Option<Expr>,
    pub shift: Option<(Expr, Expr)>,
    pub lean:  Option<(Expr, Expr)>,
}

impl PoseQuals {
    pub fn is_empty(&self) -> bool {
        self.twist.is_none() && self.pitch.is_none() && self.gap.is_none()
            && self.shift.is_none() && self.lean.is_none()
    }
}

/// `for k in A..B { … }` inside a pose: lines and nested loops only.
#[derive(Debug, Clone)]
pub struct PoseFor {
    pub var:   Ident,
    pub start: Expr,
    pub end:   Expr,
    pub lines: Vec<PoseLine>,
    pub loops: Vec<PoseFor>,
    pub span:  Span,
}

/// A part is EITHER a shape (`shape = sphere(radius=4)`) OR an instance of
/// a previously declared entity (`entity = Arm`) — never both. Instancing
/// is the composition rung: an entity's parts can be entities, so worlds
/// build as deep trees of reusable semantic units.
#[derive(Debug, Clone)]
pub struct PartDecl {
    pub name: Ident,
    pub shape: Option<ShapeExpr>,
    pub entity: Option<Ident>,
    /// Parameter overrides for an instance: `entity = Arm(length=12)`.
    pub entity_args: Vec<NamedArg>,
    pub material: Option<Ident>,
    pub span: Span,
}

/// `anchor socket = Humerus.top` — an entity-level anchor export.
/// The exported name becomes usable on instances of this entity:
/// `RightArm.socket` resolves to `RightArm.Humerus.top`.
#[derive(Debug, Clone)]
pub struct AnchorDecl {
    pub name: Ident,
    pub target: AnchorRef,
    pub span: Span,
}

// ── Shape expressions ──────────────────────────────────────────────────────

/// Any shape primitive with its named arguments.
///
/// Phase B2: shapes are containment predicates, so CSG is composition —
/// `Union`/`Difference`/`Intersect` combine children, and `At`/`Spin`
/// place them in the parent shape's local space. Anchors follow the FIRST
/// operand (the base, for `Difference`), transformed by any wrapper.
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

    // CSG combinators (Phase B2)
    /// `union(a, b, …, blend=k)` — filled where ANY child is. With
    /// `blend`, the join is a smooth fillet `k` wide instead of a crease:
    /// two spheres become a shoulder. Blend has no predicate form, so
    /// `contains` defers to `distance` for it.
    Union     { shapes: Vec<ShapeExpr>, args: Vec<NamedArg> },

    // Sculptor primitives
    /// `capsule(height=, radius=)` — a sphere-swept line: base at origin,
    /// axis +Y, straight segment of `height`, rounded caps of `radius`
    /// extending beyond each end. Total extent is `height + 2*radius`.
    Capsule   { args: Vec<NamedArg> },
    /// `torus(major_radius=, minor_radius=)` — centered, ring in the XZ
    /// plane, axis +Y.
    Torus     { args: Vec<NamedArg> },
    /// `difference(base, cut, …)` — the base minus every cut.
    Difference{ base: Box<ShapeExpr>, cuts: Vec<ShapeExpr> },
    /// `intersect(a, b, …)` — filled where ALL children are.
    Intersect { shapes: Vec<ShapeExpr> },

    // Local transform wrappers (Phase B2)
    /// `at(shape, x=…, y=…, z=…)` — translate the child in local space.
    At        { inner: Box<ShapeExpr>, args: Vec<NamedArg> },
    /// `spin(shape, axis=x|y|z, degrees=…)` — rotate the child about its
    /// local origin.
    Spin      { inner: Box<ShapeExpr>, args: Vec<NamedArg> },
    /// `mirror(shape, axis=x)` — reflect the child across the plane through
    /// its local origin with that normal (`axis=` or explicit `nx, ny, nz`).
    /// `symmetric_across` wraps its subject in one, which is what makes a
    /// mirrored part a mirror IMAGE rather than a copy in a mirrored place.
    Mirror    { inner: Box<ShapeExpr>, args: Vec<NamedArg> },
    /// `scale(shape, x=…, y=…, z=…)` — stretch the child per axis about its
    /// local origin. Default 1. Not an isometry: distance becomes a bound.
    Scale     { inner: Box<ShapeExpr>, args: Vec<NamedArg> },
}

/// A `key = value` argument inside a shape call.
#[derive(Debug, Clone)]
pub struct NamedArg {
    pub key: String,
    pub value: Expr,
}

// ── Placement statements ───────────────────────────────────────────────────
//
// The ENTIRE placement language: two node types. The 13 relation keywords
// are surface sugar, desugared at parse time (parser::desugar_placement) —
// the AST only ever contains Align and Mirror. RelationStmt below survives
// solely as the CONSTRAINT predicate vocabulary: same words, evaluated as
// checks instead of assignments.

/// `Skull.bottom`, `Trunk.side(t=0.7, angle=90)` — a named frame on a part.
#[derive(Debug, Clone)]
pub struct AnchorRef {
    pub part:   String,
    pub anchor: String,
    pub args:   Vec<NamedArg>,
    pub span:   Span,
}

/// The face-relative parts of a mate. Both pairs are `(a, b)` in the same
/// named directions — up/right/front per face, see the Placement section of
/// SKILL.md — so one table explains both.
#[derive(Debug, Clone)]
pub struct MateOffsets {
    /// Slide across the face, world units. `gap` is the same translation
    /// along the normal. This is what turns an anchor from a point into a
    /// patch.
    pub shift: (Expr, Expr),
    /// Lean the part's axis toward `a`, then toward `b`, in degrees. On a
    /// trunk's `side`, `(45, 0)` is 45° upward. Unlike `pitch` it never
    /// sweeps the part around a curved shape, and unlike `twist` + `pitch`
    /// it never rolls it.
    pub lean:  (Expr, Expr),
}

impl MateOffsets {
    pub fn zero() -> Self {
        MateOffsets { shift: (Expr::Float(0.0), Expr::Float(0.0)), lean: (Expr::Float(0.0), Expr::Float(0.0)) }
    }
    pub fn shift_only(a: Expr, b: Expr) -> Self {
        MateOffsets { shift: (a, b), lean: (Expr::Float(0.0), Expr::Float(0.0)) }
    }
}

#[derive(Debug, Clone)]
pub enum Placement {
    /// Mate two anchors: subject anchor coincides with object anchor,
    /// normals opposed (unless an anchor is orientation-free).
    Align {
        subject: AnchorRef,
        object:  AnchorRef,
        /// Rotation about the socket normal, degrees.
        ///
        /// Qualifiers are EXPRESSIONS, folded to constants at resolve
        /// time, so a thing can parameterize its own pose:
        /// `Lower.top on Upper.bottom pitch=bend`. By the time the frame
        /// solver sees a placement every one of these is a literal.
        twist:   Expr,
        /// Tilt off the socket normal about the tangent X, degrees.
        pitch:   Expr,
        /// Separation along the socket normal, WORLD units. 0 = touching.
        gap:     Expr,
        /// `shift` and `lean`: the face-relative parts of the mate. Boxed
        /// together so `Placement` stays small — clippy's
        /// `large_enum_variant` is a gate.
        offsets: Box<MateOffsets>,
        span:    Span,
    },
    /// Reflect the SOLVED frame of `source` across the plane through
    /// `plane`'s anchor point.
    Mirror {
        subject: String,
        source:  String,
        plane:   AnchorRef,
        /// Plane normal, in the plane part's local space. Default X =
        /// bilateral (left/right) symmetry.
        axis:    Axis,
        span:    Span,
    },
}

impl Placement {
    pub fn subject_name(&self) -> &str {
        match self {
            Placement::Align { subject, .. } => &subject.part,
            Placement::Mirror { subject, .. } => subject,
        }
    }

    pub fn object_name(&self) -> &str {
        match self {
            Placement::Align { object, .. } => &object.part,
            // Mirror depends on BOTH source and plane parts; source is the
            // primary edge, the plane part is added in the solver's graph.
            Placement::Mirror { source, .. } => source,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Placement::Align { span, .. } | Placement::Mirror { span, .. } => *span,
        }
    }
}

// ── Constraint statements ──────────────────────────────────────────────────

/// Constraint predicate: two parts and a relation keyword, CHECKED after
/// the solve rather than driving placement.
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

// ── Loops (Phase E3) ─────────────────────────────────────────────────────

/// `for VAR in START..END { … }` — everything inside repeats once per
/// integer in the half-open range. Unrolled at resolve time: nothing after
/// the resolver knows loops existed. Bounds may use the thing's parameters
/// and lets, and an enclosing loop's variable.
#[derive(Debug, Clone)]
pub struct ForBlock {
    pub var:         Ident,
    pub start:       Expr,
    pub end:         Expr,
    pub lets:        Vec<Prop>,
    pub parts:       Vec<PartDecl>,
    pub relations:   Vec<Placement>,
    pub constraints: Vec<ConstraintStmt>,
    pub loops:       Vec<ForBlock>,
    pub span:        Span,
}

// ── Pure functions (Phase E2) ───────────────────────────────────────────

/// `fn NAME(params) = expr` — a pure value function, resolved by
/// substitution at the call site before folding. No recursion (checked at
/// declaration time over the whole call graph) and no internal `let`: the
/// body is one expression. This is deliberately the smallest thing that
/// makes a formula like `taper` reusable across call sites — see
/// NOTES.md for the ribcage design this exists to support.
#[derive(Debug, Clone)]
pub struct FnDecl {
    pub name:   Ident,
    pub params: Vec<Ident>,
    pub body:   Expr,
    pub span:   Span,
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
    /// `if cond { a } else { b }` — an EXPRESSION with a value. `else` is
    /// mandatory: every expression evaluates to something.
    If { cond: Box<Expr>, then: Box<Expr>, else_: Box<Expr> },
    /// `noise(scale=0.1)`
    Call { name: String, args: Vec<NamedArg> },
    /// `[3, 5.5, 8]` — a list literal. Phase D.2: lists are values; a
    /// point is a list of three numbers, a path a list of points.
    List(Vec<Expr>),
    /// `[for i in 0..n { expr }]` — a list built by iteration. Same
    /// half-open range and braces as `for` blocks; the bounds fold to
    /// whole numbers, so this is total.
    /// `var` is boxed to keep `Expr` at its pre-D.2 size: `Placement`
    /// holds three expressions inline, and clippy's `large_enum_variant`
    /// is a real gate.
    Comprehension { var: Box<Ident>, start: Box<Expr>, end: Box<Expr>, body: Box<Expr> },
    /// `xs[i]` — 0-based indexing into a list value.
    Index { base: Box<Expr>, index: Box<Expr> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    And, Or,
    Add, Sub, Mul, Div,
    Lt, Gt, LtEq, GtEq, Eq, Neq,
}
