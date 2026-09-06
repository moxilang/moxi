pub mod fence;
pub mod token;

use crate::error::{MoxiError, Span};
use token::{Token, TokenKind};

/// Converts a raw `.mi` source string into a flat list of tokens.
///
/// The lexer is a single-pass character iterator.  It tracks line and
/// column so every token carries an accurate `Span`.
pub struct Lexer<'src> {
    chars: std::iter::Peekable<std::str::CharIndices<'src>>,
    line: usize,
    col: usize,
    errors: Vec<MoxiError>,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self {
        Self {
            chars: src.char_indices().peekable(),
            line: 1,
            col: 1,
            errors: Vec::new(),
        }
    }

    /// Run the full lexer and return `(tokens, errors)`.
    /// Errors are non-fatal: the lexer keeps going so we can report
    /// multiple problems in one pass.
    pub fn tokenize(mut self) -> (Vec<Token>, Vec<MoxiError>) {
        let mut tokens = Vec::new();

        loop {
            let tok = self.next_token();
            let is_eof = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }

        (tokens, self.errors)
    }

    // ── Internal helpers ────────────────────────────────────────────────────

    fn span(&self) -> Span {
        Span::new(self.line, self.col)
    }

    fn advance(&mut self) -> Option<char> {
        let (_, ch) = self.chars.next()?;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    fn peek2(&self) -> Option<char> {
        // Look two characters ahead without consuming.
        let mut iter = self.chars.clone();
        iter.next();
        iter.next().map(|(_, c)| c)
    }

    /// Legacy Markdown comments: `#` headings and `>` blockquotes, but
    /// ONLY at the start of a line, which is what those constructs
    /// actually are.
    ///
    /// Anywhere else `>` is the greater-than operator. Treating it as a
    /// comment opener regardless of position silently truncated every
    /// line containing a comparison — `if a > b { … }` became `if a`, and
    /// a generator's `where = elevation > 3 and elevation < 13` became
    /// `where = elevation`, which the old permissive evaluator then
    /// accepted for every cell.
    ///
    /// Post-S1 this whole path only matters for files with no fences at
    /// all; inside a fence, prose has already been masked out.
    fn skip_whitespace_and_comments(&mut self) {
        // True while nothing but whitespace has been seen on this line.
        let mut line_start = self.col == 1;
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\r') => { self.advance(); }
                Some('\n') => { self.advance(); line_start = true; }
                Some('#') | Some('>') if line_start => {
                    while self.peek().is_some() && self.peek() != Some('\n') {
                        self.advance();
                    }
                }
                _ => break,
            }
        }
    }

    fn read_string(&mut self, start: Span) -> TokenKind {
        // Opening `"` already consumed.
        let mut s = String::new();
        loop {
            match self.advance() {
                Some('"') => return TokenKind::StringLit(s),
                Some('\\') => {
                    match self.advance() {
                        Some('n')  => s.push('\n'),
                        Some('t')  => s.push('\t'),
                        Some('"')  => s.push('"'),
                        Some('\\') => s.push('\\'),
                        Some(c)    => s.push(c),
                        None => {
                            self.errors.push(MoxiError::UnterminatedString { span: start });
                            return TokenKind::StringLit(s);
                        }
                    }
                }
                Some(c) => s.push(c),
                None => {
                    self.errors.push(MoxiError::UnterminatedString { span: start });
                    return TokenKind::StringLit(s);
                }
            }
        }
    }

    fn read_number(&mut self, first: char) -> TokenKind {
        let mut raw = String::from(first);
        let mut is_float = false;

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                raw.push(c);
                self.advance();
            } else if c == '.' && !is_float && self.peek2().is_some_and(|c2| c2.is_ascii_digit()) {
                is_float = true;
                raw.push(c);
                self.advance();
            } else {
                break;
            }
        }

        if is_float {
            TokenKind::Float(raw.parse().unwrap_or(0.0))
        } else {
            TokenKind::Int(raw.parse().unwrap_or(0))
        }
    }

    fn read_ident_or_keyword(&mut self, first: char) -> TokenKind {
        let mut word = String::from(first);
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                word.push(c);
                self.advance();
            } else {
                break;
            }
        }
        Self::keyword_or_ident(word)
    }

    fn keyword_or_ident(word: String) -> TokenKind {
        match word.as_str() {
            // v1 keywords
            "atom"      => TokenKind::Atom,
            "legend"    => TokenKind::Legend,
            "voxel"     => TokenKind::Voxel,
            "translate" => TokenKind::Translate,
            "merge"     => TokenKind::Merge,
            "print"     => TokenKind::Print,

            // v2 keywords
            //
            // `thing` is the canonical spelling; `entity` is accepted as a
            // synonym for one release so existing scripts keep compiling.
            // Both the declaration form (`thing Skeleton { … }`) and the
            // instance form (`part RightArm { thing = Arm }`) use it.
            "thing"      => TokenKind::Entity,
            "entity"     => TokenKind::Entity,
            "part"       => TokenKind::Part,
            "relation"   => TokenKind::Relation,
            "constraint" => TokenKind::Constraint,
            "shape"      => TokenKind::Shape,
            "material"   => TokenKind::Material,
            "generator"  => TokenKind::Generator,
            "world"      => TokenKind::World,
            "refine"     => TokenKind::Refine,
            "detail"     => TokenKind::Detail,
            "biome"      => TokenKind::Biome,
            "terrain"    => TokenKind::Terrain,
            "water"      => TokenKind::Water,
            "resolve"    => TokenKind::Resolve,
            "scatter"    => TokenKind::Scatter,
            "over"       => TokenKind::Over,
            "where"      => TokenKind::Where,
            "avoid"      => TokenKind::Avoid,
            "parts"      => TokenKind::Parts,
            "on"         => TokenKind::On,

            // Phase D: values
            "let"        => TokenKind::Let,
            "if"         => TokenKind::If,
            "else"       => TokenKind::Else,

            // Built-in shapes
            "box"        => TokenKind::Box_,
            "sphere"     => TokenKind::Sphere,
            "cylinder"   => TokenKind::Cylinder,
            "cone"       => TokenKind::Cone,
            "ellipsoid"  => TokenKind::Ellipsoid,
            "blob"       => TokenKind::Blob,
            "heightfield"=> TokenKind::Heightfield,
            "shell"      => TokenKind::Shell,
            "extrude"    => TokenKind::Extrude,
            "capsule"    => TokenKind::Capsule,
            "torus"      => TokenKind::Torus,
            
            // Built-in relations
            "inside"          => TokenKind::Inside,
            "outside"         => TokenKind::Outside,
            "adjacent_to"     => TokenKind::AdjacentTo,
            "above"           => TokenKind::Above,
            "below"           => TokenKind::Below,
            "left_of"         => TokenKind::LeftOf,
            "right_of"        => TokenKind::RightOf,
            "in_front_of"     => TokenKind::InFrontOf,
            "behind"          => TokenKind::Behind,
            "symmetric_across"=> TokenKind::SymmetricAcross,
            "attached_to"     => TokenKind::AttachedTo,
            "touch"           => TokenKind::Touch,
            "surrounds"       => TokenKind::Surrounds,

            // Boolean operators
            "and" => TokenKind::And,
            "or"  => TokenKind::Or,
            "not" => TokenKind::Not,

            _     => TokenKind::Ident(word),
        }
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();

        let span = self.span();

        let ch = match self.advance() {
            None     => return Token::new(TokenKind::Eof, span),
            Some(ch) => ch,
        };

        let kind = match ch {
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            '+' => TokenKind::Plus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,

            '-' => {
                if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    // Negative number literal
                    let first = self.advance().unwrap();
                    let inner = self.read_number(first);
                    match inner {
                        TokenKind::Int(n)   => TokenKind::Int(-n),
                        TokenKind::Float(f) => TokenKind::Float(-f),
                        other               => other,
                    }
                } else {
                    TokenKind::Minus
                }
            }

            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::EqEq
                } else {
                    TokenKind::Eq
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::Neq
                } else {
                    self.errors.push(MoxiError::UnexpectedChar { ch: '!', span });
                    self.next_token().kind
                }
            }
            '<' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::LtEq
                } else {
                    TokenKind::Lt
                }
            }
            '>' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }

            '"' => self.read_string(span),

            c if c.is_ascii_digit() => self.read_number(c),

            c if c.is_alphabetic() || c == '_' => self.read_ident_or_keyword(c),

            c => {
                self.errors.push(MoxiError::UnexpectedChar { ch: c, span });
                self.next_token().kind
            }
        };

        Token::new(kind, span)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use token::TokenKind;

    fn kinds(src: &str) -> Vec<TokenKind> {
        let (tokens, errors) = Lexer::new(src).tokenize();
        assert!(errors.is_empty(), "lex errors: {errors:?}");
        tokens.into_iter().map(|t| t.kind).collect()
    }

    /// The regression: a mid-line `>` is the operator, not a comment. It
    /// used to swallow the rest of the line, which silently truncated
    /// every comparison in the language.
    #[test]
    fn greater_than_mid_line_is_an_operator() {
        let k = kinds("where = elevation > 3 and elevation < 13");
        assert!(k.contains(&TokenKind::Gt), "'>' must lex as an operator: {k:?}");
        assert!(k.contains(&TokenKind::And), "the rest of the line must survive: {k:?}");
        assert!(k.contains(&TokenKind::Int(13)), "the tail must survive: {k:?}");
    }

    #[test]
    fn greater_than_or_equal_still_lexes() {
        assert!(kinds("a >= 2").contains(&TokenKind::GtEq));
    }

    /// Legacy blockquote and heading comments still work at line start,
    /// so no-fence files keep compiling.
    #[test]
    fn line_initial_hash_and_angle_are_still_comments() {
        let k = kinds("# a heading > with an angle\n> a note\natom BONE { color = ivory }\n");
        assert_eq!(k[0], TokenKind::Atom, "comment lines must be skipped: {k:?}");
    }

    /// Indented blockquotes are still line-initial.
    #[test]
    fn indented_comments_are_still_comments() {
        let k = kinds("    > indented note\natom BONE { color = ivory }\n");
        assert_eq!(k[0], TokenKind::Atom);
    }
}