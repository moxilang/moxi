/// Source location — line and column, 1-indexed — and the file: 0 is the
/// script being compiled, n > 0 the n-th file it imports (Issue J; the
/// names live in `pipeline`'s file table for the compile in progress).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    /// u32 with `file` keeps a span at 16 bytes (the AST carries thousands)
    pub col: u32,
    pub file: u32,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col: col as u32, file: 0 }
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// Every error that can occur anywhere in the Moxi pipeline.
#[derive(Debug, Clone, PartialEq)]
pub enum MoxiError {
    // ── Lexer ──────────────────────────────────────────────────────────────
    /// A character that has no place in Moxi source.
    UnexpectedChar { ch: char, span: Span },
    /// A string literal was opened but never closed.
    UnterminatedString { span: Span },

    // ── Parser ─────────────────────────────────────────────────────────────
    /// Got a token we didn't expect at this position.
    UnexpectedToken { got: String, expected: String, span: Span },
    /// Ran out of tokens before the construct was complete.
    UnexpectedEof { expected: String },

    // ── Semantic resolver ──────────────────────────────────────────────────
    /// A name was used but never declared.
    UndefinedName { name: String, span: Span },
    /// The same name was declared twice in the same scope.
    DuplicateName { name: String, span: Span },
    /// A part references a material that doesn't exist.
    UndefinedMaterial { name: String, span: Span },
    /// An atom referenced in a material isn't declared.
    UndefinedAtom { name: String, span: Span },

    // ── Constraint validator ───────────────────────────────────────────────
    /// A declared constraint was violated after geometry resolution.
    ConstraintViolation { description: String },

    // ── Placement (anchors & mates) ────────────────────────────────────────
    /// An anchor name that doesn't exist on the part's shape.
    UndefinedAnchor { part: String, anchor: String, valid: String, span: Span },
    /// An anchor exists but its arguments are invalid.
    BadAnchor { part: String, anchor: String, message: String, span: Span },
    /// A thing has more than one part with no placement. Exactly one part
    /// (the root, the first-declared unplaced one) may go unplaced; every
    /// other part must be a placement's subject. `parts` lists every
    /// unplaced part except the root, in declaration order.
    UnplacedParts { thing: String, parts: Vec<String>, root: String, span: Span },

    // ── Entity instancing (composition) ────────────────────────────────────
    /// Anything wrong with a `part X { entity = Y }` instance: unknown or
    /// not-yet-declared template, shape+entity on one part, a socket that
    /// isn't on the instance's root part, or a mirror between mismatched
    /// instance types.
    InstanceError { instance: String, message: String, span: Span },

    // ── Values (Phase D) ───────────────────────────────────────────────────
    /// An expression could not be evaluated: an undefined name (the
    /// message lists what is in scope), a type mismatch, division by
    /// zero, or a construct that is not a value yet.
    ExprError { message: String, span: Span },

    // ── Functions (Phase E2) ────────────────────────────────────────────────
    /// A `fn` call that could not be resolved: wrong arity, an unknown
    /// function name, or a call graph that isn't acyclic. The message
    /// names the function and, for arity, the parameter list.
    FnError { message: String, span: Span },

    // ── Poses (living models) ───────────────────────────────────────────────
    /// A `pose` that cannot be applied: it names an unknown part, the root
    /// (nothing to move), a mirror image (pose its source), a part placed
    /// inside an instance (use the instance's own poses), or an unknown
    /// pose. The message names the pose and lists what it could name.
    PoseError { pose: String, message: String, span: Span },

    // ── Animations (living models) ──────────────────────────────────────────
    /// An `animate` that cannot play as written: keyframe times not
    /// starting at 0 or not increasing, a pose that does not exist (the
    /// message lists the thing's poses), fewer than two keyframes, or a
    /// joint turning more than 179° between two keyframes (which an
    /// exporter would play the short way round).
    AnimationError { animation: String, message: String, span: Span },

    // ── Materials and states (living models) ────────────────────────────────
    /// A material property the language does not have (the message lists
    /// the ones it does), or a value of the wrong kind (`emissive` must be a
    /// number ≥ 0).
    MaterialError { material: String, message: String, span: Span },
    /// A `state` that cannot apply: an unknown material or part (listed), an
    /// attribute a state cannot set, a value of the wrong kind, or the same
    /// thing set twice.
    StateError { state: String, message: String, span: Span },
    /// A `joint` range that is malformed (bounds reversed, not numbers),
    /// names a part a joint cannot belong to (listed), or is violated — at
    /// rest or by a pose, with the value and the range.
    JointError { part: String, message: String, span: Span },
    /// A `revolve` that cannot turn: a part that is not a mate's subject
    /// (listed), a qualifier other than twist/pitch, a zero or non-number
    /// rate, a joint range in the way, or rates whose common period is too
    /// long for a looping clip.
    RevolveError { revolve: String, message: String, span: Span },
    /// An `aim=` or `facing=` target that names nothing: neither one of the
    /// six direction words nor a part of the thing (both lists in the
    /// message). `qualifier` is `aim` or `facing`.
    AimError { part: String, qualifier: String, message: String, span: Span },
    /// A `breakable` that cannot be: a part no mate places (the root), a
    /// mirror image, a part inside an instance, an unknown part (each with
    /// the list of what can break), a missing or doubled trigger, `hits` not
    /// a whole number ≥ 1, or an unknown `push` word (listed).
    BreakError { part: String, message: String, span: Span },
    /// A `from "file" import …` that cannot be: a file that cannot be read,
    /// a name the file does not have (its things and materials listed), a
    /// clash with a local name (with the `as` fix), a name imported twice,
    /// a cycle (the chain named), or a generator or legacy declaration.
    ImportError { message: String, span: Span },
}

impl std::fmt::Display for MoxiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MoxiError::UnexpectedChar { ch, span } =>
                write!(f, "[{span}] unexpected character '{ch}'"),
            MoxiError::UnterminatedString { span } =>
                write!(f, "[{span}] unterminated string literal"),
            MoxiError::UnexpectedToken { got, expected, span } =>
                write!(f, "[{span}] expected {expected}, got '{got}'"),
            MoxiError::UnexpectedEof { expected } =>
                write!(f, "unexpected end of file, expected {expected}"),
            MoxiError::UndefinedName { name, span } =>
                write!(f, "[{span}] '{name}' is not defined"),
            MoxiError::DuplicateName { name, span } =>
                write!(f, "[{span}] '{name}' is already defined in this scope"),
            MoxiError::UndefinedMaterial { name, span } =>
                write!(f, "[{span}] material '{name}' is not defined"),
            MoxiError::UndefinedAtom { name, span } =>
                write!(f, "[{span}] atom '{name}' is not defined"),
            MoxiError::ConstraintViolation { description } =>
                write!(f, "constraint violated: {description}"),
            MoxiError::UndefinedAnchor { part, anchor, valid, span } =>
                write!(f, "[{span}] part '{part}' has no anchor '{anchor}' — valid anchors: {valid}"),
            MoxiError::BadAnchor { part, anchor, message, span } =>
                write!(f, "[{span}] anchor '{anchor}' on part '{part}': {message}"),
            MoxiError::UnplacedParts { thing, parts, root, span } => {
                let names = parts.iter().map(|p| format!("'{p}'")).collect::<Vec<_>>().join(", ");
                let verb = if parts.len() == 1 { "has" } else { "have" };
                let each = if parts.len() == 1 { "it" } else { "each one" };
                write!(f, "[{span}] thing '{thing}': {names} {verb} no placement; its root is \
                           '{root}' — only the root may go unplaced: place {each} on another part \
                           (`Part.anchor on Other.anchor`) or mirror it with `symmetric_across`")
            }
            MoxiError::InstanceError { instance, message, span } =>
                write!(f, "[{span}] instance '{instance}': {message}"),
            MoxiError::ExprError { message, span } =>
                write!(f, "[{span}] {message}"),
            MoxiError::FnError { message, span } =>
                write!(f, "[{span}] {message}"),
            MoxiError::PoseError { pose, message, span } =>
                write!(f, "[{span}] pose '{pose}': {message}"),
            MoxiError::AnimationError { animation, message, span } =>
                write!(f, "[{span}] animation '{animation}': {message}"),
            MoxiError::MaterialError { material, message, span } =>
                write!(f, "[{span}] material '{material}': {message}"),
            MoxiError::StateError { state, message, span } =>
                write!(f, "[{span}] state '{state}': {message}"),
            MoxiError::JointError { part, message, span } =>
                write!(f, "[{span}] joint '{part}': {message}"),
            MoxiError::RevolveError { revolve, message, span } =>
                write!(f, "[{span}] revolve '{revolve}': {message}"),
            MoxiError::AimError { part, qualifier, message, span } =>
                write!(f, "[{span}] {qualifier} of '{part}': {message}"),
            MoxiError::BreakError { part, message, span } =>
                write!(f, "[{span}] breakable '{part}': {message}"),
            MoxiError::ImportError { message, span } =>
                write!(f, "[{span}] import: {message}"),
        }
    }
}

impl std::error::Error for MoxiError {}

/// Convenience alias used throughout the codebase.
pub type MoxiResult<T> = Result<T, Vec<MoxiError>>;
