use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::geom::Axis;
use crate::lexer::token::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    errors: Vec<MoxiError>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, cursor: 0, errors: Vec::new() }
    }

    pub fn parse(mut self) -> (Document, Vec<MoxiError>) {
        let doc = self.parse_document();
        (doc, self.errors)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.cursor.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn span(&self) -> Span {
        self.peek().span
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.cursor.min(self.tokens.len() - 1)].clone();
        if self.cursor < self.tokens.len() - 1 {
            self.cursor += 1;
        }
        tok
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn expect_kind(&mut self, kind: &TokenKind, label: &str) -> Result<Token, MoxiError> {
        if std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(kind) {
            Ok(self.advance())
        } else {
            Err(MoxiError::UnexpectedToken {
                got: format!("{:?}", self.peek_kind()),
                expected: label.to_string(),
                span: self.span(),
            })
        }
    }

    fn expect_ident(&mut self) -> Result<Ident, MoxiError> {
        let span = self.span();
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => { self.advance(); Ok(Ident { name, span }) }
            other => Err(MoxiError::UnexpectedToken {
                got: format!("{other:?}"),
                expected: "identifier".to_string(),
                span,
            }),
        }
    }

    fn skip_to_close_brace(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.peek_kind() {
                TokenKind::Eof => break,
                TokenKind::LBrace => { depth += 1; self.advance(); }
                TokenKind::RBrace => {
                    if depth == 0 { break; }
                    depth -= 1;
                    self.advance();
                }
                _ => { self.advance(); }
            }
        }
    }

    // ── Document ──────────────────────────────────────────────────────────

    fn parse_document(&mut self) -> Document {
        let mut items = Vec::new();
        while !self.at_eof() {
            match self.parse_top_level() {
                Ok(item) => items.push(item),
                Err(e) => {
                    self.errors.push(e);
                    self.skip_to_close_brace();
                    if matches!(self.peek_kind(), TokenKind::RBrace) { self.advance(); }
                }
            }
        }
        Document { items }
    }

    fn parse_top_level(&mut self) -> Result<TopLevel, MoxiError> {
        match self.peek_kind().clone() {
            TokenKind::Atom      => Ok(TopLevel::AtomDecl(self.parse_atom()?)),
            TokenKind::Voxel     => Ok(TopLevel::VoxelDecl(self.parse_voxel()?)),
            TokenKind::Material  => Ok(TopLevel::MaterialDecl(self.parse_material()?)),
            TokenKind::Entity    => Ok(TopLevel::EntityDecl(self.parse_entity()?)),
            TokenKind::Generator => Ok(TopLevel::GeneratorDecl(self.parse_generator()?)),
            TokenKind::World     => Ok(TopLevel::WorldDecl(self.parse_world()?)),
            TokenKind::Print     => Ok(TopLevel::PrintStmt(self.parse_print()?)),
            TokenKind::Refine    => Ok(TopLevel::RefineStmt(self.parse_refine()?)),
            other => Err(MoxiError::UnexpectedToken {
                got: format!("{other:?}"),
                expected: "top-level declaration".to_string(),
                span: self.span(),
            }),
        }
    }

    // ── atom ──────────────────────────────────────────────────────────────

    fn parse_atom(&mut self) -> Result<AtomDecl, MoxiError> {
        let span = self.span();
        self.advance();
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        let props = self.parse_prop_list()?;
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        Ok(AtomDecl { name, props, span })
    }

    // ── voxel ─────────────────────────────────────────────────────────────

    fn parse_voxel(&mut self) -> Result<VoxelDecl, MoxiError> {
        let span = self.span();
        self.advance();
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        let mut legend = Vec::new();
        let mut layers = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Legend => {
                    self.advance();
                    self.expect_kind(&TokenKind::LBrace, "'{'")?;
                    legend = self.parse_legend_entries()?;
                    self.expect_kind(&TokenKind::RBrace, "'}'")?;
                }
                TokenKind::LBracket => layers.push(self.parse_voxel_layer()?),
                _ => { self.advance(); }
            }
        }
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        Ok(VoxelDecl { name, legend, layers, span })
    }

    fn parse_legend_entries(&mut self) -> Result<Vec<LegendEntry>, MoxiError> {
        let mut entries = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            let glyph_ident = self.expect_ident()?;
            let glyph = glyph_ident.name.chars().next().unwrap_or('?');
            self.expect_kind(&TokenKind::Eq, "'='")?;
            let atom = self.expect_ident()?;
            entries.push(LegendEntry { glyph, atom });
        }
        Ok(entries)
    }

    fn parse_voxel_layer(&mut self) -> Result<VoxelLayer, MoxiError> {
        self.expect_kind(&TokenKind::LBracket, "'['")?;
        self.advance(); // `Layer`
        let index = match self.peek_kind().clone() {
            TokenKind::Int(n) => { self.advance(); n }
            _ => 0,
        };
        self.expect_kind(&TokenKind::RBracket, "']'")?;
        let mut rows = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::LBracket | TokenKind::RBrace | TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::LayerRow(r) | TokenKind::Ident(r) => { rows.push(r); self.advance(); }
                _ => break,
            }
        }
        Ok(VoxelLayer { index, rows })
    }

    // ── material ──────────────────────────────────────────────────────────

    fn parse_material(&mut self) -> Result<MaterialDecl, MoxiError> {
        let span = self.span();
        self.advance();
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        let props = self.parse_prop_list()?;
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        Ok(MaterialDecl { name, props, span })
    }

    // ── entity ────────────────────────────────────────────────────────────

    fn parse_entity(&mut self) -> Result<EntityDecl, MoxiError> {
        let span = self.span();
        self.advance();
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        let mut parts       = Vec::new();
        let mut relations   = Vec::new();
        let mut constraints = Vec::new();
        let mut anchors     = Vec::new();
        let mut resolve     = None;
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Part => {
                    match self.parse_part() {
                        Ok(p) => parts.push(p),
                        Err(e) => { self.errors.push(e); self.skip_to_close_brace(); self.advance(); }
                    }
                }
                TokenKind::Relation => {
                    self.advance();
                    self.expect_kind(&TokenKind::LBrace, "'{'")?;
                    while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                        match self.parse_placement_stmt() {
                            Ok(r) => relations.push(r),
                            Err(e) => { self.errors.push(e); self.advance(); }
                        }
                    }
                    self.expect_kind(&TokenKind::RBrace, "'}'")?;
                }
                TokenKind::Constraint => {
                    match self.parse_constraint_stmt() {
                        Ok(c) => constraints.push(c),
                        Err(e) => { self.errors.push(e); self.advance(); }
                    }
                }
                // Entity-level anchor export: `anchor socket = Humerus.top`
                TokenKind::Ident(ref k) if k == "anchor" => {
                    match self.parse_anchor_decl() {
                        Ok(a) => anchors.push(a),
                        Err(e) => { self.errors.push(e); self.advance(); }
                    }
                }
                TokenKind::Resolve => { resolve = Some(self.parse_resolve_opts()?); }
                TokenKind::Parts => {
                    self.advance();
                    self.expect_kind(&TokenKind::Eq, "'='")?;
                    self.parse_expr()?;
                }
                _ => { self.advance(); }
            }
        }
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        Ok(EntityDecl { name, parts, relations, constraints, anchors, resolve, span })
    }

    /// `anchor NAME = Part.anchor(args…)` — an exported socket.
    fn parse_anchor_decl(&mut self) -> Result<AnchorDecl, MoxiError> {
        let span = self.span();
        self.advance(); // the `anchor` identifier
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::Eq, "'='")?;
        let target = self.parse_partial_anchor_ref()?;
        if target.anchor.is_none() {
            return Err(MoxiError::UnexpectedToken {
                got:      "bare part name".to_string(),
                expected: "Part.anchor (e.g. `anchor socket = Humerus.top`)".to_string(),
                span,
            });
        }
        Ok(AnchorDecl { name, target: target.into_anchor_ref("center"), span })
    }

    // ── part ──────────────────────────────────────────────────────────────

    fn parse_part(&mut self) -> Result<PartDecl, MoxiError> {
        let span = self.span();
        self.advance();
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        let mut shape     = None;
        let mut entity    = None;
        let mut material  = None;
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Shape => {
                    self.advance();
                    self.expect_kind(&TokenKind::Eq, "'='")?;
                    shape = Some(self.parse_shape_expr()?);
                }
                // Instance: `part RightArm { entity = Arm }`
                TokenKind::Entity => {
                    self.advance();
                    self.expect_kind(&TokenKind::Eq, "'='")?;
                    entity = Some(self.expect_ident()?);
                }
                TokenKind::Material => {
                    self.advance();
                    self.expect_kind(&TokenKind::Eq, "'='")?;
                    material = Some(self.expect_ident()?);
                }
                TokenKind::Comma => { self.advance(); }
                _ => { self.advance(); }
            }
        }
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        Ok(PartDecl { name, shape, entity, material, span })
    }

    // ── shapes ────────────────────────────────────────────────────────────
    //
    // Phase B2: `union`, `difference`, `intersect`, `at`, `spin` parse as
    // shape combinators HERE, context-sensitively — they are ordinary
    // identifiers everywhere else, so no lexer keywords and no collisions
    // with part or entity names.

    fn parse_shape_expr(&mut self) -> Result<ShapeExpr, MoxiError> {
        let span = self.span();
        match self.peek_kind().clone() {
            TokenKind::Box_       => { self.advance(); Ok(ShapeExpr::Box_      { args: self.parse_named_args()? }) }
            TokenKind::Sphere     => { self.advance(); Ok(ShapeExpr::Sphere    { args: self.parse_named_args()? }) }
            TokenKind::Cylinder   => { self.advance(); Ok(ShapeExpr::Cylinder  { args: self.parse_named_args()? }) }
            TokenKind::Cone       => { self.advance(); Ok(ShapeExpr::Cone      { args: self.parse_named_args()? }) }
            TokenKind::Ellipsoid  => { self.advance(); Ok(ShapeExpr::Ellipsoid { args: self.parse_named_args()? }) }
            TokenKind::Blob       => { self.advance(); Ok(ShapeExpr::Blob      { args: self.parse_named_args()? }) }
            TokenKind::Heightfield=> { self.advance(); Ok(ShapeExpr::Heightfield{ args: self.parse_named_args()? }) }
            TokenKind::Shell => {
                self.advance();
                self.expect_kind(&TokenKind::LParen, "'('")?;
                let inner = Box::new(self.parse_shape_expr()?);
                if matches!(self.peek_kind(), TokenKind::Comma) { self.advance(); }
                let args = self.parse_named_arg_list()?;
                self.expect_kind(&TokenKind::RParen, "')'")?;
                Ok(ShapeExpr::Shell { inner, args })
            }
            TokenKind::Extrude => {
                self.advance();
                self.expect_kind(&TokenKind::LParen, "'('")?;
                let profile = Box::new(self.parse_shape_expr()?);
                if matches!(self.peek_kind(), TokenKind::Comma) { self.advance(); }
                let args = self.parse_named_arg_list()?;
                self.expect_kind(&TokenKind::RParen, "')'")?;
                Ok(ShapeExpr::Extrude { profile, args })
            }

            // ── CSG combinators (Phase B2) ────────────────────────────────
            // union(a, b, …) / intersect(a, b, …): a comma-separated list
            // of shapes. Anchors follow the first operand.
            TokenKind::Ident(ref s) if s == "union" || s == "intersect" => {
                let is_union = s == "union";
                self.advance();
                self.expect_kind(&TokenKind::LParen, "'('")?;
                let mut shapes = vec![self.parse_shape_expr()?];
                while matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    shapes.push(self.parse_shape_expr()?);
                }
                self.expect_kind(&TokenKind::RParen, "')'")?;
                Ok(if is_union {
                    ShapeExpr::Union { shapes }
                } else {
                    ShapeExpr::Intersect { shapes }
                })
            }
            // difference(base, cut, …): the base minus every cut.
            TokenKind::Ident(ref s) if s == "difference" => {
                self.advance();
                self.expect_kind(&TokenKind::LParen, "'('")?;
                let base = Box::new(self.parse_shape_expr()?);
                let mut cuts = Vec::new();
                while matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    cuts.push(self.parse_shape_expr()?);
                }
                if cuts.is_empty() {
                    return Err(MoxiError::UnexpectedToken {
                        got:      "')'".to_string(),
                        expected: "difference(base, cut, …) needs at least one cut".to_string(),
                        span,
                    });
                }
                self.expect_kind(&TokenKind::RParen, "')'")?;
                Ok(ShapeExpr::Difference { base, cuts })
            }
            // at(shape, x=…, y=…, z=…) / spin(shape, axis=…, degrees=…):
            // local transform wrappers around one child shape.
            TokenKind::Ident(ref s) if s == "at" || s == "spin" => {
                let is_at = s == "at";
                self.advance();
                self.expect_kind(&TokenKind::LParen, "'('")?;
                let inner = Box::new(self.parse_shape_expr()?);
                if matches!(self.peek_kind(), TokenKind::Comma) { self.advance(); }
                let args = self.parse_named_arg_list()?;
                self.expect_kind(&TokenKind::RParen, "')'")?;
                Ok(if is_at {
                    ShapeExpr::At { inner, args }
                } else {
                    ShapeExpr::Spin { inner, args }
                })
            }

            other => Err(MoxiError::UnexpectedToken {
                got: format!("{other:?}"),
                expected: "shape primitive".to_string(),
                span,
            }),
        }
    }

    fn parse_named_args(&mut self) -> Result<Vec<NamedArg>, MoxiError> {
        self.expect_kind(&TokenKind::LParen, "'('")?;
        let args = self.parse_named_arg_list()?;
        self.expect_kind(&TokenKind::RParen, "')'")?;
        Ok(args)
    }

    fn parse_named_arg_list(&mut self) -> Result<Vec<NamedArg>, MoxiError> {
        let mut args = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::RBrace | TokenKind::Eof) {
            let key = match self.peek_kind().clone() {
                TokenKind::Ident(s) => { self.advance(); s }
                _ => break,
            };
            self.expect_kind(&TokenKind::Eq, "'='")?;
            let value = self.parse_expr()?;
            args.push(NamedArg { key, value });
            if matches!(self.peek_kind(), TokenKind::Comma) { self.advance(); }
        }
        Ok(args)
    }

    // ── placements ────────────────────────────────────────────────────────
    //
    // placement  := anchor_ref REL_KEYWORD anchor_ref qualifier*    (sugar)
    //             | anchor_ref "on" anchor_ref qualifier*           (explicit)
    // anchor_ref := IDENT ( "." IDENT ( "(" named_args ")" )? )?
    // qualifier  := ("twist"|"pitch"|"gap") "=" NUMBER
    //             | "from" "=" IDENT           (symmetric_across only)
    //             | "axis" "=" ("x"|"y"|"z")   (symmetric_across only)
    //
    // Keyword sugar desugars HERE, at parse time — the AST only ever
    // contains Align and Mirror.

    fn parse_placement_stmt(&mut self) -> Result<Placement, MoxiError> {
        let span = self.span();
        let subject = self.parse_partial_anchor_ref()?;

        if matches!(self.peek_kind(), TokenKind::On) {
            self.advance();
            let object = self.parse_partial_anchor_ref()?;
            let q = self.parse_qualifiers()?;

            let (Some(_), Some(_)) = (&subject.anchor, &object.anchor) else {
                return Err(MoxiError::UnexpectedToken {
                    got:      "bare part name".to_string(),
                    expected: "explicit anchors with 'on' (e.g. A.bottom on B.top); use a relation keyword for defaults".to_string(),
                    span,
                });
            };
            if q.from.is_some() || q.axis.is_some() {
                return Err(MoxiError::UnexpectedToken {
                    got:      "'from'/'axis' qualifier".to_string(),
                    expected: "'from' and 'axis' only on symmetric_across".to_string(),
                    span,
                });
            }
            Ok(Placement::Align {
                subject: subject.into_anchor_ref("center"), // anchors verified present above
                object:  object.into_anchor_ref("center"),
                twist: q.twist, pitch: q.pitch, gap: q.gap,
                span,
            })
        } else {
            let predicate = self.parse_relation_kind()?;
            let object = self.parse_partial_anchor_ref()?;
            let q = self.parse_qualifiers()?;
            self.desugar_placement(subject, predicate, object, q, span)
        }
    }

    fn parse_partial_anchor_ref(&mut self) -> Result<PartialAnchorRef, MoxiError> {
        let part = self.expect_ident()?;
        if !matches!(self.peek_kind(), TokenKind::Dot) {
            return Ok(PartialAnchorRef { part, anchor: None });
        }
        self.advance(); // '.'
        let anchor = self.expect_ident()?;
        let args = if matches!(self.peek_kind(), TokenKind::LParen) {
            self.parse_named_args()?
        } else {
            Vec::new()
        };
        Ok(PartialAnchorRef { part, anchor: Some((anchor.name, args)) })
    }

    fn parse_qualifiers(&mut self) -> Result<Qualifiers, MoxiError> {
        let mut q = Qualifiers::default();
        loop {
            let key = match self.peek_kind().clone() {
                TokenKind::Ident(k) if self.next_is_eq()
                    && matches!(k.as_str(), "twist" | "pitch" | "gap" | "from" | "axis") => k,
                _ => break,
            };
            self.advance(); // key
            self.advance(); // '='
            match key.as_str() {
                "twist" => q.twist = self.expect_number()?,
                "pitch" => q.pitch = self.expect_number()?,
                "gap"   => q.gap   = self.expect_number()?,
                "from"  => q.from  = Some(self.expect_ident()?),
                "axis"  => {
                    let id = self.expect_ident()?;
                    q.axis = Some(Axis::parse(&id.name).ok_or(MoxiError::UnexpectedToken {
                        got:      id.name,
                        expected: "axis x, y, or z".to_string(),
                        span:     id.span,
                    })?);
                }
                _ => unreachable!(),
            }
        }
        Ok(q)
    }

    fn next_is_eq(&self) -> bool {
        self.cursor + 1 < self.tokens.len()
            && matches!(self.tokens[self.cursor + 1].kind, TokenKind::Eq)
    }

    fn expect_number(&mut self) -> Result<f64, MoxiError> {
        match self.peek_kind().clone() {
            TokenKind::Float(v) => { self.advance(); Ok(v) }
            TokenKind::Int(n)   => { self.advance(); Ok(n as f64) }
            other => Err(MoxiError::UnexpectedToken {
                got:      format!("{other:?}"),
                expected: "numeric literal".to_string(),
                span:     self.span(),
            }),
        }
    }

    fn desugar_placement(
        &mut self,
        subject:   PartialAnchorRef,
        predicate: RelationKind,
        object:    PartialAnchorRef,
        q:         Qualifiers,
        span:      Span,
    ) -> Result<Placement, MoxiError> {
        if predicate == RelationKind::SymmetricAcross {
            let source = q.from.ok_or_else(|| MoxiError::UnexpectedToken {
                got:      "missing 'from='".to_string(),
                expected: "symmetric_across requires from=<source part> (the part to mirror)".to_string(),
                span,
            })?;
            return Ok(Placement::Mirror {
                subject: subject.part.name,
                source:  source.name,
                plane:   object.into_anchor_ref("center"),
                axis:    q.axis.unwrap_or(Axis::X),
                span,
            });
        }

        if q.from.is_some() || q.axis.is_some() {
            return Err(MoxiError::UnexpectedToken {
                got:      "'from'/'axis' qualifier".to_string(),
                expected: "'from' and 'axis' only on symmetric_across".to_string(),
                span,
            });
        }

        // Sugar table: each keyword names its pair of default anchors.
        // Explicit anchors (A.foo above B.bar) override their side's default.
        let (sub_a, obj_a) = match predicate {
            RelationKind::Above       => ("bottom", "top"),
            RelationKind::Below       => ("top", "bottom"),
            RelationKind::LeftOf      => ("east", "west"),
            RelationKind::RightOf     => ("west", "east"),
            RelationKind::InFrontOf   => ("north", "south"),
            RelationKind::Behind      => ("south", "north"),
            RelationKind::Outside     => ("west", "east"),
            RelationKind::Inside
            | RelationKind::Surrounds => ("center", "center"),
            RelationKind::Touch
            | RelationKind::AdjacentTo
            | RelationKind::AttachedTo => ("bottom", "top"),
            RelationKind::SymmetricAcross => unreachable!(),
        };

        Ok(Placement::Align {
            subject: subject.into_anchor_ref(sub_a),
            object:  object.into_anchor_ref(obj_a),
            twist: q.twist, pitch: q.pitch, gap: q.gap,
            span,
        })
    }

    fn parse_relation_kind(&mut self) -> Result<RelationKind, MoxiError> {
        let span = self.span();
        let kind = match self.peek_kind().clone() {
            TokenKind::Inside          => RelationKind::Inside,
            TokenKind::Outside         => RelationKind::Outside,
            TokenKind::AdjacentTo      => RelationKind::AdjacentTo,
            TokenKind::Above           => RelationKind::Above,
            TokenKind::Below           => RelationKind::Below,
            TokenKind::LeftOf          => RelationKind::LeftOf,
            TokenKind::RightOf         => RelationKind::RightOf,
            TokenKind::InFrontOf       => RelationKind::InFrontOf,
            TokenKind::Behind          => RelationKind::Behind,
            TokenKind::SymmetricAcross => RelationKind::SymmetricAcross,
            TokenKind::AttachedTo      => RelationKind::AttachedTo,
            TokenKind::Touch           => RelationKind::Touch,
            TokenKind::Surrounds       => RelationKind::Surrounds,
            other => return Err(MoxiError::UnexpectedToken {
                got: format!("{other:?}"),
                expected: "relation keyword".to_string(),
                span,
            }),
        };
        self.advance();
        Ok(kind)
    }

    // ── constraints ───────────────────────────────────────────────────────

    fn parse_constraint_stmt(&mut self) -> Result<ConstraintStmt, MoxiError> {
        let span = self.span();
        self.advance(); // consume `constraint`
        let subject = self.expect_ident()?;
        let expr = if self.is_relation_token() {
            let predicate = self.parse_relation_kind()?;
            let object = self.expect_ident()?;
            let mut qualifiers = Vec::new();
            while let TokenKind::Ident(q) = self.peek_kind().clone() {
                qualifiers.push(Ident { name: q, span: self.span() });
                self.advance();
            }
            ConstraintExpr::Relation(RelationStmt { subject, predicate, object, qualifiers, span })
        } else {
            let op = self.parse_cmp_op()?;
            let value = self.parse_expr()?;
            ConstraintExpr::Bound { name: subject, op, value }
        };
        Ok(ConstraintStmt { expr, span })
    }

    fn is_relation_token(&self) -> bool {
        matches!(self.peek_kind(),
            TokenKind::Inside | TokenKind::Outside | TokenKind::AdjacentTo |
            TokenKind::Above  | TokenKind::Below   | TokenKind::LeftOf     |
            TokenKind::RightOf| TokenKind::InFrontOf | TokenKind::Behind   |
            TokenKind::SymmetricAcross | TokenKind::AttachedTo             |
            TokenKind::Touch  | TokenKind::Surrounds)
    }

    fn parse_cmp_op(&mut self) -> Result<CmpOp, MoxiError> {
        let span = self.span();
        let op = match self.peek_kind() {
            TokenKind::Lt   => CmpOp::Lt,
            TokenKind::Gt   => CmpOp::Gt,
            TokenKind::LtEq => CmpOp::LtEq,
            TokenKind::GtEq => CmpOp::GtEq,
            TokenKind::EqEq => CmpOp::Eq,
            TokenKind::Neq  => CmpOp::Neq,
            other => return Err(MoxiError::UnexpectedToken {
                got: format!("{other:?}"),
                expected: "comparison operator".to_string(),
                span,
            }),
        };
        self.advance();
        Ok(op)
    }

    // ── generator ─────────────────────────────────────────────────────────

    fn parse_generator(&mut self) -> Result<GeneratorDecl, MoxiError> {
        let span = self.span();
        self.advance();
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        self.expect_kind(&TokenKind::Scatter, "'scatter'")?;
        let scatter_target = self.expect_ident()?;
        let mut props = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            let prop_span = self.span();
            let key = match self.peek_kind().clone() {
                TokenKind::Ident(k) => { self.advance(); k }
                TokenKind::Over     => { self.advance(); "over".to_string() }
                TokenKind::Where    => { self.advance(); "where".to_string() }
                TokenKind::Avoid    => { self.advance(); "avoid".to_string() }
                _ => { self.advance(); continue; }
            };
            self.expect_kind(&TokenKind::Eq, "'='")?;
            let value = self.parse_expr()?;
            props.push(Prop { key, value, span: prop_span });
        }
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        Ok(GeneratorDecl { name, scatter_target, props, span })
    }

    // ── world ─────────────────────────────────────────────────────────────

    fn parse_world(&mut self) -> Result<WorldDecl, MoxiError> {
        let span = self.span();
        self.advance();
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        let mut scale = None; let mut sea_level = None;
        let mut terrain = None; let mut biomes = Vec::new();
        let mut water = None; let mut resolve = None;
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Ident(ref k) if k == "scale" => {
                    self.advance(); self.expect_kind(&TokenKind::Eq, "'='")?;
                    scale = Some(self.expect_ident()?);
                }
                TokenKind::Ident(ref k) if k == "sea_level" => {
                    self.advance(); self.expect_kind(&TokenKind::Eq, "'='")?;
                    sea_level = Some(self.parse_expr()?);
                }
                TokenKind::Terrain  => { terrain = Some(self.parse_terrain_block()?); }
                TokenKind::Biome    => { biomes.push(self.parse_biome_block()?); }
                TokenKind::Water    => { water = Some(self.parse_water_block()?); }
                TokenKind::Resolve  => { resolve = Some(self.parse_resolve_opts()?); }
                _ => { self.advance(); }
            }
        }
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        Ok(WorldDecl { name, scale, sea_level, terrain, biomes, water, resolve, span })
    }

    fn parse_terrain_block(&mut self) -> Result<TerrainBlock, MoxiError> {
        self.advance();
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        let mut base = None; let mut max_elevation = None; let mut edge_falloff = None;
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Ident(ref k) if k == "base" => {
                    self.advance(); self.expect_kind(&TokenKind::Eq, "'='")?;
                    base = Some(self.parse_shape_expr()?);
                }
                TokenKind::Ident(ref k) if k == "max_elevation" => {
                    self.advance(); self.expect_kind(&TokenKind::Eq, "'='")?;
                    max_elevation = Some(self.parse_expr()?);
                }
                TokenKind::Ident(ref k) if k == "edge_falloff" => {
                    self.advance(); self.expect_kind(&TokenKind::Eq, "'='")?;
                    edge_falloff = Some(self.expect_ident()?);
                }
                _ => { self.advance(); }
            }
        }
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        let base = base.ok_or_else(|| MoxiError::UnexpectedToken {
            got: "missing".to_string(), expected: "terrain base".to_string(), span: self.span(),
        })?;
        Ok(TerrainBlock { base, max_elevation, edge_falloff })
    }

    fn parse_biome_block(&mut self) -> Result<BiomeBlock, MoxiError> {
        self.advance();
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        let mut condition = Expr::Int(1); let mut surface_material = None; let mut generator = None;
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Where => {
                    self.advance(); self.expect_kind(&TokenKind::Eq, "'='")?;
                    condition = self.parse_expr()?;
                }
                TokenKind::Ident(ref k) if k == "surface_material" => {
                    self.advance(); self.expect_kind(&TokenKind::Eq, "'='")?;
                    surface_material = Some(self.expect_ident()?);
                }
                TokenKind::Generator => {
                    self.advance(); self.expect_kind(&TokenKind::Eq, "'='")?;
                    generator = Some(self.expect_ident()?);
                }
                _ => { self.advance(); }
            }
        }
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        Ok(BiomeBlock { name, condition, surface_material, generator })
    }

    fn parse_water_block(&mut self) -> Result<WaterBlock, MoxiError> {
        self.advance();
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        let mut level = Expr::Int(0); let mut material = None; let mut depth_material = None;
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Ident(ref k) if k == "level" => {
                    self.advance(); self.expect_kind(&TokenKind::Eq, "'='")?;
                    level = self.parse_expr()?;
                }
                TokenKind::Material => {
                    self.advance(); self.expect_kind(&TokenKind::Eq, "'='")?;
                    material = Some(self.expect_ident()?);
                }
                TokenKind::Ident(ref k) if k == "depth_material" => {
                    self.advance(); self.expect_kind(&TokenKind::Eq, "'='")?;
                    depth_material = Some(self.expect_ident()?);
                }
                _ => { self.advance(); }
            }
        }
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        Ok(WaterBlock { level, material, depth_material })
    }

    fn parse_resolve_opts(&mut self) -> Result<ResolveOpts, MoxiError> {
        self.advance(); // `resolve`
        // consume `voxel_size` identifier
        match self.peek_kind().clone() {
            TokenKind::Ident(ref k) if k == "voxel_size" => { self.advance(); }
            _ => {}
        }
        self.expect_kind(&TokenKind::Eq, "'='")?;
        let voxel_size = match self.peek_kind().clone() {
            TokenKind::Float(f) => { self.advance(); f }
            TokenKind::Int(n)   => { self.advance(); n as f64 }
            other => return Err(MoxiError::UnexpectedToken {
                got: format!("{other:?}"), expected: "voxel size".to_string(), span: self.span(),
            }),
        };
        Ok(ResolveOpts { voxel_size })
    }

    // ── print / refine ────────────────────────────────────────────────────

    fn parse_print(&mut self) -> Result<PrintStmt, MoxiError> {
        let span = self.span();
        self.advance();
        let target = self.expect_ident()?;
        let detail = if matches!(self.peek_kind(), TokenKind::Detail) {
            self.advance();
            self.expect_kind(&TokenKind::Eq, "'='")?;
            Some(self.parse_detail_level()?)
        } else { None };
        Ok(PrintStmt { target, detail, span })
    }

    fn parse_refine(&mut self) -> Result<RefineStmt, MoxiError> {
        let span = self.span();
        self.advance();
        let mut path = vec![self.expect_ident()?];
        while matches!(self.peek_kind(), TokenKind::Dot) {
            self.advance();
            path.push(self.expect_ident()?);
        }
        self.expect_kind(&TokenKind::Detail, "'detail'")?;
        self.expect_kind(&TokenKind::Eq, "'='")?;
        let detail = self.parse_detail_level()?;
        Ok(RefineStmt { path, detail, span })
    }

    fn parse_detail_level(&mut self) -> Result<DetailLevel, MoxiError> {
        let span = self.span();
        let level = match self.peek_kind().clone() {
            TokenKind::Ident(ref s) => match s.as_str() {
                "sketch" => DetailLevel::Sketch,
                "low"    => DetailLevel::Low,
                "medium" => DetailLevel::Medium,
                "high"   => DetailLevel::High,
                other    => return Err(MoxiError::UnexpectedToken {
                    got: other.to_string(), expected: "sketch/low/medium/high".to_string(), span,
                }),
            },
            other => return Err(MoxiError::UnexpectedToken {
                got: format!("{other:?}"), expected: "detail level".to_string(), span,
            }),
        };
        self.advance();
        Ok(level)
    }

    // ── prop list ─────────────────────────────────────────────────────────

    fn parse_prop_list(&mut self) -> Result<Vec<Prop>, MoxiError> {
        let mut props = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            let span = self.span();
            let key = match self.peek_kind().clone() {
                TokenKind::Ident(s) => { self.advance(); s }
                _ => break,
            };
            self.expect_kind(&TokenKind::Eq, "'='")?;
            let value = self.parse_expr()?;
            props.push(Prop { key, value, span });
            if matches!(self.peek_kind(), TokenKind::Comma) { self.advance(); }
        }
        Ok(props)
    }

    // ── expressions ───────────────────────────────────────────────────────

    fn parse_expr(&mut self) -> Result<Expr, MoxiError> { self.parse_expr_or() }

    fn parse_expr_or(&mut self) -> Result<Expr, MoxiError> {
        let mut lhs = self.parse_expr_and()?;
        while matches!(self.peek_kind(), TokenKind::Or) {
            self.advance();
            let rhs = self.parse_expr_and()?;
            lhs = Expr::BinOp { op: BinOp::Or, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_expr_and(&mut self) -> Result<Expr, MoxiError> {
        let mut lhs = self.parse_expr_cmp()?;
        while matches!(self.peek_kind(), TokenKind::And) {
            self.advance();
            let rhs = self.parse_expr_cmp()?;
            lhs = Expr::BinOp { op: BinOp::And, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_expr_cmp(&mut self) -> Result<Expr, MoxiError> {
        let lhs = self.parse_expr_add()?;
        let op = match self.peek_kind() {
            TokenKind::Lt   => BinOp::Lt,
            TokenKind::Gt   => BinOp::Gt,
            TokenKind::LtEq => BinOp::LtEq,
            TokenKind::GtEq => BinOp::GtEq,
            TokenKind::EqEq => BinOp::Eq,
            TokenKind::Neq  => BinOp::Neq,
            _ => return Ok(lhs),
        };
        self.advance();
        let rhs = self.parse_expr_add()?;
        Ok(Expr::BinOp { op, lhs: Box::new(lhs), rhs: Box::new(rhs) })
    }

    fn parse_expr_add(&mut self) -> Result<Expr, MoxiError> {
        let mut lhs = self.parse_expr_unary()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus  => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_expr_unary()?;
            lhs = Expr::BinOp { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_expr_unary(&mut self) -> Result<Expr, MoxiError> {
        if matches!(self.peek_kind(), TokenKind::Not) {
            self.advance();
            return Ok(Expr::Not(Box::new(self.parse_expr_atom()?)));
        }
        self.parse_expr_atom()
    }

    fn parse_expr_atom(&mut self) -> Result<Expr, MoxiError> {
        let span = self.span();
        match self.peek_kind().clone() {
            TokenKind::Int(n)       => { self.advance(); Ok(Expr::Int(n)) }
            TokenKind::Float(f)     => { self.advance(); Ok(Expr::Float(f)) }
            TokenKind::StringLit(s) => { self.advance(); Ok(Expr::Str(s)) }
            TokenKind::LBracket => {
                self.advance();
                let mut items = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::RBracket | TokenKind::Eof) {
                    items.push(self.parse_expr()?);
                    if matches!(self.peek_kind(), TokenKind::Comma) { self.advance(); }
                }
                self.expect_kind(&TokenKind::RBracket, "']'")?;
                Ok(Expr::List(items))
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect_kind(&TokenKind::RParen, "')'")?;
                Ok(inner)
            }
            TokenKind::Ident(name) => {
                self.advance();
                if matches!(self.peek_kind(), TokenKind::LParen) {
                    let args = self.parse_named_args()?;
                    Ok(Expr::Call { name, args })
                } else {
                    Ok(Expr::Ident(Ident { name, span }))
                }
            }
            other => Err(MoxiError::UnexpectedToken {
                got: format!("{other:?}"),
                expected: "expression".to_string(),
                span,
            }),
        }
    }
}

// ── Parse-internal placement helpers ──────────────────────────────────────

/// An anchor reference mid-parse: the anchor may be absent (sugar forms
/// fill defaults in desugar_placement).
struct PartialAnchorRef {
    part:   Ident,
    anchor: Option<(String, Vec<NamedArg>)>,
}

impl PartialAnchorRef {
    fn into_anchor_ref(self, default_anchor: &str) -> AnchorRef {
        let span = self.part.span;
        match self.anchor {
            Some((name, args)) => AnchorRef { part: self.part.name, anchor: name, args, span },
            None => AnchorRef {
                part:   self.part.name,
                anchor: default_anchor.to_string(),
                args:   Vec::new(),
                span,
            },
        }
    }
}

#[derive(Default)]
struct Qualifiers {
    twist: f64,
    pitch: f64,
    gap:   f64,
    from:  Option<Ident>,
    axis:  Option<Axis>,
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse_src(src: &str) -> (Document, Vec<MoxiError>) {
        let (tokens, lex_errors) = Lexer::new(src).tokenize();
        assert!(lex_errors.is_empty(), "lex errors: {lex_errors:?}");
        Parser::new(tokens).parse()
    }

    /// Phase B2 syntax smoke test: nested CSG with wrappers parses into
    /// the expected tree, and `difference` demands at least one cut.
    #[test]
    fn csg_shapes_parse() {
        let src = r#"
entity Widget {
    part Body {
        shape = difference(
            cylinder(height=10, radius=5),
            at(cylinder(height=10, radius=4), y=1),
            at(spin(cylinder(height=12, radius=1), axis=z, degrees=90), y=5)
        )
    }
    part Pair {
        shape = union(sphere(radius=2), at(sphere(radius=2), x=6))
    }
}
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(e) = &doc.items[0] else { panic!("expected entity") };

        let Some(ShapeExpr::Difference { base, cuts }) = &e.parts[0].shape else {
            panic!("expected difference");
        };
        assert!(matches!(**base, ShapeExpr::Cylinder { .. }));
        assert_eq!(cuts.len(), 2);
        assert!(matches!(cuts[0], ShapeExpr::At { .. }));
        let ShapeExpr::At { inner, .. } = &cuts[1] else { panic!("expected at") };
        assert!(matches!(**inner, ShapeExpr::Spin { .. }));

        let Some(ShapeExpr::Union { shapes }) = &e.parts[1].shape else {
            panic!("expected union");
        };
        assert_eq!(shapes.len(), 2);
    }

    #[test]
    fn difference_requires_a_cut() {
        let src = r#"
entity Bad {
    part P { shape = difference(sphere(radius=3)) }
}
"#;
        let (_, errors) = parse_src(src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::UnexpectedToken { expected, .. } if expected.contains("at least one cut"))),
            "expected a needs-one-cut error, got: {errors:?}");
    }
}
