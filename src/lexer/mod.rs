pub mod token;

use crate::error::{MoxiError, Span};
use token::{Token, TokenKind};

/// Converts a raw `.mi` source string into a flat list of tokens.
///
/// The lexer is a single-pass character iterator.  It tracks line and
/// column so every token carries an accurate `Span`.
pub struct Lexer<'src> {
    src: &'src str,
    chars: std::iter::Peekable<std::str::CharIndices<'src>>,
    line: usize,
    col: usize,
    errors: Vec<MoxiError>,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Self {
        Self {
            src,
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

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\r') => { self.advance(); }
                Some('\n') => { self.advance(); }
                // Line comments: # … and > … (Markdown blockquote)
                Some('#') | Some('>') => {
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
            } else if c == '.' && !is_float && self.peek2().map_or(false, |c2| c2.is_ascii_digit()) {
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
            "attach"     => TokenKind::Attach,

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
                if self.peek().map_or(false, |c| c.is_ascii_digit()) {
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