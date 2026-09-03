use std::collections::{HashMap, HashSet};

use crate::anchors::analytic_extents;
use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::frame::Vec3;
use crate::frame_resolver::resolve_frames;

#[derive(Debug, Clone)]
pub struct ResolvedAtom {
    pub name:  String,
    pub color: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedMaterial {
    pub name:        String,
    pub color:       String,
    pub atom_index:  usize,
    pub extra_props: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedPart {
    pub name:           String,
    pub shape:          Option<ShapeExpr>,
    pub material_index: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ResolvedEntity {
    pub name:        String,
    pub parts:       Vec<ResolvedPart>,
    pub relations:   Vec<Placement>,
    pub constraints: Vec<ConstraintStmt>,
    pub resolve:     Option<ResolveOpts>,
}

#[derive(Debug, Clone)]
pub struct ResolvedScene {
    pub atoms:     Vec<ResolvedAtom>,
    pub materials: Vec<ResolvedMaterial>,
    pub entities:  Vec<ResolvedEntity>,
    pub prints:    Vec<PrintStmt>,
    pub refines:   Vec<RefineStmt>,
    /// Entities used as instance templates (`part X { entity = Y }` for
    /// some Y). They are components of other entities, not world layers —
    /// main.rs skips them when assembling the scene.
    pub instanced: HashSet<String>,
}

// ── Entity templates (composition) ─────────────────────────────────────────
//
// Every resolved entity is kept as a TEMPLATE so later entities can
// instance it. Templates are stored POST-flattening: their parts and
// relations already carry any nested-instance prefixes, and their exports
// point at flattened part names. Instancing a template therefore only ever
// prefixes one more level — nesting recurses for free.

#[derive(Debug, Clone)]
struct EntityTemplate {
    /// Declared parameters with their evaluated defaults, in order.
    params:    Vec<(String, f64)>,
    parts:     Vec<ResolvedPart>,
    relations: Vec<Placement>,
    /// Exported anchors, in declaration order: (export name, target).
    exports:   Vec<(String, AnchorRef)>,
    /// Pre-substitution forms, for re-substitution when an instance
    /// overrides parameters. `raw_shapes` holds this entity's OWN shaped
    /// parts (inherited parts are already concrete); `raw_relations` and
    /// `raw_exports` are the full lists before the default-env pass.
    raw_shapes:    HashMap<String, ShapeExpr>,
    raw_relations: Vec<Placement>,
    raw_exports:   Vec<(String, AnchorRef)>,
}

/// Prefix every part reference in a placement with `{prefix}.` — how a
/// template's internal relations are inlined into the instancing entity.
fn prefix_placement(p: &Placement, prefix: &str) -> Placement {
    let pre = |r: &AnchorRef| AnchorRef {
        part:   format!("{prefix}.{}", r.part),
        anchor: r.anchor.clone(),
        args:   r.args.clone(),
        span:   r.span,
    };
    match p {
        Placement::Align { subject, object, twist, pitch, gap, shift, span } => Placement::Align {
            subject: pre(subject),
            object:  pre(object),
            twist: *twist, pitch: *pitch, gap: *gap, shift: *shift,
            span: *span,
        },
        Placement::Mirror { subject, source, plane, axis, span } => Placement::Mirror {
            subject: format!("{prefix}.{subject}"),
            source:  format!("{prefix}.{source}"),
            plane:   pre(plane),
            axis: *axis,
            span: *span,
        },
    }
}

// ── Parameter substitution (Phase C) ──────────────────────────────────────
//
// Parameters substitute into shape arguments and relation anchor
// arguments, with constant folding: `radius = girth * 0.9` becomes a
// literal at resolve time. Idents that aren't parameters (axis names
// like `z`, material refs) pass through untouched.

type ParamEnv = HashMap<String, f64>;

fn eval_const(expr: &Expr, env: &ParamEnv) -> Option<f64> {
    match expr {
        Expr::Int(n)   => Some(*n as f64),
        Expr::Float(f) => Some(*f),
        Expr::Ident(i) => env.get(&i.name).copied(),
        Expr::BinOp { op, lhs, rhs } => {
            let (l, r) = (eval_const(lhs, env)?, eval_const(rhs, env)?);
            match op {
                BinOp::Add => Some(l + r),
                BinOp::Sub => Some(l - r),
                BinOp::Mul => Some(l * r),
                BinOp::Div => if r != 0.0 { Some(l / r) } else { None },
                _ => None,
            }
        }
        _ => None,
    }
}

fn subst_expr(expr: &Expr, env: &ParamEnv) -> Expr {
    if let Some(v) = eval_const(expr, env) {
        // Fold anything fully constant under this env — but leave bare
        // literals alone (no-op) and non-parameter idents untouched.
        match expr {
            Expr::Int(_) | Expr::Float(_) => expr.clone(),
            Expr::Ident(i) if !env.contains_key(&i.name) => expr.clone(),
            _ => Expr::Float(v),
        }
    } else {
        match expr {
            Expr::BinOp { op, lhs, rhs } => Expr::BinOp {
                op:  op.clone(),
                lhs: Box::new(subst_expr(lhs, env)),
                rhs: Box::new(subst_expr(rhs, env)),
            },
            other => other.clone(),
        }
    }
}

fn subst_args(args: &[NamedArg], env: &ParamEnv) -> Vec<NamedArg> {
    args.iter().map(|a| NamedArg {
        key:   a.key.clone(),
        value: subst_expr(&a.value, env),
    }).collect()
}

fn subst_shape(shape: &ShapeExpr, env: &ParamEnv) -> ShapeExpr {
    match shape {
        ShapeExpr::Box_ { args }        => ShapeExpr::Box_ { args: subst_args(args, env) },
        ShapeExpr::Sphere { args }      => ShapeExpr::Sphere { args: subst_args(args, env) },
        ShapeExpr::Cylinder { args }    => ShapeExpr::Cylinder { args: subst_args(args, env) },
        ShapeExpr::Cone { args }        => ShapeExpr::Cone { args: subst_args(args, env) },
        ShapeExpr::Ellipsoid { args }   => ShapeExpr::Ellipsoid { args: subst_args(args, env) },
        ShapeExpr::Blob { args }        => ShapeExpr::Blob { args: subst_args(args, env) },
        ShapeExpr::Heightfield { args } => ShapeExpr::Heightfield { args: subst_args(args, env) },
        ShapeExpr::Shell { inner, args } => ShapeExpr::Shell {
            inner: Box::new(subst_shape(inner, env)), args: subst_args(args, env),
        },
        ShapeExpr::Extrude { profile, args } => ShapeExpr::Extrude {
            profile: Box::new(subst_shape(profile, env)), args: subst_args(args, env),
        },
        ShapeExpr::Union { shapes } => ShapeExpr::Union {
            shapes: shapes.iter().map(|x| subst_shape(x, env)).collect(),
        },
        ShapeExpr::Intersect { shapes } => ShapeExpr::Intersect {
            shapes: shapes.iter().map(|x| subst_shape(x, env)).collect(),
        },
        ShapeExpr::Difference { base, cuts } => ShapeExpr::Difference {
            base: Box::new(subst_shape(base, env)),
            cuts: cuts.iter().map(|x| subst_shape(x, env)).collect(),
        },
        ShapeExpr::At { inner, args } => ShapeExpr::At {
            inner: Box::new(subst_shape(inner, env)), args: subst_args(args, env),
        },
        ShapeExpr::Spin { inner, args } => ShapeExpr::Spin {
            inner: Box::new(subst_shape(inner, env)), args: subst_args(args, env),
        },
    }
}

fn subst_anchor_ref(r: &AnchorRef, env: &ParamEnv) -> AnchorRef {
    AnchorRef {
        part:   r.part.clone(),
        anchor: r.anchor.clone(),
        args:   subst_args(&r.args, env),
        span:   r.span,
    }
}

fn subst_placement(p: &Placement, env: &ParamEnv) -> Placement {
    match p {
        Placement::Align { subject, object, twist, pitch, gap, shift, span } => Placement::Align {
            subject: subst_anchor_ref(subject, env),
            object:  subst_anchor_ref(object, env),
            twist: *twist, pitch: *pitch, gap: *gap, shift: *shift,
            span: *span,
        },
        Placement::Mirror { subject, source, plane, axis, span } => Placement::Mirror {
            subject: subject.clone(),
            source:  source.clone(),
            plane:   subst_anchor_ref(plane, env),
            axis: *axis,
            span: *span,
        },
    }
}

pub struct Resolver {
    errors:          Vec<MoxiError>,
    atom_index:      HashMap<String, usize>,
    material_index:  HashMap<String, usize>,
    entity_index:    HashMap<String, usize>,
    generator_index: HashMap<String, usize>,
    templates:       HashMap<String, EntityTemplate>,
    instanced:       HashSet<String>,
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            errors:          Vec::new(),
            atom_index:      HashMap::new(),
            material_index:  HashMap::new(),
            entity_index:    HashMap::new(),
            generator_index: HashMap::new(),
            templates:       HashMap::new(),
            instanced:       HashSet::new(),
        }
    }

    pub fn resolve(mut self, doc: Document) -> (ResolvedScene, Vec<MoxiError>) {
        // Pass 1 — register all names so forward references work
        for item in &doc.items {
            match item {
                TopLevel::AtomDecl(a)      => self.register_atom(a),
                TopLevel::MaterialDecl(m)  => self.register_material_name(m),
                TopLevel::EntityDecl(e)    => self.register_entity_name(e),
                TopLevel::GeneratorDecl(g) => self.register_generator_name(g),
                _ => {}
            }
        }

        // Pass 2a — every declared atom, in declaration order, BEFORE any
        // material resolves. Materials without `voxel_atom` synthesize an
        // atom by appending, so this ordering is what guarantees a
        // synthesized index can never collide with or displace a declared
        // one. Do not fold this back into the main loop.
        let mut atoms: Vec<ResolvedAtom> = Vec::new();
        for item in &doc.items {
            if let TopLevel::AtomDecl(a) = item {
                atoms.push(self.resolve_atom(a.clone()));
            }
        }

        // Pass 2b — resolve the remaining bodies
        let mut materials = Vec::new();
        let mut entities  = Vec::new();
        let mut prints    = Vec::new();
        let mut refines   = Vec::new();

        for item in doc.items {
            match item {
                TopLevel::AtomDecl(_) => {} // handled in pass 2a
                TopLevel::MaterialDecl(m) => {
                    if let Some(mat) = self.resolve_material(m, &mut atoms) {
                        materials.push(mat);
                    }
                }
                TopLevel::EntityDecl(e) => {
                    if let Some(ent) = self.resolve_entity(e) {
                        entities.push(ent);
                    }
                }
                TopLevel::PrintStmt(p) => {
                    self.check_entity_ref(&p.target);
                    prints.push(p);
                }
                TopLevel::RefineStmt(r) => {
                    if let Some(root) = r.path.first() {
                        self.check_entity_ref(root);
                    }
                    refines.push(r);
                }
                _ => {}
            }
        }

        let instanced = std::mem::take(&mut self.instanced);
        (ResolvedScene { atoms, materials, entities, prints, refines, instanced }, self.errors)
    }

    // ── Pass 1: registration ───────────────────────────────────────────────

    fn register_atom(&mut self, a: &AtomDecl) {
        let idx = self.atom_index.len();
        if self.atom_index.insert(a.name.name.clone(), idx).is_some() {
            self.errors.push(MoxiError::DuplicateName {
                name: a.name.name.clone(), span: a.name.span,
            });
        }
    }

    fn register_material_name(&mut self, m: &MaterialDecl) {
        let idx = self.material_index.len();
        if self.material_index.insert(m.name.name.clone(), idx).is_some() {
            self.errors.push(MoxiError::DuplicateName {
                name: m.name.name.clone(), span: m.name.span,
            });
        }
    }

    fn register_entity_name(&mut self, e: &EntityDecl) {
        let idx = self.entity_index.len();
        if self.entity_index.insert(e.name.name.clone(), idx).is_some() {
            self.errors.push(MoxiError::DuplicateName {
                name: e.name.name.clone(), span: e.name.span,
            });
        }
    }

    fn register_generator_name(&mut self, g: &GeneratorDecl) {
        let idx = self.generator_index.len();
        if self.generator_index.insert(g.name.name.clone(), idx).is_some() {
            self.errors.push(MoxiError::DuplicateName {
                name: g.name.name.clone(), span: g.name.span,
            });
        }
    }

    // ── Pass 2: body resolution ────────────────────────────────────────────

    fn resolve_atom(&self, a: AtomDecl) -> ResolvedAtom {
        let color = self.extract_str_prop(&a.props, "color")
            .unwrap_or_else(|| "white".to_string());
        ResolvedAtom { name: a.name.name, color }
    }

    /// `voxel_atom` is OPTIONAL. Named: bind that atom, so several
    /// materials can share one. Absent: synthesize a private atom from
    /// this material's own color, which is what makes
    /// `material Bone { color = ivory }` complete on its own.
    ///
    /// Synthesis APPENDS. Every declared atom is resolved before any
    /// material runs, so `atoms.len()` is the first free index and no
    /// declared index ever moves. That invariant is load-bearing:
    /// `grid_to_scene` resolves a voxel's color by atom index, so a
    /// shifted index is a silently wrong color, not a compile error.
    fn resolve_material(
        &mut self,
        m:     MaterialDecl,
        atoms: &mut Vec<ResolvedAtom>,
    ) -> Option<ResolvedMaterial> {
        let color = self.extract_str_prop(&m.props, "color")
            .unwrap_or_else(|| "white".to_string());

        let atom_name = self.extract_str_prop(&m.props, "voxel_atom");
        let atom_index = match atom_name {
            Some(ref name) => match self.atom_index.get(name).copied() {
                Some(idx) => idx,
                None => {
                    self.errors.push(MoxiError::UndefinedAtom {
                        name: name.clone(), span: m.name.span,
                    });
                    return None;
                }
            },
            None => {
                let idx = atoms.len();
                atoms.push(ResolvedAtom {
                    name:  m.name.name.clone(),
                    color: color.clone(),
                });
                idx
            }
        };

        let mut extra_props = HashMap::new();
        for prop in &m.props {
            if prop.key != "color" && prop.key != "voxel_atom" {
                extra_props.insert(prop.key.clone(), self.expr_to_str(&prop.value));
            }
        }

        Some(ResolvedMaterial { name: m.name.name, color, atom_index, extra_props })
    }

    /// Resolve one entity, FLATTENING any instance parts:
    ///
    ///   1. `part X { entity = Arm }` inlines Arm's (already flattened)
    ///      parts as `X.Humerus`, `X.Forearm`, … plus Arm's internal
    ///      relations with the same prefix.
    ///   2. Placements naming `X.socket` rewrite through Arm's exported
    ///      anchors to the real part anchor (`X.Humerus.top`). When the
    ///      instance is the SUBJECT of a placement, the socket must live
    ///      on the instance's root part (a part with no internal
    ///      placement), or the whole subassembly could not move rigidly.
    ///   3. `LeftX symmetric_across P from=RightX` between two instances
    ///      of the same template expands into one Mirror per part, and
    ///      LeftX's internal relations are dropped — a mirrored instance's
    ///      internal structure is fully determined by its source.
    ///
    /// A.2: if a referenced anchor is NOT an export but IS a universal
    /// compass name, it resolves against the instance's ASSEMBLY extents —
    /// the template's frames are solved analytically, the union AABB of
    /// its parts computed, and the reference rewritten to a `point()`
    /// anchor on the instance's root part. `Middle.west on Left.east`
    /// between instances now just works, no exports required.
    ///
    /// The finished entity is stored as a template so later entities can
    /// instance it in turn (declare-before-instance is required).
    fn resolve_entity(&mut self, e: EntityDecl) -> Option<ResolvedEntity> {
        // Phase C: evaluate parameter defaults (must be constants).
        let mut env_default: ParamEnv = HashMap::new();
        let mut params_vec: Vec<(String, f64)> = Vec::new();
        for p in &e.params {
            match eval_const(&p.value, &env_default) {
                Some(v) => {
                    env_default.insert(p.key.clone(), v);
                    params_vec.push((p.key.clone(), v));
                }
                None => {
                    self.errors.push(MoxiError::InstanceError {
                        instance: e.name.name.clone(),
                        message:  format!(
                            "parameter '{}' needs a constant default value", p.key),
                        span: p.span,
                    });
                    env_default.insert(p.key.clone(), 0.0);
                    params_vec.push((p.key.clone(), 0.0));
                }
            }
        }

        let mut parts: Vec<ResolvedPart> = Vec::new();
        // Own shaped parts, pre-substitution — the template's raw forms.
        let mut raw_shapes: HashMap<String, ShapeExpr> = HashMap::new();
        // Per-instance effective exports (substituted with that
        // instance's parameter environment).
        let mut instance_exports: HashMap<String, Vec<(String, AnchorRef)>> = HashMap::new();
        // Every declared name (shape part or instance), for duplicate checks.
        let mut declared: HashMap<String, Span> = HashMap::new();
        // Flattened SHAPED part names — what placements may reference.
        let mut part_names: HashMap<String, Span> = HashMap::new();
        // Relations inlined from instance templates (prefixed).
        let mut internal_relations: Vec<Placement> = Vec::new();
        // instance name → template name
        let mut instance_of: HashMap<String, String> = HashMap::new();
        // instance name → its internally-placed (non-root) part names
        let mut internal_subjects: HashMap<String, HashSet<String>> = HashMap::new();

        for part in e.parts {
            let PartDecl { name, shape, entity, entity_args, material, span: _ } = part;
            let pname = name.name.clone();

            if declared.contains_key(&pname) {
                self.errors.push(MoxiError::DuplicateName {
                    name: pname, span: name.span,
                });
                continue;
            }
            declared.insert(pname.clone(), name.span);

            match (shape, entity) {
                (Some(_), Some(tmpl)) => {
                    self.errors.push(MoxiError::InstanceError {
                        instance: pname,
                        message:  format!(
                            "a part is either a shape or an instance of '{}', not both",
                            tmpl.name),
                        span: name.span,
                    });
                }

                // Instance: inline the template with a `pname.` prefix.
                (None, Some(tmpl_ident)) => {
                    let Some(tmpl) = self.templates.get(&tmpl_ident.name).cloned() else {
                        let message = if self.entity_index.contains_key(&tmpl_ident.name) {
                            format!("thing '{}' must be declared before it is instanced",
                                    tmpl_ident.name)
                        } else {
                            format!("thing '{}' is not defined", tmpl_ident.name)
                        };
                        self.errors.push(MoxiError::InstanceError {
                            instance: pname, message, span: tmpl_ident.span,
                        });
                        continue;
                    };
                    self.instanced.insert(tmpl_ident.name.clone());

                    // Phase C: build this instance's parameter environment
                    // — template defaults overridden by constant instance
                    // arguments — and re-substitute the template's raw
                    // forms when anything is overridden.
                    let mut child_env: ParamEnv =
                        tmpl.params.iter().cloned().collect();
                    let mut bad_args = false;
                    for arg in &entity_args {
                        if !tmpl.params.iter().any(|(n, _)| n == &arg.key) {
                            let valid = if tmpl.params.is_empty() {
                                format!("thing '{}' takes no parameters",
                                        tmpl_ident.name)
                            } else {
                                format!("parameters of '{}': {}",
                                        tmpl_ident.name,
                                        tmpl.params.iter().map(|(n, _)| n.as_str())
                                            .collect::<Vec<_>>().join(", "))
                            };
                            self.errors.push(MoxiError::InstanceError {
                                instance: pname.clone(),
                                message:  format!(
                                    "unknown parameter '{}' — {valid}", arg.key),
                                span: tmpl_ident.span,
                            });
                            bad_args = true;
                            continue;
                        }
                        match eval_const(&arg.value, &HashMap::new()) {
                            Some(v) => { child_env.insert(arg.key.clone(), v); }
                            None => {
                                self.errors.push(MoxiError::InstanceError {
                                    instance: pname.clone(),
                                    message:  format!(
                                        "argument '{}' must be a constant \
                                         expression (parameter pass-through \
                                         is a later phase)", arg.key),
                                    span: tmpl_ident.span,
                                });
                                bad_args = true;
                            }
                        }
                    }
                    let overridden = !entity_args.is_empty() && !bad_args;

                    for tp in &tmpl.parts {
                        let full  = format!("{pname}.{}", tp.name);
                        let shape = if overridden {
                            match tmpl.raw_shapes.get(&tp.name) {
                                Some(raw) => Some(subst_shape(raw, &child_env)),
                                None      => tp.shape.clone(),
                            }
                        } else {
                            tp.shape.clone()
                        };
                        part_names.insert(full.clone(), name.span);
                        parts.push(ResolvedPart {
                            name:           full,
                            shape,
                            material_index: tp.material_index,
                        });
                    }

                    let rel_source: Vec<Placement> = if overridden {
                        tmpl.raw_relations.iter()
                            .map(|r| subst_placement(r, &child_env)).collect()
                    } else {
                        tmpl.relations.clone()
                    };
                    let mut subs = HashSet::new();
                    for tr in &rel_source {
                        let pr = prefix_placement(tr, &pname);
                        subs.insert(pr.subject_name().to_string());
                        internal_relations.push(pr);
                    }
                    internal_subjects.insert(pname.clone(), subs);

                    let exports: Vec<(String, AnchorRef)> = if overridden {
                        tmpl.raw_exports.iter()
                            .map(|(n, ar)| (n.clone(), subst_anchor_ref(ar, &child_env)))
                            .collect()
                    } else {
                        tmpl.exports.clone()
                    };
                    instance_exports.insert(pname.clone(), exports);
                    instance_of.insert(pname, tmpl_ident.name.clone());
                }

                // Plain shaped part (shape may be None, as before).
                (shape, None) => {
                    let material_index = match &material {
                        Some(mat) => match self.material_index.get(&mat.name).copied() {
                            Some(idx) => Some(idx),
                            None => {
                                self.errors.push(MoxiError::UndefinedMaterial {
                                    name: mat.name.clone(), span: mat.span,
                                });
                                None
                            }
                        },
                        None => None,
                    };
                    part_names.insert(pname.clone(), name.span);
                    if let Some(ref sh) = shape {
                        raw_shapes.insert(pname.clone(), sh.clone());
                    }
                    let shape = shape.map(|sh| subst_shape(&sh, &env_default));
                    parts.push(ResolvedPart { name: pname, shape, material_index });
                }
            }
        }

        // ── Rewrite this entity's own placements through the instances ────

        let mut rewritten: Vec<Placement> = Vec::new();
        let mut mirrored: HashSet<String> = HashSet::new();

        for pl in e.relations {
            match pl {
                Placement::Align { subject, object, twist, pitch, gap, shift, span } => {
                    let orig_part   = subject.part.clone();
                    let orig_anchor = subject.anchor.clone();
                    let subj_inst   = instance_of.get(&subject.part).cloned();

                    let Some(subject) = self.map_anchor_ref(subject, &instance_of,
                        &instance_exports, &parts, &internal_relations) else { continue };
                    let Some(object)  = self.map_anchor_ref(object,  &instance_of,
                        &instance_exports, &parts, &internal_relations) else { continue };

                    // Root-socket rule: an instance used as the SUBJECT
                    // must be gripped by its root part, or its internal
                    // chain would place that part twice.
                    if let Some(tname) = subj_inst {
                        let non_root = internal_subjects.get(&orig_part)
                            .is_some_and(|s| s.contains(&subject.part));
                        if non_root {
                            self.errors.push(MoxiError::InstanceError {
                                instance: orig_part,
                                message:  format!(
                                    "anchor '{orig_anchor}' resolves to '{}', which is already \
                                     placed by a relation inside '{tname}'; when an instance is \
                                     the subject of a placement, its anchor must live on the \
                                     instance's root part",
                                    subject.part),
                                span,
                            });
                            continue;
                        }
                    }

                    rewritten.push(Placement::Align {
                        subject, object, twist, pitch, gap, shift, span,
                    });
                }

                Placement::Mirror { subject, source, plane, axis, span } => {
                    let s_t = instance_of.get(&subject).cloned();
                    let r_t = instance_of.get(&source).cloned();
                    let Some(plane) = self.map_anchor_ref(plane, &instance_of,
                        &instance_exports, &parts, &internal_relations) else { continue };

                    match (s_t, r_t) {
                        // Instance-to-instance: expand into one Mirror per
                        // part; the mirrored instance's internal relations
                        // are dropped below (its structure is fully
                        // determined by the source).
                        (Some(st), Some(rt)) => {
                            if st != rt {
                                self.errors.push(MoxiError::InstanceError {
                                    instance: subject,
                                    message:  format!(
                                        "cannot mirror '{source}' (a '{rt}') into a '{st}'; \
                                         both sides of symmetric_across must instance the \
                                         same entity"),
                                    span,
                                });
                                continue;
                            }
                            let tmpl_part_names: Vec<String> = self.templates.get(&st)
                                .map(|t| t.parts.iter().map(|p| p.name.clone()).collect())
                                .unwrap_or_default();
                            mirrored.insert(subject.clone());
                            for tp in &tmpl_part_names {
                                rewritten.push(Placement::Mirror {
                                    subject: format!("{subject}.{tp}"),
                                    source:  format!("{source}.{tp}"),
                                    plane:   plane.clone(),
                                    axis, span,
                                });
                            }
                        }

                        (None, None) => {
                            rewritten.push(Placement::Mirror { subject, source, plane, axis, span });
                        }

                        (s_some, _) => {
                            let offender = if s_some.is_some() { subject } else { source };
                            self.errors.push(MoxiError::InstanceError {
                                instance: offender,
                                message:  "symmetric_across between an instance and a plain \
                                           part is not supported; mirror instance-to-instance \
                                           or part-to-part".to_string(),
                                span,
                            });
                        }
                    }
                }
            }
        }

        // Mirrored instances contribute no internal relations — every one
        // of their parts is placed by an expanded Mirror instead.
        // `raw_full` keeps parameter idents intact (the template's raw
        // form); `relations` is the default-env substitution of it — the
        // concrete list this entity validates, solves, and rasterizes.
        let raw_full: Vec<Placement> = internal_relations.iter()
            .filter(|p| {
                let inst = p.subject_name().split('.').next().unwrap_or("");
                !mirrored.contains(inst)
            })
            .cloned()
            .chain(rewritten)
            .collect();
        let relations: Vec<Placement> = raw_full.iter()
            .map(|p| subst_placement(p, &env_default))
            .collect();

        // ── Exports (this entity's own sockets) ────────────────────────────

        let shape_by_name: HashMap<&str, &ShapeExpr> = parts.iter()
            .filter_map(|p| p.shape.as_ref().map(|s| (p.name.as_str(), s)))
            .collect();

        let mut exports:     Vec<(String, AnchorRef)> = Vec::new();
        let mut raw_exports: Vec<(String, AnchorRef)> = Vec::new();
        for a in e.anchors {
            let Some(raw_target) = self.map_anchor_ref(a.target, &instance_of,
                &instance_exports, &parts, &internal_relations) else { continue };
            let target = subst_anchor_ref(&raw_target, &env_default);
            self.check_anchor_ref(&target, &part_names, &shape_by_name);
            if exports.iter().any(|(n, _)| n == &a.name.name) {
                self.errors.push(MoxiError::DuplicateName {
                    name: a.name.name.clone(), span: a.name.span,
                });
            } else {
                exports.push((a.name.name.clone(), target));
                raw_exports.push((a.name.name.clone(), raw_target));
            }
        }

        // Validate placements POST-flattening: part names exist AND anchor
        // names/args are valid for each part's actual shape — static
        // UndefinedAnchor errors with the shape's full vocabulary.
        for pl in &relations {
            match pl {
                Placement::Align { subject, object, .. } => {
                    self.check_anchor_ref(subject, &part_names, &shape_by_name);
                    self.check_anchor_ref(object,  &part_names, &shape_by_name);
                }
                Placement::Mirror { subject, source, plane, span, .. } => {
                    self.check_part_name(subject, *span, &part_names);
                    self.check_part_name(source,  *span, &part_names);
                    self.check_anchor_ref(plane, &part_names, &shape_by_name);
                }
            }
        }

        // Validate constraint names reference known parts
        for con in &e.constraints {
            match &con.expr {
                ConstraintExpr::Relation(r) => {
                    self.check_part_ref(&r.subject, &part_names);
                    self.check_part_ref(&r.object,  &part_names);
                }
                ConstraintExpr::Bound { name, .. } => {
                    self.check_part_ref(name, &part_names);
                }
            }
        }

        // Register as a template for later entities to instance.
        self.templates.insert(e.name.name.clone(), EntityTemplate {
            params:        params_vec,
            parts:         parts.clone(),
            relations:     relations.clone(),
            exports,
            raw_shapes,
            raw_relations: raw_full,
            raw_exports,
        });

        Some(ResolvedEntity {
            name:        e.name.name,
            parts,
            relations,
            constraints: e.constraints,
            resolve:     e.resolve,
        })
    }

    /// Rewrite an anchor reference through instance exports:
    /// `RightArm.socket` → `RightArm.Humerus.top` (with the export's args,
    /// unless the reference supplies its own). References to plain parts
    /// pass through untouched.
    ///
    /// A.2 fallback: a compass name that isn't an export resolves against
    /// the instance's assembly extents (see `instance_compass_anchor`).
    /// Anything else errors with the template's export vocabulary — the
    /// composition-level twin of the shape-anchor suggestion.
    fn map_anchor_ref(
        &mut self,
        r:                AnchorRef,
        instance_of:      &HashMap<String, String>,
        instance_exports: &HashMap<String, Vec<(String, AnchorRef)>>,
        flat_parts:       &[ResolvedPart],
        flat_rels:        &[Placement],
    ) -> Option<AnchorRef> {
        let Some(tmpl_name) = instance_of.get(&r.part) else { return Some(r) };
        let tmpl_name = tmpl_name.clone();

        // Template missing ⇒ the instance error was already reported.
        if !self.templates.contains_key(&tmpl_name) { return None; }

        // Phase C: exports come from the INSTANCE (substituted with its
        // parameter environment), not the template.
        let empty: Vec<(String, AnchorRef)> = Vec::new();
        let inst_exports = instance_exports.get(&r.part).unwrap_or(&empty);
        let export: Option<AnchorRef> = inst_exports.iter()
            .find(|(n, _)| n == &r.anchor)
            .map(|(_, e)| e.clone());
        let export_names: Vec<String> =
            inst_exports.iter().map(|(n, _)| n.clone()).collect();

        match export {
            Some(exp) => Some(AnchorRef {
                part:   format!("{}.{}", r.part, exp.part),
                anchor: exp.anchor,
                args:   if r.args.is_empty() { exp.args } else { r.args },
                span:   r.span,
            }),
            None => {
                // A.2: universal compass anchors on the whole assembly.
                const COMPASS: &[&str] =
                    &["center", "top", "bottom", "north", "south", "east", "west"];
                if COMPASS.contains(&r.anchor.as_str()) {
                    if let Some(ar) = self.instance_compass_anchor(
                        &r.part, &r.anchor, r.span, flat_parts, flat_rels)
                    {
                        return Some(ar);
                    }
                }

                let valid = if export_names.is_empty() {
                    format!("compass anchors (center/top/bottom/north/south/east/west), \
                             or add `anchor NAME = Part.anchor` inside thing '{tmpl_name}'")
                } else {
                    format!("{}, or the compass anchors \
                             (center/top/bottom/north/south/east/west)",
                            export_names.join(", "))
                };
                self.errors.push(MoxiError::UndefinedAnchor {
                    part: r.part, anchor: r.anchor, valid, span: r.span,
                });
                None
            }
        }
    }

    /// A.2 — synthesize a compass anchor on an instance's ASSEMBLY:
    /// solve the template's internal frames (analytic, no voxels), take
    /// the union AABB of every part's transformed extents, compute the
    /// compass point + outward normal on that box, and carry it as a
    /// `point()` anchor on the instance's ROOT part — whose template-local
    /// frame is the identity, so template coordinates ARE its local
    /// coordinates. Returns None if the template can't be solved (its own
    /// errors were already reported).
    /// A.2 (+C) — synthesize a compass anchor on an instance's ASSEMBLY:
    /// take the instance's already-flattened parts (so parameter
    /// overrides are reflected — a length=12 arm has a longer box than a
    /// length=9 one), solve their internal frames, take the union AABB,
    /// and carry the compass point + outward normal as a `point()` anchor
    /// on the instance's root part (frame = identity, so no coordinate
    /// change). Returns None if the instance can't be solved.
    fn instance_compass_anchor(
        &self,
        instance:   &str,
        anchor:     &str,
        span:       Span,
        flat_parts: &[ResolvedPart],
        flat_rels:  &[Placement],
    ) -> Option<AnchorRef> {
        let prefix = format!("{instance}.");

        let parts: Vec<(String, ShapeExpr)> = flat_parts.iter()
            .filter(|p| p.name.starts_with(&prefix))
            .filter_map(|p| p.shape.clone().map(|s| (p.name.clone(), s)))
            .collect();
        if parts.is_empty() {
            return None;
        }
        let rels: Vec<Placement> = flat_rels.iter()
            .filter(|p| p.subject_name().starts_with(&prefix))
            .cloned()
            .collect();
        let frames = resolve_frames(&parts, &rels).ok()?;

        // Assembly AABB in instance space.
        let mut min = Vec3::new(f64::MAX, f64::MAX, f64::MAX);
        let mut max = Vec3::new(f64::MIN, f64::MIN, f64::MIN);
        for (name, shape) in &parts {
            let f = frames.get(name)?;
            let e = analytic_extents(shape);
            for &cx in &[e.min.x, e.max.x] {
                for &cy in &[e.min.y, e.max.y] {
                    for &cz in &[e.min.z, e.max.z] {
                        let p = f.apply_point(Vec3::new(cx, cy, cz));
                        min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
                        max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
                    }
                }
            }
        }
        let c = min.add(max).scale(0.5);

        let (pos, normal) = match anchor {
            "center" => (c, None),
            "top"    => (Vec3::new(c.x, max.y, c.z), Some(Vec3::Y)),
            "bottom" => (Vec3::new(c.x, min.y, c.z), Some(Vec3::Y.neg())),
            "north"  => (Vec3::new(c.x, c.y, max.z), Some(Vec3::Z)),
            "south"  => (Vec3::new(c.x, c.y, min.z), Some(Vec3::Z.neg())),
            "east"   => (Vec3::new(max.x, c.y, c.z), Some(Vec3::X)),
            "west"   => (Vec3::new(min.x, c.y, c.z), Some(Vec3::X.neg())),
            _ => return None,
        };

        // Carrier: the first declared shaped part with no internal
        // placement — a solver root (frame = identity). Names are already
        // instance-prefixed, so no re-prefixing.
        let subjects: HashSet<&str> =
            rels.iter().map(|p| p.subject_name()).collect();
        let (root_name, _) = parts.iter().find(|(n, _)| !subjects.contains(n.as_str()))?;

        let fnum = |v: f64| Expr::Float(v);
        let mut args = vec![
            NamedArg { key: "x".into(), value: fnum(pos.x) },
            NamedArg { key: "y".into(), value: fnum(pos.y) },
            NamedArg { key: "z".into(), value: fnum(pos.z) },
        ];
        match normal {
            Some(n) => args.extend([
                NamedArg { key: "nx".into(), value: fnum(n.x) },
                NamedArg { key: "ny".into(), value: fnum(n.y) },
                NamedArg { key: "nz".into(), value: fnum(n.z) },
            ]),
            None => args.push(NamedArg { key: "free".into(), value: Expr::Int(1) }),
        }

        Some(AnchorRef {
            part:   root_name.clone(),
            anchor: "point".to_string(),
            args,
            span,
        })
    }

    fn check_entity_ref(&mut self, ident: &Ident) {
        if !self.entity_index.contains_key(&ident.name) {
            self.errors.push(MoxiError::UndefinedName {
                name: ident.name.clone(), span: ident.span,
            });
        }
    }

    fn check_part_ref(&mut self, ident: &Ident, known: &HashMap<String, Span>) {
        if !known.contains_key(&ident.name) {
            self.errors.push(MoxiError::UndefinedName {
                name: ident.name.clone(), span: ident.span,
            });
        }
    }

    fn check_part_name(&mut self, name: &str, span: Span, known: &HashMap<String, Span>) {
        if !known.contains_key(name) {
            self.errors.push(MoxiError::UndefinedName {
                name: name.to_string(), span,
            });
        }
    }

    fn check_anchor_ref(
        &mut self,
        r:      &AnchorRef,
        known:  &HashMap<String, Span>,
        shapes: &HashMap<&str, &ShapeExpr>,
    ) {
        if !known.contains_key(&r.part) {
            self.errors.push(MoxiError::UndefinedName {
                name: r.part.clone(), span: r.span,
            });
            return;
        }
        let Some(shape) = shapes.get(r.part.as_str()) else { return };
        match crate::anchors::resolve_anchor(shape, &r.anchor, &r.args) {
            Ok(_) => {}
            Err(crate::anchors::AnchorError::Undefined { anchor, valid, .. }) => {
                self.errors.push(MoxiError::UndefinedAnchor {
                    part:   r.part.clone(),
                    anchor,
                    valid:  valid.join(", "),
                    span:   r.span,
                });
            }
            Err(crate::anchors::AnchorError::BadArgs { anchor, message }) => {
                self.errors.push(MoxiError::BadAnchor {
                    part: r.part.clone(), anchor, message, span: r.span,
                });
            }
        }
    }

    // ── Helpers ────────────────────────────────────────────────────────────

    fn extract_str_prop(&self, props: &[Prop], key: &str) -> Option<String> {
        props.iter().find(|p| p.key == key).map(|p| self.expr_to_str(&p.value))
    }

    fn expr_to_str(&self, expr: &Expr) -> String {
        match expr {
            Expr::Ident(i) => i.name.clone(),
            Expr::Str(s)   => s.clone(),
            Expr::Int(n)   => n.to_string(),
            Expr::Float(f) => f.to_string(),
            _              => "<complex>".to_string(),
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────
//
// Integration-style: parse real Moxi source, resolve, and inspect the
// flattened output — the same path the compiler takes.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser as MoxiParser;

    fn resolve_src(src: &str) -> (ResolvedScene, Vec<MoxiError>) {
        let (tokens, lex_errors) = Lexer::new(src).tokenize();
        assert!(lex_errors.is_empty(), "lex errors: {lex_errors:?}");
        let (doc, parse_errors) = MoxiParser::new(tokens).parse();
        assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
        Resolver::new().resolve(doc)
    }

    const SRC: &str = r#"
atom BONE { color = ivory }
material Bone { color = ivory, voxel_atom = BONE }

entity Arm {
    part Humerus { shape = cylinder(height=9, radius=0.8), material = Bone }
    part Hand    { shape = sphere(radius=1.5), material = Bone }
    relation {
        Hand.top on Humerus.bottom
    }
    anchor socket = Humerus.top
    resolve voxel_size = 1.0
}

entity Body {
    part Torso    { shape = ellipsoid(rx=6, ry=8, rz=4), material = Bone }
    part RightArm { entity = Arm }
    part LeftArm  { entity = Arm }
    relation {
        RightArm.socket on Torso.east
        LeftArm symmetric_across Torso from=RightArm
    }
    resolve voxel_size = 1.0
}
"#;

    #[test]
    fn instances_flatten_with_prefixed_names() {
        let (scene, errors) = resolve_src(SRC);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let body = scene.entities.iter().find(|e| e.name == "Body").unwrap();
        let names: Vec<&str> = body.parts.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"Torso"));
        assert!(names.contains(&"RightArm.Humerus"));
        assert!(names.contains(&"RightArm.Hand"));
        assert!(names.contains(&"LeftArm.Humerus"));
        assert!(names.contains(&"LeftArm.Hand"));
        // Arm is a component now, not a world layer.
        assert!(scene.instanced.contains("Arm"));
    }

    #[test]
    fn socket_rewrites_to_root_part_anchor() {
        let (scene, errors) = resolve_src(SRC);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let body = scene.entities.iter().find(|e| e.name == "Body").unwrap();
        let found = body.relations.iter().any(|p| matches!(p,
            Placement::Align { subject, object, .. }
                if subject.part == "RightArm.Humerus" && subject.anchor == "top"
                && object.part == "Torso" && object.anchor == "east"));
        assert!(found, "RightArm.socket should rewrite to RightArm.Humerus.top on Torso.east");
    }

    #[test]
    fn mirrored_instance_expands_per_part_and_drops_internals() {
        let (scene, errors) = resolve_src(SRC);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let body = scene.entities.iter().find(|e| e.name == "Body").unwrap();

        // One Mirror per template part: Humerus + Hand.
        let mirrors = body.relations.iter()
            .filter(|p| matches!(p, Placement::Mirror { .. }))
            .count();
        assert_eq!(mirrors, 2);

        // LeftArm's internal chain is gone — its parts are placed by the
        // expanded mirrors, never by inherited internal relations.
        let leftarm_internal = body.relations.iter().any(|p| matches!(p,
            Placement::Align { subject, .. } if subject.part.starts_with("LeftArm.")));
        assert!(!leftarm_internal);
    }

    #[test]
    fn missing_export_lists_available_sockets() {
        let src = SRC.replace("RightArm.socket", "RightArm.shoulder");
        let (_, errors) = resolve_src(&src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::UndefinedAnchor { anchor, valid, .. }
                if anchor == "shoulder" && valid.contains("socket"))),
            "expected UndefinedAnchor listing 'socket', got: {errors:?}");
    }

    // ── Self-contained materials ──────────────────────────────────────

    /// A material with no `voxel_atom` synthesizes its own atom from its
    /// own color — the two-declaration form is no longer required.
    #[test]
    fn material_without_voxel_atom_synthesizes_its_own() {
        let src = r#"
material Bone { color = ivory }
entity E {
    part P { shape = sphere(radius=2), material = Bone }
    resolve voxel_size = 1.0
}
"#;
        let (scene, errors) = resolve_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(scene.atoms.len(), 1);
        assert_eq!(scene.atoms[0].color, "ivory");
        assert_eq!(scene.materials[0].atom_index, 0);
    }

    /// Declared atoms keep their indices when a synthesized atom is added
    /// — synthesis appends, never inserts. This is the property that makes
    /// voxel colors byte-identical across the change: `grid_to_scene`
    /// resolves color by index, so a shifted index is a silently wrong
    /// color rather than an error.
    #[test]
    fn synthesized_atoms_append_after_declared_ones() {
        let src = r#"
atom BONE { color = ivory }
material Bone  { color = ivory, voxel_atom = BONE }
material Blood { color = maroon }
atom LEAF { color = green }
material Leafy { color = green, voxel_atom = LEAF }
entity E {
    part P { shape = sphere(radius=2), material = Blood }
    resolve voxel_size = 1.0
}
"#;
        let (scene, errors) = resolve_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        // Declared atoms first, in declaration order, indices untouched.
        assert_eq!(scene.atoms[0].name, "BONE");
        assert_eq!(scene.atoms[1].name, "LEAF");
        let bone  = scene.materials.iter().find(|m| m.name == "Bone").unwrap();
        let leafy = scene.materials.iter().find(|m| m.name == "Leafy").unwrap();
        assert_eq!(bone.atom_index,  0);
        assert_eq!(leafy.atom_index, 1);

        // The synthesized one is appended, even though its material was
        // declared between the two atoms.
        let blood = scene.materials.iter().find(|m| m.name == "Blood").unwrap();
        assert_eq!(blood.atom_index, 2);
        assert_eq!(scene.atoms[2].color, "maroon");
    }

    /// An explicitly named atom that does not exist is still a hard error
    /// — synthesis is the fallback for ABSENCE, not for typos.
    #[test]
    fn unknown_voxel_atom_is_still_an_error() {
        let src = r#"
atom BONE { color = ivory }
material Bone { color = ivory, voxel_atom = BOEN }
"#;
        let (_, errors) = resolve_src(src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::UndefinedAtom { name, .. } if name == "BOEN")),
            "expected UndefinedAtom, got: {errors:?}");
    }

    /// A.2 — an un-exported compass name on an instance resolves against
    /// the assembly extents and rewrites to a point() on the root part.
    /// Arm assembly: Humerus (root, y ∈ [0,9], x ∈ [−0.8, 0.8]) + Hand
    /// (sphere r=1.5 mated under it, center (0, −1.5, 0)) ⇒ assembly box
    /// x ∈ [−1.5, 1.5], y ∈ [−3, 9]. Its `west` = (−1.5, 3, 0), normal −X.
    #[test]
    fn instance_compass_rewrites_to_root_point() {
        let src = SRC.replace("RightArm.socket on Torso.east",
                              "RightArm.west on Torso.east");
        let (scene, errors) = resolve_src(&src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let body = scene.entities.iter().find(|e| e.name == "Body").unwrap();

        let pl = body.relations.iter().find_map(|p| match p {
            Placement::Align { subject, object, .. }
                if object.part == "Torso" && object.anchor == "east" => Some(subject),
            _ => None,
        }).expect("the compass placement should survive rewriting");

        assert_eq!(pl.part, "RightArm.Humerus", "carried on the instance root");
        assert_eq!(pl.anchor, "point");
        let get = |k: &str| pl.args.iter().find(|a| a.key == k).map(|a| match &a.value {
            Expr::Float(f) => *f, Expr::Int(n) => *n as f64, _ => f64::NAN,
        }).unwrap();
        assert!((get("x") + 1.5).abs() < 1e-9, "west face x = −1.5");
        assert!((get("y") - 3.0).abs() < 1e-9, "assembly mid-height y = 3");
        assert!((get("nx") + 1.0).abs() < 1e-9, "outward normal −X");
    }

    /// A.2 end-to-end: the rewritten point() solves through the frame
    /// resolver — RightArm's west face lands ON Torso.east (x = 6), so
    /// the Humerus root sits at x = 7.5, and the assembly's mid-height
    /// (y = 3 locally) lands at the socket's y = 0 ⇒ root y = −3.
    #[test]
    fn instance_compass_solves_to_expected_frame() {
        use crate::frame_resolver::resolve_frames;
        let src = SRC.replace("RightArm.socket on Torso.east",
                              "RightArm.west on Torso.east");
        let (scene, errors) = resolve_src(&src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let body = scene.entities.iter().find(|e| e.name == "Body").unwrap();

        let parts: Vec<(String, ShapeExpr)> = body.parts.iter()
            .filter_map(|p| p.shape.clone().map(|s| (p.name.clone(), s)))
            .collect();
        let frames = resolve_frames(&parts, &body.relations).unwrap();

        let humerus = frames["RightArm.Humerus"];
        assert!((humerus.pos.x - 7.5).abs() < 1e-9, "root x: 6 + 1.5, got {}", humerus.pos.x);
        assert!((humerus.pos.y + 3.0).abs() < 1e-9, "root y: −3, got {}", humerus.pos.y);
    }

    // ── Phase C: entity parameters ────────────────────────────────────────

    const PARAM_SRC: &str = r#"
atom BONE { color = ivory }
material Bone { color = ivory, voxel_atom = BONE }

entity Arm(length=9, girth=0.8) {
    part Humerus { shape = cylinder(height=length, radius=girth), material = Bone }
    part Hand    { shape = sphere(radius=girth*2), material = Bone }
    relation {
        Hand.top on Humerus.bottom
    }
    anchor socket = Humerus.top
    resolve voxel_size = 1.0
}

entity Body {
    part Torso   { shape = ellipsoid(rx=6, ry=8, rz=4), material = Bone }
    part LongArm { entity = Arm(length=12) }
    part DefArm  { entity = Arm }
    relation {
        LongArm.socket on Torso.east
        DefArm.socket on Torso.west
    }
    resolve voxel_size = 1.0
}
"#;

    fn shape_arg(scene: &ResolvedScene, entity: &str, part: &str, key: &str) -> f64 {
        let e = scene.entities.iter().find(|e| e.name == entity).unwrap();
        let p = e.parts.iter().find(|p| p.name == part)
            .unwrap_or_else(|| panic!("no part {part}"));
        let args = match p.shape.as_ref().unwrap() {
            ShapeExpr::Cylinder { args } | ShapeExpr::Sphere { args } => args,
            other => panic!("unexpected shape {other:?}"),
        };
        match &args.iter().find(|a| a.key == key).unwrap().value {
            Expr::Float(f) => *f,
            Expr::Int(n)   => *n as f64,
            other          => panic!("arg {key} not folded to a constant: {other:?}"),
        }
    }

    /// Overrides re-substitute the template's raw shapes; defaults stay;
    /// arithmetic folds (`girth*2` → 1.6) — and the standalone Arm layer
    /// uses its own defaults.
    #[test]
    fn params_substitute_per_instance() {
        let (scene, errors) = resolve_src(PARAM_SRC);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        assert_eq!(shape_arg(&scene, "Body", "LongArm.Humerus", "height"), 12.0);
        assert_eq!(shape_arg(&scene, "Body", "DefArm.Humerus",  "height"), 9.0);
        assert_eq!(shape_arg(&scene, "Body", "LongArm.Hand",    "radius"), 1.6);
        assert_eq!(shape_arg(&scene, "Arm",  "Humerus",         "height"), 9.0);
    }

    /// Unknown parameter names error with the parameter vocabulary.
    #[test]
    fn unknown_param_is_an_instance_error() {
        let src = PARAM_SRC.replace("Arm(length=12)", "Arm(lenth=12)");
        let (_, errors) = resolve_src(&src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::InstanceError { message, .. }
                if message.contains("lenth") && message.contains("length"))),
            "expected unknown-parameter error, got: {errors:?}");
    }

    /// Compass anchors see each instance's actual size: a length=12 arm's
    /// assembly is taller than a length=9 one.
    #[test]
    fn instance_compass_reflects_parameters() {
        let src = PARAM_SRC
            .replace("LongArm.socket on Torso.east", "LongArm.top on Torso.east")
            .replace("DefArm.socket on Torso.west",  "DefArm.top on Torso.west");
        let (scene, errors) = resolve_src(&src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let body = scene.entities.iter().find(|e| e.name == "Body").unwrap();

        let top_y = |inst: &str| -> f64 {
            body.relations.iter().find_map(|p| match p {
                Placement::Align { subject, .. }
                    if subject.part.starts_with(inst) && subject.anchor == "point" =>
                {
                    subject.args.iter().find(|a| a.key == "y").map(|a| match &a.value {
                        Expr::Float(f) => *f, _ => f64::NAN,
                    })
                }
                _ => None,
            }).unwrap()
        };
        // Assembly top = humerus height (root at 0..h). 12 vs 9.
        assert!((top_y("LongArm.") - 12.0).abs() < 1e-9);
        assert!((top_y("DefArm.") - 9.0).abs() < 1e-9);
    }
}
