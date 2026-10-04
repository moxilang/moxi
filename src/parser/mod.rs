use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::geom::Axis;
use crate::lexer::token::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
    errors: Vec<MoxiError>,
    /// Phase E3: index expressions of the thing being parsed; see
    /// `parse_indexed_ident`. Moved into `EntityDecl.index_exprs`.
    index_exprs: Vec<Expr>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, cursor: 0, errors: Vec::new(), index_exprs: Vec::new() }
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
            TokenKind::World     => Ok(TopLevel::WorldDecl(Box::new(self.parse_world()?))),
            TokenKind::Print     => Ok(TopLevel::PrintStmt(self.parse_print()?)),
            TokenKind::Refine    => Ok(TopLevel::RefineStmt(self.parse_refine()?)),
            TokenKind::Fn        => Ok(TopLevel::FnDecl(self.parse_fn_decl()?)),
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
        // Each thing gets its own index table (Phase E3).
        self.index_exprs.clear();
        // Optional parameter list with required defaults:
        //   thing Arm(length=9, girth=0.8) { … }
        let params: Vec<Prop> = if matches!(self.peek_kind(), TokenKind::LParen) {
            self.parse_named_args()?
                .into_iter()
                .map(|a| Prop { key: a.key, value: a.value, span })
                .collect()
        } else {
            Vec::new()
        };
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        let mut parts       = Vec::new();
        let mut lets        = Vec::new();
        let mut relations   = Vec::new();
        let mut constraints = Vec::new();
        let mut anchors     = Vec::new();
        let mut loops       = Vec::new();
        let mut poses       = Vec::new();
        let mut resolve     = None;
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Part => {
                    match self.parse_part() {
                        Ok(p) => parts.push(p),
                        Err(e) => { self.errors.push(e); self.skip_to_close_brace(); self.advance(); }
                    }
                }
                TokenKind::Relation => self.parse_relation_block(&mut relations)?,
                TokenKind::Constraint => {
                    match self.parse_constraint_stmt() {
                        Ok(c) => constraints.push(c),
                        Err(e) => { self.errors.push(e); self.advance(); }
                    }
                }
                // Thing-level anchor export: `anchor socket = Humerus.top`
                TokenKind::Ident(ref k) if k == "anchor" => {
                    match self.parse_anchor_decl() {
                        Ok(a) => anchors.push(a),
                        Err(e) => { self.errors.push(e); self.advance(); }
                    }
                }
                // Living models: `pose Name { Part qual=… }`. Contextual,
                // like `anchor`, so no existing identifier breaks.
                TokenKind::Ident(ref k) if k == "pose" => {
                    match self.parse_pose_decl() {
                        Ok(p) => poses.push(p),
                        Err(e) => { self.errors.push(e); self.skip_to_close_brace(); self.advance(); }
                    }
                }
                TokenKind::Resolve => { resolve = Some(self.parse_resolve_opts()?); }
                TokenKind::Let => lets.push(self.parse_let_stmt()?),
                TokenKind::For => {
                    match self.parse_for_block() {
                        Ok(b) => loops.push(b),
                        Err(e) => { self.errors.push(e); self.skip_to_close_brace(); self.advance(); }
                    }
                }
                TokenKind::Parts => {
                    self.advance();
                    self.expect_kind(&TokenKind::Eq, "'='")?;
                    self.parse_expr()?;
                }
                _ => { self.advance(); }
            }
        }
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        let index_exprs = std::mem::take(&mut self.index_exprs);
        Ok(EntityDecl {
            name, params, lets, parts, relations, constraints, anchors, resolve,
            loops, index_exprs, poses, span,
        })
    }

    /// `pose NAME { LINE* }`, LINE = `Part qual=…+` | `Inst pose=Name` |
    /// `for v in a..b { LINE* }`.
    fn parse_pose_decl(&mut self) -> Result<PoseDecl, MoxiError> {
        let span = self.span();
        self.advance(); // `pose`
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace, "'{' opening the pose, e.g. `pose Up { Arm lean=(60, 0) }`")?;
        let (lines, loops) = self.parse_pose_body()?;
        self.expect_kind(&TokenKind::RBrace, "'}' closing the pose")?;
        Ok(PoseDecl { name, lines, loops, span })
    }

    fn parse_pose_body(&mut self) -> Result<(Vec<PoseLine>, Vec<PoseFor>), MoxiError> {
        let mut lines = Vec::new();
        let mut loops = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            if matches!(self.peek_kind(), TokenKind::For) {
                let span = self.span();
                self.advance(); // `for`
                let var = self.expect_ident()?;
                self.expect_kind(&TokenKind::In, "'in' after the loop variable, e.g. `for k in 1..segs`")?;
                let start = self.parse_expr()?;
                self.expect_kind(&TokenKind::Dot, "'..' between the range bounds, e.g. `1..segs`")?;
                self.expect_kind(&TokenKind::Dot, "'..' between the range bounds, e.g. `1..segs`")?;
                let end = self.parse_expr()?;
                self.expect_kind(&TokenKind::LBrace, "'{' opening the loop body")?;
                let (l, n) = self.parse_pose_body()?;
                self.expect_kind(&TokenKind::RBrace, "'}' closing the loop body")?;
                loops.push(PoseFor { var, start, end, lines: l, loops: n, span });
                continue;
            }
            lines.push(self.parse_pose_line()?);
        }
        Ok((lines, loops))
    }

    fn parse_pose_line(&mut self) -> Result<PoseLine, MoxiError> {
        let part = match self.peek_kind() {
            // `Inst.Part` parses so the resolver can say why it is not
            // allowed (a thing poses only its own mates), instead of a
            // bare syntax error.
            TokenKind::Ident(_) => {
                let mut id = self.parse_indexed_ident()?;
                while matches!(self.peek_kind(), TokenKind::Dot) {
                    self.advance();
                    let next = self.parse_indexed_ident()?;
                    id.name = format!("{}.{}", id.name, next.name);
                }
                id
            }
            other => return Err(MoxiError::UnexpectedToken {
                got:      format!("{other:?}"),
                expected: "a pose line — a part and the qualifiers it takes in this pose, \
                           e.g. `Arm lean=(60, 0)`, or `Wing pose=Up` for an instance".to_string(),
                span:     self.span(),
            }),
        };
        if matches!(self.peek_kind(), TokenKind::Ident(k) if k == "pose") && self.next_is_eq() {
            self.advance(); // `pose`
            self.advance(); // '='
            return Ok(PoseLine { part, set: PoseSet::Pose(self.expect_ident()?) });
        }
        let mut q = PoseQuals::default();
        loop {
            let key = match self.peek_kind().clone() {
                TokenKind::Ident(k) if self.next_is_eq() => k,
                _ => break,
            };
            let key_span = self.span();
            let dup = match key.as_str() {
                "twist" => q.twist.is_some(),
                "pitch" => q.pitch.is_some(),
                "gap"   => q.gap.is_some(),
                "shift" => q.shift.is_some(),
                "lean"  => q.lean.is_some(),
                _ => return Err(MoxiError::UnexpectedToken {
                    got:      format!("'{key}='"),
                    expected: "a qualifier a pose can set: twist, pitch, gap, shift, lean \
                               (a pose moves parts; it never changes anchors, shapes or materials)"
                        .to_string(),
                    span:     key_span,
                }),
            };
            if dup {
                return Err(MoxiError::UnexpectedToken {
                    got:      format!("a second '{key}=' for '{}'", part.name),
                    expected: "each qualifier at most once per part in a pose".to_string(),
                    span:     key_span,
                });
            }
            self.advance(); // key
            self.advance(); // '='
            match key.as_str() {
                "twist" => q.twist = Some(self.parse_expr()?),
                "pitch" => q.pitch = Some(self.parse_expr()?),
                "gap"   => q.gap   = Some(self.parse_expr()?),
                "shift" => q.shift = Some(self.expect_pair("shift", "shift=(-2.5, 1.0)")?),
                "lean"  => q.lean  = Some(self.expect_pair("lean", "lean=(45, 0)")?),
                _ => unreachable!(),
            }
        }
        if q.is_empty() {
            return Err(MoxiError::UnexpectedToken {
                got:      format!("'{}' with nothing to set", part.name),
                expected: "at least one of twist=, pitch=, gap=, shift=, lean= (or pose= for an instance)"
                    .to_string(),
                span:     part.span,
            });
        }
        Ok(PoseLine { part, set: PoseSet::Quals(Box::new(q)) })
    }

    /// `relation { … }` — shared by thing bodies and loop bodies.
    fn parse_relation_block(&mut self, into: &mut Vec<Placement>) -> Result<(), MoxiError> {
        self.advance(); // `relation`
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            match self.parse_placement_stmt() {
                Ok(r) => into.push(r),
                Err(e) => { self.errors.push(e); self.advance(); }
            }
        }
        self.expect_kind(&TokenKind::RBrace, "'}'")?;
        Ok(())
    }

    /// `let NAME = expr` — shared by thing bodies and loop bodies.
    fn parse_let_stmt(&mut self) -> Result<Prop, MoxiError> {
        let let_span = self.span();
        self.advance(); // `let`
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::Eq, "'=' after the `let` name")?;
        let value = self.parse_expr()?;
        Ok(Prop { key: name.name, value, span: let_span })
    }

    /// `for VAR in START..END { part … relation { … } constraint … let … for … }`
    fn parse_for_block(&mut self) -> Result<ForBlock, MoxiError> {
        let span = self.span();
        self.advance(); // `for`
        let var = self.expect_ident()?;
        self.expect_kind(&TokenKind::In, "'in' after the loop variable, e.g. `for i in 0..12`")?;
        let start = self.parse_expr()?;
        self.expect_kind(&TokenKind::Dot, "'..' between the range bounds, e.g. `0..12`")?;
        self.expect_kind(&TokenKind::Dot, "'..' between the range bounds, e.g. `0..12`")?;
        let end = self.parse_expr()?;
        self.expect_kind(&TokenKind::LBrace, "'{' opening the loop body")?;

        let mut block = ForBlock {
            var, start, end,
            lets: Vec::new(), parts: Vec::new(), relations: Vec::new(),
            constraints: Vec::new(), loops: Vec::new(), span,
        };
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            match self.peek_kind().clone() {
                TokenKind::Part => {
                    match self.parse_part() {
                        Ok(p) => block.parts.push(p),
                        Err(e) => { self.errors.push(e); self.skip_to_close_brace(); self.advance(); }
                    }
                }
                TokenKind::Relation => self.parse_relation_block(&mut block.relations)?,
                TokenKind::Constraint => {
                    match self.parse_constraint_stmt() {
                        Ok(c) => block.constraints.push(c),
                        Err(e) => { self.errors.push(e); self.advance(); }
                    }
                }
                TokenKind::Let => block.lets.push(self.parse_let_stmt()?),
                TokenKind::For => block.loops.push(self.parse_for_block()?),
                other => return Err(MoxiError::UnexpectedToken {
                    got:      format!("{other:?}"),
                    expected: "a loop body item — part, relation { … }, constraint, let, \
                               or a nested for".to_string(),
                    span:     self.span(),
                }),
            }
        }
        self.expect_kind(&TokenKind::RBrace, "'}' closing the loop body")?;
        Ok(block)
    }

    /// A part name, optionally indexed: `Rib`, `RibR[i]`, `Cell[i][j]`.
    /// Each index expression goes into the thing's table and the name
    /// carries a marker `[#k]` — a form no identifier can take — which the
    /// resolver replaces with the folded value, giving `RibR[3]`.
    fn parse_indexed_ident(&mut self) -> Result<Ident, MoxiError> {
        let mut id = self.expect_ident()?;
        while matches!(self.peek_kind(), TokenKind::LBracket) {
            self.advance();
            let e = self.parse_expr()?;
            self.expect_kind(&TokenKind::RBracket, "']' closing the index")?;
            let k = self.index_exprs.len();
            self.index_exprs.push(e);
            id.name = format!("{}[#{k}]", id.name);
        }
        Ok(id)
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
        let name = self.parse_indexed_ident()?;
        self.expect_kind(&TokenKind::LBrace, "'{'")?;
        let mut shape       = None;
        let mut entity      = None;
        let mut entity_args = Vec::new();
        let mut material    = None;
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
                    if matches!(self.peek_kind(), TokenKind::LParen) {
                        entity_args = self.parse_named_args()?;
                    }
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
        Ok(PartDecl { name, shape, entity, entity_args, material, span })
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
            TokenKind::Capsule    => { self.advance(); Ok(ShapeExpr::Capsule    { args: self.parse_named_args()? }) }
            TokenKind::Torus      => { self.advance(); Ok(ShapeExpr::Torus      { args: self.parse_named_args()? }) }
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
                let mut args   = Vec::new();
                while matches!(self.peek_kind(), TokenKind::Comma) {
                    self.advance();
                    // `name = …` after the shapes: trailing named arguments
                    // (`blend=k`). A shape never starts with `ident =`.
                    if matches!(self.peek_kind(), TokenKind::Ident(_)) && self.next_is_eq() {
                        args = self.parse_named_arg_list()?;
                        break;
                    }
                    shapes.push(self.parse_shape_expr()?);
                }
                self.expect_kind(&TokenKind::RParen, "')'")?;
                if !is_union && !args.is_empty() {
                    return Err(MoxiError::UnexpectedToken {
                        got:      format!("'{}='", args[0].key),
                        expected: "intersect takes no named arguments; `blend=` is for union".to_string(),
                        span,
                    });
                }
                Ok(if is_union {
                    ShapeExpr::Union { shapes, args }
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
            // Local transform wrappers around one child shape:
            //   at(shape, x=…, y=…, z=…)      spin(shape, axis=…, degrees=…)
            //   mirror(shape, axis=x)         scale(shape, x=…, y=…, z=…)
            // Context-sensitive, like the CSG combinators: ordinary
            // identifiers everywhere else.
            TokenKind::Ident(ref s) if matches!(s.as_str(), "at" | "spin" | "mirror" | "scale") => {
                let which = s.clone();
                self.advance();
                self.expect_kind(&TokenKind::LParen, "'('")?;
                let inner = Box::new(self.parse_shape_expr()?);
                if matches!(self.peek_kind(), TokenKind::Comma) { self.advance(); }
                let args = self.parse_named_arg_list()?;
                self.expect_kind(&TokenKind::RParen, "')'")?;
                Ok(match which.as_str() {
                    "at"     => ShapeExpr::At { inner, args },
                    "spin"   => ShapeExpr::Spin { inner, args },
                    "mirror" => ShapeExpr::Mirror { inner, args },
                    _        => ShapeExpr::Scale { inner, args },
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

    /// `key=value, ...` for shapes and instances. A bare `key` immediately
    /// followed by `=` is a named arg; anything else is parsed as an
    /// EXPRESSION and stored with an empty key — this is what lets
    /// `Expr::Call` (math builtins: `sin(90)`, `clamp(x, 0, 1)`) take
    /// positional arguments through the same list, without opening
    /// positional args up to shapes and instances, which stay named-only
    /// by convention (every existing script uses `key=value` there).
    fn parse_named_arg_list(&mut self) -> Result<Vec<NamedArg>, MoxiError> {
        let mut args = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::RBrace | TokenKind::Eof) {
            let is_named = matches!(self.peek_kind(), TokenKind::Ident(_)) && self.next_is_eq();
            if is_named {
                let key = match self.peek_kind().clone() {
                    TokenKind::Ident(s) => { self.advance(); s }
                    _ => unreachable!("is_named checked this above"),
                };
                self.expect_kind(&TokenKind::Eq, "'='")?;
                let value = self.parse_expr()?;
                args.push(NamedArg { key, value });
            } else {
                let value = self.parse_expr()?;
                args.push(NamedArg { key: String::new(), value });
            }
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
                offsets: Box::new(MateOffsets { shift: q.shift, lean: q.lean }),
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
        let part = self.parse_indexed_ident()?;
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
                    && matches!(k.as_str(),
                        "twist" | "pitch" | "gap" | "shift" | "lean" | "from" | "axis") => k,
                _ => break,
            };
            self.advance(); // key
            self.advance(); // '='
            match key.as_str() {
                // Expressions, not literals: a thing parameterizes its
                // own pose. Folded at resolve time like every other
                // argument, so nothing downstream sees a name.
                "twist" => q.twist = self.parse_expr()?,
                "pitch" => q.pitch = self.parse_expr()?,
                "gap"   => q.gap   = self.parse_expr()?,
                "shift" => q.shift = self.expect_pair("shift", "shift=(-2.5, 1.0)")?,
                "lean"  => q.lean  = self.expect_pair("lean", "lean=(45, 0)")?,
                "from"  => q.from  = Some(self.parse_indexed_ident()?),
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

    /// `(a, b)` for `shift` and `lean`. Both components are expressions,
    /// so a thing can offset or lean by a parameter: `shift=(reach, 0)`.
    fn expect_pair(&mut self, name: &str, example: &str) -> Result<(Expr, Expr), MoxiError> {
        self.expect_kind(&TokenKind::LParen, &format!("'(' — {name} takes a pair, e.g. {example}"))?;
        let a = self.parse_expr()?;
        self.expect_kind(&TokenKind::Comma, &format!("',' between the two components of {name}"))?;
        let b = self.parse_expr()?;
        self.expect_kind(&TokenKind::RParen, &format!("')' closing {name}"))?;
        Ok((a, b))
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
        //
        // Orientation is glTF's: +Y up, +Z is the FRONT of every thing
        // (`north` is the front face), +X is right as seen from the front.
        // So `A in_front_of B` puts A at +Z of B — A's back (south) against
        // B's front (north). This was once the other way round, which no
        // right-handed viewer could reconcile with `right_of` = +X.
        let (sub_a, obj_a) = match predicate {
            RelationKind::Above       => ("bottom", "top"),
            RelationKind::Below       => ("top", "bottom"),
            RelationKind::LeftOf      => ("east", "west"),
            RelationKind::RightOf     => ("west", "east"),
            RelationKind::InFrontOf   => ("south", "north"),
            RelationKind::Behind      => ("north", "south"),
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
                offsets: Box::new(MateOffsets { shift: q.shift, lean: q.lean }),
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
        let subject = self.parse_indexed_ident()?;
        let expr = if self.is_relation_token() {
            let predicate = self.parse_relation_kind()?;
            let object = self.parse_indexed_ident()?;
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

    // ── fn (Phase E2) ────────────────────────────────────────────────────

    /// `fn NAME(a, b) = expr` — bare parameter names, no defaults (a
    /// function is not a thing; it has no instances to override anything).
    fn parse_fn_decl(&mut self) -> Result<FnDecl, MoxiError> {
        let span = self.span();
        self.advance(); // `fn`
        let name = self.expect_ident()?;
        self.expect_kind(&TokenKind::LParen, "'(' after the function name")?;
        let mut params = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            params.push(self.expect_ident()?);
            if matches!(self.peek_kind(), TokenKind::Comma) { self.advance(); }
        }
        self.expect_kind(&TokenKind::RParen, "')' closing the parameter list")?;
        self.expect_kind(&TokenKind::Eq, "'=' — a function body is a single expression")?;
        let body = self.parse_expr()?;
        Ok(FnDecl { name, params, body, span })
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
        let mut lhs = self.parse_expr_mul()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Plus  => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_expr_mul()?;
            lhs = Expr::BinOp { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_expr_mul(&mut self) -> Result<Expr, MoxiError> {
        let mut lhs = self.parse_expr_unary()?;
        loop {
            let op = match self.peek_kind() {
                TokenKind::Star  => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
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
        // Unary minus: `-lx`, `-(a + b)`, `-sin(30)`. A minus before a
        // DIGIT in prefix position is already folded into the literal by
        // the lexer; this handles everything else. Written as `0 - x` so
        // no new AST node and no new evaluator case: a type error still
        // names the operator (`-` needs a number).
        if matches!(self.peek_kind(), TokenKind::Minus) {
            self.advance();
            let operand = self.parse_expr_unary()?;
            return Ok(Expr::BinOp {
                op:  BinOp::Sub,
                lhs: Box::new(Expr::Int(0)),
                rhs: Box::new(operand),
            });
        }
        self.parse_expr_atom()
    }

    fn parse_expr_atom(&mut self) -> Result<Expr, MoxiError> {
        let span = self.span();
        match self.peek_kind().clone() {
            // Phase D: `if cond { a } else { b }`. Braces, like every
            // other block in the language; `else` mandatory because an
            // expression must have a value on every path.
            TokenKind::If => {
                self.advance();
                let cond = self.parse_expr()?;
                self.expect_kind(&TokenKind::LBrace, "'{' after the `if` condition")?;
                let then = self.parse_expr()?;
                self.expect_kind(&TokenKind::RBrace, "'}' closing the `if` branch")?;
                self.expect_kind(&TokenKind::Else,
                    "'else' — `if` is an expression and needs a value on both paths")?;
                self.expect_kind(&TokenKind::LBrace, "'{' after `else`")?;
                let else_ = self.parse_expr()?;
                self.expect_kind(&TokenKind::RBrace, "'}' closing the `else` branch")?;
                Ok(Expr::If {
                    cond:  Box::new(cond),
                    then:  Box::new(then),
                    else_: Box::new(else_),
                })
            }
            TokenKind::Int(n)       => { self.advance(); Ok(Expr::Int(n)) }
            TokenKind::Float(f)     => { self.advance(); Ok(Expr::Float(f)) }
            TokenKind::StringLit(s) => { self.advance(); Ok(Expr::Str(s)) }
            // Phase D.2: `[a, b, c]` or `[for i in a..b { expr }]`.
            TokenKind::LBracket => {
                self.advance();
                let list = if matches!(self.peek_kind(), TokenKind::For) {
                    self.advance();
                    let var = self.expect_ident()?;
                    self.expect_kind(&TokenKind::In, "'in' after the variable, e.g. `[for i in 0..12 { … }]`")?;
                    let start = self.parse_expr()?;
                    self.expect_kind(&TokenKind::Dot, "'..' between the range bounds")?;
                    self.expect_kind(&TokenKind::Dot, "'..' between the range bounds")?;
                    let end = self.parse_expr()?;
                    self.expect_kind(&TokenKind::LBrace, "'{' opening the element expression")?;
                    let body = self.parse_expr()?;
                    self.expect_kind(&TokenKind::RBrace, "'}' closing the element expression")?;
                    Expr::Comprehension {
                        var: Box::new(var), start: Box::new(start), end: Box::new(end), body: Box::new(body),
                    }
                } else {
                    let mut items = Vec::new();
                    while !matches!(self.peek_kind(), TokenKind::RBracket | TokenKind::Eof) {
                        items.push(self.parse_expr()?);
                        if matches!(self.peek_kind(), TokenKind::Comma) { self.advance(); }
                    }
                    Expr::List(items)
                };
                self.expect_kind(&TokenKind::RBracket, "']'")?;
                self.parse_index_postfix(list)
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect_kind(&TokenKind::RParen, "')'")?;
                self.parse_index_postfix(inner)
            }
            TokenKind::Ident(name) => {
                self.advance();
                let atom = if matches!(self.peek_kind(), TokenKind::LParen) {
                    let args = self.parse_named_args()?;
                    Expr::Call { name, args }
                } else {
                    Expr::Ident(Ident { name, span })
                };
                self.parse_index_postfix(atom)
            }
            other => Err(MoxiError::UnexpectedToken {
                got: format!("{other:?}"),
                expected: "expression".to_string(),
                span,
            }),
        }
    }

    /// `xs[i]`, `grid[i][j]` — 0-based list indexing, as many times as
    /// there are brackets. Only follows an atom that can hold a list
    /// (a name, a call, a literal, a parenthesized expression), so a `[`
    /// that opens the next statement is never mistaken for an index.
    fn parse_index_postfix(&mut self, mut base: Expr) -> Result<Expr, MoxiError> {
        while matches!(self.peek_kind(), TokenKind::LBracket) {
            self.advance();
            let index = self.parse_expr()?;
            self.expect_kind(&TokenKind::RBracket, "']' closing the index")?;
            base = Expr::Index { base: Box::new(base), index: Box::new(index) };
        }
        Ok(base)
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

struct Qualifiers {
    twist: Expr,
    pitch: Expr,
    gap:   Expr,
    shift: (Expr, Expr),
    lean:  (Expr, Expr),
    from:  Option<Ident>,
    axis:  Option<Axis>,
}

impl Default for Qualifiers {
    fn default() -> Self {
        let zero = || Expr::Float(0.0);
        Qualifiers {
            twist: zero(),
            pitch: zero(),
            gap:   zero(),
            shift: (zero(), zero()),
            lean:  (zero(), zero()),
            from:  None,
            axis:  None,
        }
    }
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

        let Some(ShapeExpr::Union { shapes, args }) = &e.parts[1].shape else {
            panic!("expected union");
        };
        assert_eq!(shapes.len(), 2);
        assert!(args.is_empty(), "no blend given, so no args");
    }

    #[test]
    fn mirror_and_scale_parse_as_wrappers() {
        let src = r#"
thing T {
    part A { shape = mirror(at(sphere(radius=1), x=3), axis=x) }
    part B { shape = scale(torus(major_radius=4, minor_radius=0.3), z=0.7) }
}
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(e) = &doc.items[0] else { panic!("expected a thing") };
        let Some(ShapeExpr::Mirror { inner, args }) = &e.parts[0].shape else {
            panic!("expected mirror");
        };
        assert!(matches!(**inner, ShapeExpr::At { .. }));
        assert_eq!(args[0].key, "axis");
        let Some(ShapeExpr::Scale { inner, args }) = &e.parts[1].shape else {
            panic!("expected scale");
        };
        assert!(matches!(**inner, ShapeExpr::Torus { .. }));
        assert_eq!(args[0].key, "z");
    }

    /// `blend=` is a trailing named argument on union: the parser must
    /// tell it apart from another shape operand, and reject it on
    /// intersect, which has no fillet.
    #[test]
    fn union_blend_parses_and_is_union_only() {
        let src = r#"
thing Blob {
    part Body {
        shape = union(sphere(radius=5), at(sphere(radius=3), y=6), blend=2.5)
    }
}
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(e) = &doc.items[0] else { panic!("expected a thing") };
        let Some(ShapeExpr::Union { shapes, args }) = &e.parts[0].shape else {
            panic!("expected union");
        };
        assert_eq!(shapes.len(), 2, "blend must not be read as a third operand");
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].key, "blend");

        let bad = src.replace("union(", "intersect(");
        let (_, errors) = parse_src(&bad);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::UnexpectedToken { expected, .. } if expected.contains("intersect takes no named"))),
            "expected a union-only error, got: {errors:?}");
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

    #[test]
    fn entity_params_and_instance_args_parse() {
        let src = r#"
entity Arm(length=9, girth=0.8) {
    part Bone { shape = cylinder(height=length, radius=girth) }
}
entity Body {
    part R { entity = Arm(length=12) }
    part L { entity = Arm }
}
"#;
        let (tokens, lex_errors) = Lexer::new(src).tokenize();
        assert!(lex_errors.is_empty(), "lex: {lex_errors:?}");
        let (doc, parse_errors) = Parser::new(tokens).parse();
        assert!(parse_errors.is_empty(), "parse: {parse_errors:?}");

        let TopLevel::EntityDecl(arm) = &doc.items[0] else { panic!("expected entity") };
        assert_eq!(arm.params.len(), 2);
        assert_eq!(arm.params[0].key, "length");

        let TopLevel::EntityDecl(body) = &doc.items[1] else { panic!("expected entity") };
        assert_eq!(body.parts[0].entity_args.len(), 1);
        assert_eq!(body.parts[0].entity_args[0].key, "length");
        assert!(body.parts[1].entity_args.is_empty());
    }

    /// `thing` is canonical and `entity` is its legacy synonym. Both
    /// spellings must parse to the identical AST, in both the declaration
    /// and the instance position — otherwise the compatibility window is a
    /// promise the compiler does not keep.
    #[test]
    fn thing_and_entity_are_the_same_keyword() {
        let new = r#"
thing Arm { part Bone { shape = cylinder(height=9, radius=0.8) } }
thing Body { part R { thing = Arm } }
"#;
        let old = r#"
entity Arm { part Bone { shape = cylinder(height=9, radius=0.8) } }
entity Body { part R { entity = Arm } }
"#;
        for src in [new, old] {
            let (doc, errors) = parse_src(src);
            assert!(errors.is_empty(), "unexpected errors: {errors:?}");
            assert_eq!(doc.items.len(), 2);

            let TopLevel::EntityDecl(arm) = &doc.items[0] else { panic!("expected a thing") };
            assert_eq!(arm.name.name, "Arm");

            let TopLevel::EntityDecl(body) = &doc.items[1] else { panic!("expected a thing") };
            assert_eq!(
                body.parts[0].entity.as_ref().map(|i| i.name.as_str()),
                Some("Arm"),
                "the instance form must bind the template either way",
            );
        }
    }

    /// Mixed spellings inside one file are legal during the window — a
    /// half-migrated script must not be a parse error.
    #[test]
    fn the_two_spellings_may_be_mixed() {
        let src = r#"
entity Arm { part Bone { shape = sphere(radius=1) } }
thing Body { part R { entity = Arm } }
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(doc.items.len(), 2);
    }

    // ── Phase D ───────────────────────────────────────────────────────

    #[test]
    fn let_bindings_and_if_expressions_parse() {
        let src = r#"
thing Gear(teeth=12) {
    let pitch = 360 / teeth
    let big   = if teeth > 10 { 1 } else { 0 }
    part Disc { shape = cylinder(height=2, radius=pitch) }
}
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(g) = &doc.items[0] else { panic!("expected a thing") };
        assert_eq!(g.lets.len(), 2);
        assert_eq!(g.lets[0].key, "pitch");
        assert!(matches!(g.lets[1].value, Expr::If { .. }));
        assert_eq!(g.parts.len(), 1);
    }

    /// Qualifiers accept expressions, not just literals, so a thing can
    /// parameterize its own pose: `Lower.top on Upper.bottom pitch=bend`.
    /// Phase D.2 surface: literals, comprehensions, indexing (chained).
    #[test]
    fn lists_comprehensions_and_indexing_parse() {
        let src = r#"
thing T(n=4) {
    let reaches = [3, 5.5, 8]
    let grid    = [for i in 0..n { [for j in 0..n { i * j }] }]
    let x       = reaches[1] + grid[2][3]
    part P { shape = sphere(radius=reaches[0]) }
}
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(t) = &doc.items[0] else { panic!("expected a thing") };
        assert!(matches!(t.lets[0].value, Expr::List(ref v) if v.len() == 3));
        let Expr::Comprehension { var, body, .. } = &t.lets[1].value else { panic!("expected a comprehension") };
        assert_eq!(var.name, "i");
        assert!(matches!(**body, Expr::Comprehension { .. }), "nested");
        let Expr::BinOp { rhs, .. } = &t.lets[2].value else { panic!("expected +") };
        let Expr::Index { base, .. } = &**rhs else { panic!("expected an index") };
        assert!(matches!(**base, Expr::Index { .. }), "grid[2][3] chains two indexes");
    }

    /// `-name` used to be a parse error with a cascade of follow-ons;
    /// every symmetric pair written with a variable hits it.
    #[test]
    fn unary_minus_applies_to_names_calls_and_groups() {
        let src = "thing T(w=4) { let a = -w  let b = -sin(30) * 2  let c = -(w + 1)  let d = 3 - -w }";
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(t) = &doc.items[0] else { panic!("expected a thing") };
        let env: crate::value::Env = [("w".to_string(), crate::value::Value::Num(4.0))].into_iter().collect();
        let v = |i: usize| crate::value::eval(&t.lets[i].value, &env).unwrap().as_num().unwrap();
        assert_eq!(v(0), -4.0);
        assert!((v(1) + 1.0).abs() < 1e-9, "unary minus binds tighter than *: (-sin 30) * 2 = -1");
        assert_eq!(v(2), -5.0);
        assert_eq!(v(3), 7.0);
    }

    #[test]
    fn lean_parses_as_a_pair_beside_shift() {
        let src = "thing T(up=30) { part A { shape = sphere(radius=1) } part B { shape = sphere(radius=1) } \
                   relation { A.bottom on B.top shift=(1, 2) lean=(up, 0) } }";
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(t) = &doc.items[0] else { panic!("expected a thing") };
        let Placement::Align { offsets, .. } = &t.relations[0] else { panic!("expected an align") };
        assert!(matches!(offsets.shift.0, Expr::Int(1)));
        assert!(matches!(offsets.lean.0, Expr::Ident(ref i) if i.name == "up"));
    }

    #[test]
    fn qualifiers_accept_expressions() {
        let src = r#"
thing Arm(bend=30) {
    part Upper { shape = capsule(height=7, radius=2) }
    part Lower { shape = capsule(height=6, radius=2) }
    relation {
        Lower.top on Upper.bottom pitch=bend twist=bend*2 gap=bend/30
    }
}
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(e) = &doc.items[0] else { panic!("expected a thing") };
        let Placement::Align { pitch, twist, gap, .. } = &e.relations[0] else {
            panic!("expected an align")
        };
        assert!(matches!(pitch, Expr::Ident(_)));
        assert!(matches!(twist, Expr::BinOp { .. }));
        assert!(matches!(gap, Expr::BinOp { .. }));
    }

    /// `if` is an expression, so it must have a value on every path.
    #[test]
    fn if_without_else_is_an_error() {
        let src = r#"
thing T(n=1) {
    let r = if n > 0 { 2 }
}
"#;
        let (_, errors) = parse_src(src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::UnexpectedToken { expected, .. } if expected.contains("else"))),
            "expected a missing-else error, got: {errors:?}");
    }

    /// E1's parser half: a positional call inside a shape argument.
    /// `key: String::new()` on positional args must not surface as a
    /// literal empty-string key anywhere downstream.
    #[test]
    fn positional_call_args_parse() {
        let src = r#"
thing T {
    part P { shape = sphere(radius=sin(90)) }
}
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(t) = &doc.items[0] else { panic!("expected a thing") };
        let Some(ShapeExpr::Sphere { args }) = &t.parts[0].shape else { panic!("expected sphere") };
        let Expr::Call { name, args: call_args } = &args[0].value else { panic!("expected a call") };
        assert_eq!(name, "sin");
        assert_eq!(call_args.len(), 1);
        assert_eq!(call_args[0].key, "");
        assert!(matches!(call_args[0].value, Expr::Int(90)));
    }

    /// Named args in the SAME list still work — shapes are unaffected.
    #[test]
    fn named_and_positional_args_can_mix_in_one_call() {
        let src = r#"
thing T {
    let x = clamp(5, 0, 10)
    part P { shape = box(width=2, height=2, depth=2) }
}
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(t) = &doc.items[0] else { panic!("expected a thing") };
        let Expr::Call { args, .. } = &t.lets[0].value else { panic!("expected a call") };
        assert_eq!(args.len(), 3);
        assert!(args.iter().all(|a| a.key.is_empty()));
    }

    #[test]
    fn fn_decl_parses() {
        let src = "fn taper(i, n) = sin(180 * (i + 0.5) / n)";
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::FnDecl(f) = &doc.items[0] else { panic!("expected a fn decl") };
        assert_eq!(f.name.name, "taper");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].name, "i");
        assert!(matches!(f.body, Expr::Call { .. }));
    }

    #[test]
    fn fn_body_must_be_an_expression_not_a_block() {
        let src = "fn taper(i) = { i }";
        let (_, errors) = parse_src(src);
        // `{` is not a valid expression start, so this is a parse error —
        // pins that a fn body cannot be a block, only an expression.
        assert!(!errors.is_empty());
    }

    // ── Phase E3 ──────────────────────────────────────────────────────

    #[test]
    fn pose_blocks_parse() {
        let src = r#"
thing Arm(n=3) {
    part Base { shape = sphere(radius=1) }
    part Wing { thing = W }
    pose Smash {
        Upper lean=(110, 10) twist=20
        Lower gap=n * 2
        Wing pose=Up
        for k in 1..n { Seg[k] lean=(30, 0) }
    }
}
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(t) = &doc.items[0] else { panic!("expected a thing") };
        assert_eq!(t.poses.len(), 1);
        let p = &t.poses[0];
        assert_eq!(p.name.name, "Smash");
        assert_eq!(p.lines.len(), 3);
        let PoseSet::Quals(q) = &p.lines[0].set else { panic!("expected qualifiers") };
        assert!(q.lean.is_some() && q.twist.is_some() && q.gap.is_none());
        assert!(matches!(&p.lines[2].set, PoseSet::Pose(id) if id.name == "Up"));
        assert_eq!(p.loops.len(), 1);
        assert_eq!(p.loops[0].lines[0].part.name, "Seg[#0]", "indexed names use the marker table");
    }

    #[test]
    fn for_block_parses_with_indexed_names() {
        let src = r#"
thing Row(n=3) {
    part Base { shape = box(width=10, height=1, depth=2) }
    for i in 0..n {
        let h = 2 + i
        part Post[i] { shape = cylinder(height=h, radius=0.3) }
        relation { Post[i].bottom on Base.top }
    }
}
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(t) = &doc.items[0] else { panic!("expected a thing") };
        assert_eq!(t.loops.len(), 1);
        let b = &t.loops[0];
        assert_eq!(b.var.name, "i");
        assert_eq!(b.lets.len(), 1);
        assert_eq!(b.parts.len(), 1);
        assert_eq!(b.parts[0].name.name, "Post[#0]", "index recorded as a marker");
        assert_eq!(b.relations.len(), 1);
        assert_eq!(t.index_exprs.len(), 2, "one for the part name, one for the relation");
    }

    #[test]
    fn nested_loops_and_multi_indices_parse() {
        let src = r#"
thing Grid {
    for i in 0..2 {
        for j in 0..3 {
            part C[i][j] { shape = sphere(radius=0.4) }
        }
    }
}
"#;
        let (doc, errors) = parse_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let TopLevel::EntityDecl(t) = &doc.items[0] else { panic!("expected a thing") };
        assert_eq!(t.loops[0].loops.len(), 1);
        assert_eq!(t.loops[0].loops[0].parts[0].name.name, "C[#0][#1]");
    }

    #[test]
    fn loop_body_rejects_items_that_do_not_repeat() {
        let src = r#"
thing T {
    for i in 0..3 {
        resolve voxel_size = 1.0
    }
}
"#;
        let (_, errors) = parse_src(src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::UnexpectedToken { expected, .. } if expected.contains("loop body item"))),
            "expected a loop-body vocabulary error, got: {errors:?}");
    }
}