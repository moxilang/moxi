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
        }
    }
}

impl std::error::Error for MoxiError {}

/// Convenience alias used throughout the codebase.
pub type MoxiResult<T> = Result<T, Vec<MoxiError>>;