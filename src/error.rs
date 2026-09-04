/// Source location — line and column, 1-indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
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
            MoxiError::InstanceError { instance, message, span } =>
                write!(f, "[{span}] instance '{instance}': {message}"),
            MoxiError::ExprError { message, span } =>
                write!(f, "[{span}] {message}"),
        }
    }
}

impl std::error::Error for MoxiError {}

/// Convenience alias used throughout the codebase.
pub type MoxiResult<T> = Result<T, Vec<MoxiError>>;
