use std::collections::{HashMap, HashSet};

use crate::anchors::analytic_extents;
use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::frame::Vec3;
use crate::frame_resolver::resolve_frames;
use crate::value::{self, Value, BUILTIN_NAMES};

mod pose;
pub use pose::{apply_pose, PoseOverride, ResolvedPose};

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
    /// Named poses, folded to numbers, in declaration order. Apply with
    /// `apply_pose(&relations, pose)`.
    pub poses:       Vec<ResolvedPose>,
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
    params:    Vec<(String, Value)>,
    /// Phase D: `let` bindings, RAW, in order — re-evaluated per instance
    /// under that instance's parameters.
    lets:      Vec<(String, Expr)>,
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
    /// Poses: folded under the defaults, and raw (parameter names intact)
    /// for re-substitution when an instance overrides parameters.
    poses:         Vec<ResolvedPose>,
    raw_poses:     Vec<ResolvedPose>,
    /// Phase E3: the declaration as written, for re-resolution.
    decl:          EntityDecl,
    /// Phase E3: true when the thing's STRUCTURE depends on its
    /// parameters — it has loops, indexed names, or nested instances fed
    /// by its own parameters. Overriding such a thing's parameters
    /// re-resolves it from `decl` rather than substituting into parts
    /// that were flattened under the defaults.
    reresolve:     bool,
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
        Placement::Align { subject, object, twist, pitch, gap, offsets, span } => Placement::Align {
            subject: pre(subject),
            object:  pre(object),
            twist: twist.clone(), pitch: pitch.clone(), gap: gap.clone(), offsets: offsets.clone(),
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

/// Phase D: the environment is the value domain's, shared with the
/// generator. Parameters and `let` bindings both live in it.
type ParamEnv = value::Env;

fn eval_const(expr: &Expr, env: &ParamEnv) -> Option<Value> {
    value::eval(expr, env).ok()
}

/// Fold everything the env can evaluate; leave the rest structurally
/// intact for `check_no_free_idents` to report. A bare identifier that
/// is NOT in the env (e.g. `axis=z`) is left as-is — evaluation fails on
/// it, and the fallthrough clones it.
fn subst_expr(expr: &Expr, env: &ParamEnv) -> Expr {
    match value::eval(expr, env) {
        Ok(v) => match expr {
            // Literals are already folded; rewriting them is a no-op.
            Expr::Int(_) | Expr::Float(_) => expr.clone(),
            _ => v.to_expr(),
        },
        Err(_) => match expr {
            Expr::BinOp { op, lhs, rhs } => Expr::BinOp {
                op:  op.clone(),
                lhs: Box::new(subst_expr(lhs, env)),
                rhs: Box::new(subst_expr(rhs, env)),
            },
            Expr::If { cond, then, else_ } => Expr::If {
                cond:  Box::new(subst_expr(cond, env)),
                then:  Box::new(subst_expr(then, env)),
                else_: Box::new(subst_expr(else_, env)),
            },
            Expr::Not(inner) => Expr::Not(Box::new(subst_expr(inner, env))),
            Expr::List(items) => Expr::List(items.iter().map(|e| subst_expr(e, env)).collect()),
            // The variable stays free inside the body; fold around it.
            Expr::Comprehension { var, start, end, body } => {
                let mut inner = env.clone();
                inner.remove(&var.name);
                Expr::Comprehension {
                    var:   var.clone(),
                    start: Box::new(subst_expr(start, env)),
                    end:   Box::new(subst_expr(end, env)),
                    body:  Box::new(subst_expr(body, &inner)),
                }
            }
            Expr::Index { base, index } => Expr::Index {
                base:  Box::new(subst_expr(base, env)),
                index: Box::new(subst_expr(index, env)),
            },
            other => other.clone(),
        },
    }
}

/// Every VALUE argument in a shape tree, in order. The `axis` key of
/// `spin` and `mirror` is skipped: `z` there is a name by design, not an
/// expression.
fn visit_shape_args<'a>(shape: &'a ShapeExpr, f: &mut dyn FnMut(&'a Expr)) {
    use ShapeExpr as S;
    let mut args_of = |args: &'a [NamedArg], skip_axis: bool| {
        for a in args {
            if !(skip_axis && a.key == "axis") { f(&a.value); }
        }
    };
    match shape {
        S::Box_ { args } | S::Sphere { args } | S::Cylinder { args } | S::Cone { args }
        | S::Ellipsoid { args } | S::Blob { args } | S::Heightfield { args }
        | S::Capsule { args } | S::Torus { args } => args_of(args, false),
        S::Shell { inner, args } | S::At { inner, args } | S::Scale { inner, args } => {
            args_of(args, false);
            visit_shape_args(inner, f);
        }
        S::Spin { inner, args } | S::Mirror { inner, args } => {
            args_of(args, true);
            visit_shape_args(inner, f);
        }
        S::Extrude { profile, args } => {
            args_of(args, false);
            visit_shape_args(profile, f);
        }
        S::Union { shapes, args } => {
            args_of(args, false);
            for s in shapes { visit_shape_args(s, f); }
        }
        S::Intersect { shapes } => {
            for s in shapes { visit_shape_args(s, f); }
        }
        S::Difference { base, cuts } => {
            visit_shape_args(base, f);
            for s in cuts { visit_shape_args(s, f); }
        }
    }
}

/// Identifiers left in a shape's arguments after substitution.
fn collect_shape_idents(shape: &ShapeExpr, out: &mut Vec<Ident>) {
    visit_shape_args(shape, &mut |e| out.extend(value::idents(e)));
}

fn collect_arg_idents(args: &[NamedArg], out: &mut Vec<Ident>) {
    for a in args {
        out.extend(value::idents(&a.value));
    }
}

// ── Phase E3: walking a thing's items ─────────────────────────────────────
//
// One traversal over every item of a thing, parameterized by `f` for each
// EXPRESSION (shape, instance and anchor arguments; qualifiers; constraint
// bounds; lets; loop bounds) and `n` for each part NAME. Used to expand
// `fn` calls everywhere, to resolve indexed names at top level, and once
// per loop iteration to bind the loop variable. Argument KEYS are never
// mapped, and neither is `spin`'s `axis` — a loop variable named `z` must
// not rewrite `axis=z`.

fn walk_args(args: &[NamedArg], f: &mut dyn FnMut(&Expr) -> Expr) -> Vec<NamedArg> {
    args.iter().map(|a| NamedArg { key: a.key.clone(), value: f(&a.value) }).collect()
}

fn walk_shape(s: &ShapeExpr, f: &mut dyn FnMut(&Expr) -> Expr) -> ShapeExpr {
    use ShapeExpr as S;
    match s {
        S::Box_ { args }        => S::Box_ { args: walk_args(args, f) },
        S::Sphere { args }      => S::Sphere { args: walk_args(args, f) },
        S::Cylinder { args }    => S::Cylinder { args: walk_args(args, f) },
        S::Cone { args }        => S::Cone { args: walk_args(args, f) },
        S::Ellipsoid { args }   => S::Ellipsoid { args: walk_args(args, f) },
        S::Blob { args }        => S::Blob { args: walk_args(args, f) },
        S::Heightfield { args } => S::Heightfield { args: walk_args(args, f) },
        S::Capsule { args }     => S::Capsule { args: walk_args(args, f) },
        S::Torus { args }       => S::Torus { args: walk_args(args, f) },
        S::Shell { inner, args } => S::Shell {
            inner: Box::new(walk_shape(inner, f)), args: walk_args(args, f),
        },
        S::Extrude { profile, args } => S::Extrude {
            profile: Box::new(walk_shape(profile, f)), args: walk_args(args, f),
        },
        S::Union { shapes, args } => S::Union {
            shapes: shapes.iter().map(|x| walk_shape(x, f)).collect(),
            args:   walk_args(args, f),
        },
        S::Intersect { shapes } => S::Intersect {
            shapes: shapes.iter().map(|x| walk_shape(x, f)).collect(),
        },
        S::Difference { base, cuts } => S::Difference {
            base: Box::new(walk_shape(base, f)),
            cuts: cuts.iter().map(|x| walk_shape(x, f)).collect(),
        },
        S::At { inner, args } => S::At {
            inner: Box::new(walk_shape(inner, f)), args: walk_args(args, f),
        },
        S::Spin { inner, args } => S::Spin {
            inner: Box::new(walk_shape(inner, f)),
            args:  args.iter().map(|a| NamedArg {
                key:   a.key.clone(),
                value: if a.key == "axis" { a.value.clone() } else { f(&a.value) },
            }).collect(),
        },
        // Same rule as spin: a loop variable named `x` must not rewrite
        // `axis=x`.
        S::Mirror { inner, args } => S::Mirror {
            inner: Box::new(walk_shape(inner, f)),
            args:  args.iter().map(|a| NamedArg {
                key:   a.key.clone(),
                value: if a.key == "axis" { a.value.clone() } else { f(&a.value) },
            }).collect(),
        },
        S::Scale { inner, args } => S::Scale {
            inner: Box::new(walk_shape(inner, f)), args: walk_args(args, f),
        },
    }
}

fn walk_anchor_ref(
    r: &AnchorRef,
    f: &mut dyn FnMut(&Expr) -> Expr,
    n: &mut dyn FnMut(&str) -> String,
) -> AnchorRef {
    AnchorRef { part: n(&r.part), anchor: r.anchor.clone(), args: walk_args(&r.args, f), span: r.span }
}

fn walk_placement(
    p: &Placement,
    f: &mut dyn FnMut(&Expr) -> Expr,
    n: &mut dyn FnMut(&str) -> String,
) -> Placement {
    match p {
        Placement::Align { subject, object, twist, pitch, gap, offsets, span } => Placement::Align {
            subject: walk_anchor_ref(subject, f, n),
            object:  walk_anchor_ref(object, f, n),
            twist:   f(twist),
            pitch:   f(pitch),
            gap:     f(gap),
            offsets: Box::new(MateOffsets {
                shift: (f(&offsets.shift.0), f(&offsets.shift.1)),
                lean:  (f(&offsets.lean.0),  f(&offsets.lean.1)),
            }),
            span:    *span,
        },
        Placement::Mirror { subject, source, plane, axis, span } => Placement::Mirror {
            subject: n(subject),
            source:  n(source),
            plane:   walk_anchor_ref(plane, f, n),
            axis:    *axis,
            span:    *span,
        },
    }
}

fn walk_part(
    p: &PartDecl,
    f: &mut dyn FnMut(&Expr) -> Expr,
    n: &mut dyn FnMut(&str) -> String,
) -> PartDecl {
    PartDecl {
        name:        Ident { name: n(&p.name.name), span: p.name.span },
        shape:       p.shape.as_ref().map(|s| walk_shape(s, f)),
        entity:      p.entity.clone(),
        entity_args: walk_args(&p.entity_args, f),
        material:    p.material.clone(),
        span:        p.span,
    }
}

fn walk_constraint(
    c: &ConstraintStmt,
    f: &mut dyn FnMut(&Expr) -> Expr,
    n: &mut dyn FnMut(&str) -> String,
) -> ConstraintStmt {
    let expr = match &c.expr {
        ConstraintExpr::Relation(r) => ConstraintExpr::Relation(RelationStmt {
            subject:    Ident { name: n(&r.subject.name), span: r.subject.span },
            predicate:  r.predicate.clone(),
            object:     Ident { name: n(&r.object.name), span: r.object.span },
            qualifiers: r.qualifiers.clone(),
            span:       r.span,
        }),
        ConstraintExpr::Bound { name, op, value } => ConstraintExpr::Bound {
            name:  Ident { name: n(&name.name), span: name.span },
            op:    op.clone(),
            value: f(value),
        },
    };
    ConstraintStmt { expr, span: c.span }
}

fn walk_prop(p: &Prop, f: &mut dyn FnMut(&Expr) -> Expr) -> Prop {
    Prop { key: p.key.clone(), value: f(&p.value), span: p.span }
}

fn walk_for(
    b: &ForBlock,
    f: &mut dyn FnMut(&Expr) -> Expr,
    n: &mut dyn FnMut(&str) -> String,
) -> ForBlock {
    ForBlock {
        var:         b.var.clone(),
        start:       f(&b.start),
        end:         f(&b.end),
        lets:        b.lets.iter().map(|p| walk_prop(p, f)).collect(),
        parts:       b.parts.iter().map(|p| walk_part(p, f, n)).collect(),
        relations:   b.relations.iter().map(|p| walk_placement(p, f, n)).collect(),
        constraints: b.constraints.iter().map(|c| walk_constraint(c, f, n)).collect(),
        loops:       b.loops.iter().map(|l| walk_for(l, f, n)).collect(),
        span:        b.span,
    }
}

fn walk_entity_in_place(
    e: &mut EntityDecl,
    f: &mut dyn FnMut(&Expr) -> Expr,
    n: &mut dyn FnMut(&str) -> String,
) {
    e.params      = e.params.iter().map(|p| walk_prop(p, f)).collect();
    e.lets        = e.lets.iter().map(|p| walk_prop(p, f)).collect();
    e.parts       = e.parts.iter().map(|p| walk_part(p, f, n)).collect();
    e.relations   = e.relations.iter().map(|p| walk_placement(p, f, n)).collect();
    e.constraints = e.constraints.iter().map(|c| walk_constraint(c, f, n)).collect();
    e.anchors     = e.anchors.iter().map(|a| AnchorDecl {
        name: a.name.clone(), target: walk_anchor_ref(&a.target, f, n), span: a.span,
    }).collect();
    e.loops       = e.loops.iter().map(|b| walk_for(b, f, n)).collect();
    e.index_exprs = e.index_exprs.iter().map(f).collect();
}

// ── Phase E3: unrolling ───────────────────────────────────────────────────

/// Iterations one thing may unroll to, across all its loops. Totality
/// already holds — bounds are constants — this only keeps a typo like
/// `0..100000` from stalling compilation in a browser tab.
const MAX_ITERATIONS: usize = 4096;

struct Unrolled {
    parts:       Vec<PartDecl>,
    relations:   Vec<Placement>,
    constraints: Vec<ConstraintStmt>,
}

fn scope_refs(scope: &HashMap<String, Expr>) -> HashMap<&str, &Expr> {
    scope.iter().map(|(k, v)| (k.as_str(), v)).collect()
}

/// Fold an expression to a whole number: bind loop variables and loop
/// lets syntactically from `scope`, then evaluate under the thing's
/// parameters and lets.
fn fold_int(e: &Expr, scope: &HashMap<String, Expr>, env: &ParamEnv) -> Result<i64, String> {
    let refs = scope_refs(scope);
    match value::eval(&substitute_idents(e, &refs), env) {
        Ok(Value::Num(v)) if v.fract() == 0.0 => Ok(v as i64),
        Ok(Value::Num(v)) => Err(format!("must be a whole number, got {v}")),
        Ok(other)         => Err(format!("must be a number, got a {}", other.kind())),
        Err(err)          => Err(err.message),
    }
}

/// Replace each index marker `[#k]` in a name with its folded value:
/// `RibR[#0]` -> `RibR[3]`. Names without markers pass through unchanged.
fn resolve_markers(
    name:        &str,
    scope:       &HashMap<String, Expr>,
    env:         &ParamEnv,
    index_exprs: &[Expr],
    errs:        &mut Vec<MoxiError>,
    span:        Span,
) -> String {
    if !name.contains("[#") {
        return name.to_string();
    }
    let mut out = String::new();
    let mut rest = name;
    while let Some(i) = rest.find("[#") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let Some(close) = after.find(']') else { break };
        let slot = after[..close].parse::<usize>().ok().and_then(|k| index_exprs.get(k));
        match slot.map(|e| fold_int(e, scope, env)) {
            Some(Ok(v)) => out.push_str(&format!("[{v}]")),
            Some(Err(m)) => {
                errs.push(MoxiError::ExprError {
                    message: format!("the index in '{out}[…]': {m}"),
                    span,
                });
                out.push_str("[?]");
            }
            None => out.push_str("[?]"),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

fn check_loop_vars(b: &ForBlock, taken: &HashSet<&str>, thing: &str, errs: &mut Vec<MoxiError>) {
    if taken.contains(b.var.name.as_str()) {
        errs.push(MoxiError::ExprError {
            message: format!(
                "loop variable '{}' shadows a parameter or `let` of '{thing}' — pick another name",
                b.var.name),
            span: b.var.span,
        });
    }
    for inner in &b.loops {
        check_loop_vars(inner, taken, thing, errs);
    }
}

/// Emit one copy of the block's items per iteration. The loop variable and
/// the body's lets are bound SYNTACTICALLY — substituted as expressions,
/// not folded — so a body expression like `2.5 + 3.5 * sin(... / pairs)`
/// still depends on the thing's parameters afterwards.
fn unroll(
    block:       &ForBlock,
    outer:       &HashMap<String, Expr>,
    env:         &ParamEnv,
    index_exprs: &[Expr],
    budget:      &mut usize,
    out:         &mut Unrolled,
    errs:        &mut Vec<MoxiError>,
) {
    let var = &block.var.name;
    if outer.contains_key(var) {
        errs.push(MoxiError::ExprError {
            message: format!("loop variable '{var}' is already bound by an enclosing loop"),
            span: block.var.span,
        });
        return;
    }

    let start = match fold_int(&block.start, outer, env) {
        Ok(v) => v,
        Err(m) => {
            errs.push(MoxiError::ExprError {
                message: format!("the start of `for {var} in …`: {m}"), span: block.span,
            });
            return;
        }
    };
    let end = match fold_int(&block.end, outer, env) {
        Ok(v) => v,
        Err(m) => {
            errs.push(MoxiError::ExprError {
                message: format!("the end of `for {var} in …`: {m}"), span: block.span,
            });
            return;
        }
    };

    for k in start..end {
        if *budget == 0 {
            errs.push(MoxiError::ExprError {
                message: format!(
                    "loops in one thing may unroll to at most {MAX_ITERATIONS} iterations — \
                     reduce the counts or split the thing"),
                span: block.span,
            });
            return;
        }
        *budget -= 1;

        let mut scope = outer.clone();
        scope.insert(var.clone(), Expr::Int(k));
        for l in &block.lets {
            let v = substitute_idents(&l.value, &scope_refs(&scope));
            scope.insert(l.key.clone(), v);
        }

        {
            let refs = scope_refs(&scope);
            let mut f = |x: &Expr| substitute_idents(x, &refs);
            let mut n = |s: &str| resolve_markers(s, &scope, env, index_exprs, errs, block.span);
            for p in &block.parts       { out.parts.push(walk_part(p, &mut f, &mut n)); }
            for r in &block.relations   { out.relations.push(walk_placement(r, &mut f, &mut n)); }
            for c in &block.constraints { out.constraints.push(walk_constraint(c, &mut f, &mut n)); }
        }

        for inner in &block.loops {
            unroll(inner, &scope, env, index_exprs, budget, out, errs);
        }
    }
}

fn value_key(v: &Value) -> String {
    match v {
        Value::Num(n)  => format!("{n}"),
        Value::Bool(b) => format!("{b}"),
        Value::List(items) => format!(
            "[{}]", items.iter().map(value_key).collect::<Vec<_>>().join(", ")),
    }
}


fn substitute_idents(expr: &Expr, subst: &HashMap<&str, &Expr>) -> Expr {
    match expr {
        Expr::Ident(id) => subst.get(id.name.as_str()).map(|e| (*e).clone()).unwrap_or_else(|| expr.clone()),
        Expr::BinOp { op, lhs, rhs } => Expr::BinOp {
            op: op.clone(),
            lhs: Box::new(substitute_idents(lhs, subst)),
            rhs: Box::new(substitute_idents(rhs, subst)),
        },
        Expr::Not(e) => Expr::Not(Box::new(substitute_idents(e, subst))),
        Expr::If { cond, then, else_ } => Expr::If {
            cond:  Box::new(substitute_idents(cond, subst)),
            then:  Box::new(substitute_idents(then, subst)),
            else_: Box::new(substitute_idents(else_, subst)),
        },
        Expr::Call { name, args } => Expr::Call {
            name: name.clone(),
            args: args.iter().map(|a| NamedArg {
                key: a.key.clone(),
                value: substitute_idents(&a.value, subst),
            }).collect(),
        },
        Expr::List(items) => Expr::List(items.iter().map(|e| substitute_idents(e, subst)).collect()),
        // The comprehension variable is bound inside its body: an outer
        // binding of the same name (a loop variable, a fn parameter) must
        // not reach in. Bounds are outside the scope.
        Expr::Comprehension { var, start, end, body } => {
            let mut inner = subst.clone();
            inner.remove(var.name.as_str());
            Expr::Comprehension {
                var:   var.clone(),
                start: Box::new(substitute_idents(start, subst)),
                end:   Box::new(substitute_idents(end, subst)),
                body:  Box::new(substitute_idents(body, &inner)),
            }
        }
        Expr::Index { base, index } => Expr::Index {
            base:  Box::new(substitute_idents(base, subst)),
            index: Box::new(substitute_idents(index, subst)),
        },
        other => other.clone(),
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
        ShapeExpr::Capsule { args } => ShapeExpr::Capsule { args: subst_args(args, env) },
        ShapeExpr::Torus { args } => ShapeExpr::Torus { args: subst_args(args, env) },
        ShapeExpr::Shell { inner, args } => ShapeExpr::Shell {
            inner: Box::new(subst_shape(inner, env)), args: subst_args(args, env),
        },
        ShapeExpr::Extrude { profile, args } => ShapeExpr::Extrude {
            profile: Box::new(subst_shape(profile, env)), args: subst_args(args, env),
        },
        ShapeExpr::Union { shapes, args } => ShapeExpr::Union {
            shapes: shapes.iter().map(|x| subst_shape(x, env)).collect(),
            args:   subst_args(args, env),
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
        ShapeExpr::Mirror { inner, args } => ShapeExpr::Mirror {
            inner: Box::new(subst_shape(inner, env)), args: subst_args(args, env),
        },
        ShapeExpr::Scale { inner, args } => ShapeExpr::Scale {
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
        Placement::Align { subject, object, twist, pitch, gap, offsets, span } => Placement::Align {
            subject: subst_anchor_ref(subject, env),
            object:  subst_anchor_ref(object, env),
            twist: subst_expr(twist, env),
            pitch: subst_expr(pitch, env),
            gap:   subst_expr(gap, env),
            offsets: Box::new(MateOffsets {
                shift: (subst_expr(&offsets.shift.0, env), subst_expr(&offsets.shift.1, env)),
                lean:  (subst_expr(&offsets.lean.0,  env), subst_expr(&offsets.lean.1,  env)),
            }),
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
    /// Phase E2: declared pure functions, keyed by name.
    functions:       HashMap<String, FnDecl>,
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
            functions:       HashMap::new(),
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
                TopLevel::FnDecl(f)        => self.register_fn(f),
                _ => {}
            }
        }
        self.check_fn_call_graph_is_acyclic();

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
                TopLevel::GeneratorDecl(g) => {
                    self.check_generator(&g);
                }
                TopLevel::FnDecl(_) => {} // registered and graph-checked in pass 1
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
    fn resolve_entity(&mut self, mut e: EntityDecl) -> Option<ResolvedEntity> {
        // Phase E3: keep the declaration as written, for re-resolution
        // when an instance overrides a parameter the structure depends on.
        let decl_raw = e.clone();
        let reresolve = !e.loops.is_empty()
            || !e.index_exprs.is_empty()
            || e.parts.iter().any(|p| p.entity_args.iter()
                .any(|a| !value::idents(&a.value).is_empty()));

        // Phase E3: expand `fn` calls EVERYWHERE up front — shape and
        // instance arguments, qualifiers, loop bounds, indices — so every
        // later stage sees only builtins.
        {
            let mut f = |x: &Expr| self.expand_fn_calls(x);
            let mut n = |s: &str| s.to_string();
            walk_entity_in_place(&mut e, &mut f, &mut n);
        }
        {
            let mut poses = std::mem::take(&mut e.poses);
            for p in &mut poses {
                pose::walk_pose_exprs(p, &mut |x: &Expr| self.expand_fn_calls(x));
            }
            e.poses = poses;
        }

        // Phase C: evaluate parameter defaults (must be constants).
        let mut env_default: ParamEnv = HashMap::new();
        let mut params_vec: Vec<(String, Value)> = Vec::new();
        for p in &e.params {
            let p_expanded = self.expand_fn_calls(&p.value);
            match eval_const(&p_expanded, &env_default) {
                Some(v) => {
                    env_default.insert(p.key.clone(), v.clone());
                    params_vec.push((p.key.clone(), v));
                }
                None => {
                    self.errors.push(MoxiError::InstanceError {
                        instance: e.name.name.clone(),
                        message:  format!(
                            "parameter '{}' needs a constant default value", p.key),
                        span: p.span,
                    });
                    env_default.insert(p.key.clone(), Value::Num(0.0));
                    params_vec.push((p.key.clone(), Value::Num(0.0)));
                }
            }
        }

        // Phase D: `let` bindings, in order. Each sees the parameters and
        // every earlier let — never a later one. Kept raw on the template
        // so instances can re-evaluate them under their own parameters.
        let mut lets_raw: Vec<(String, Expr)> = Vec::new();
        for l in &e.lets {
            if env_default.contains_key(&l.key) {
                self.errors.push(MoxiError::DuplicateName {
                    name: l.key.clone(), span: l.span,
                });
                continue;
            }
            // Expand any `fn` calls to their bodies BEFORE folding — the
            // value evaluator only knows builtins, never declared
            // functions, by design (functions are resolver-level sugar,
            // not part of the runtime value language).
            let expanded = self.expand_fn_calls(&l.value);
            match value::eval(&expanded, &env_default) {
                Ok(v) => { env_default.insert(l.key.clone(), v); }
                Err(err) => {
                    self.errors.push(MoxiError::ExprError {
                        message: format!("in `let {}`: {}", l.key, err.message),
                        span:    l.span,
                    });
                    env_default.insert(l.key.clone(), Value::Num(0.0));
                }
            }
            lets_raw.push((l.key.clone(), expanded));
        }

        // ── Phase E3: loops and indexed names ─────────────────────────────
        // Unroll every `for` block into ordinary parts, relations and
        // constraints, and resolve indexed names (`RibR[i]` -> `RibR[3]`)
        // throughout the thing. After this block nothing downstream —
        // flattening, the frame solver, the IR — knows loops existed.
        let pose_index_exprs = e.index_exprs.clone();
        {
            let span = e.span;
            let index_exprs = std::mem::take(&mut e.index_exprs);
            let loops = std::mem::take(&mut e.loops);
            let mut errs: Vec<MoxiError> = Vec::new();
            let empty: HashMap<String, Expr> = HashMap::new();

            let taken: HashSet<&str> = params_vec.iter().map(|(n, _)| n.as_str())
                .chain(lets_raw.iter().map(|(n, _)| n.as_str()))
                .collect();
            for l in &loops {
                check_loop_vars(l, &taken, &e.name.name, &mut errs);
            }

            // Top-level items: an index here folds with no loop variable
            // bound — `RibR[0]`, `Vert[verts - 1]`.
            {
                let mut f = |x: &Expr| x.clone();
                let mut n = |s: &str| resolve_markers(s, &empty, &env_default, &index_exprs, &mut errs, span);
                walk_entity_in_place(&mut e, &mut f, &mut n);
            }

            let mut budget = MAX_ITERATIONS;
            let mut out = Unrolled {
                parts: Vec::new(), relations: Vec::new(), constraints: Vec::new(),
            };
            for l in &loops {
                unroll(l, &empty, &env_default, &index_exprs, &mut budget, &mut out, &mut errs);
            }
            e.parts.extend(out.parts);
            e.relations.extend(out.relations);
            e.constraints.extend(out.constraints);
            self.errors.extend(errs);
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
        // instance name → its thing's poses, prefixed and substituted
        let mut instance_poses: HashMap<String, Vec<ResolvedPose>> = HashMap::new();

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

                    // Phase E3: a thing whose structure depends on its
                    // parameters cannot be overridden by substituting into
                    // parts flattened under the defaults — `Row(n=5)` has
                    // more parts than `Row(n=2)`. Re-resolve it under the
                    // overrides instead, cached by their values.
                    let mut tmpl_key = tmpl_ident.name.clone();
                    let (tmpl, entity_args) = if tmpl.reresolve && !entity_args.is_empty() {
                        match self.specialize(&tmpl_ident, &tmpl, &entity_args, &env_default, &pname) {
                            Some((key, t)) => { tmpl_key = key; (t, Vec::new()) }
                            None => continue,
                        }
                    } else {
                        (tmpl, entity_args)
                    };

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
                        // Phase E4: an argument folds under THIS thing's
                        // parameters and lets, so `Rib(reach=reach)` passes
                        // a computed value down. (Was: constants only.)
                        match value::eval(&arg.value, &env_default) {
                            Ok(v) => { child_env.insert(arg.key.clone(), v); }
                            Err(err) => {
                                self.errors.push(MoxiError::InstanceError {
                                    instance: pname.clone(),
                                    message:  format!(
                                        "argument '{}' could not be evaluated: {}",
                                        arg.key, err.message),
                                    span: tmpl_ident.span,
                                });
                                bad_args = true;
                            }
                        }
                    }
                    // Phase D: the template's lets are re-evaluated under
                    // THIS instance's parameters, in order. A let that was
                    // fine on the defaults can fail here (say, a division
                    // by an overridden zero) — that is an instance error.
                    for (lname, lexpr) in &tmpl.lets {
                        let expanded = self.expand_fn_calls(lexpr);
                        match value::eval(&expanded, &child_env) {
                            Ok(v) => { child_env.insert(lname.clone(), v); }
                            Err(err) => {
                                self.errors.push(MoxiError::InstanceError {
                                    instance: pname.clone(),
                                    message:  format!("in `let {lname}`: {}", err.message),
                                    span:     tmpl_ident.span,
                                });
                                bad_args = true;
                                child_env.insert(lname.clone(), Value::Num(0.0));
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
                    let inst_poses: Vec<ResolvedPose> = if overridden {
                        tmpl.raw_poses.iter().map(|p| p.subst(&child_env).prefixed(&pname)).collect()
                    } else {
                        tmpl.poses.iter().map(|p| p.prefixed(&pname)).collect()
                    };
                    instance_poses.insert(pname.clone(), inst_poses);
                    // The specialized key (`Row(n=5)`), so mirroring checks
                    // that both sides have the same STRUCTURE, not just the
                    // same declared thing.
                    instance_of.insert(pname, tmpl_key);
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
        // For poses: this thing's own mates (subject as written → flattened
        // subject part) and mirrors (image as written → source).
        let mut own_align: Vec<(String, String)> = Vec::new();
        let mut own_mirror: HashMap<String, String> = HashMap::new();

        for pl in e.relations {
            match pl {
                Placement::Align { subject, object, twist, pitch, gap, offsets, span } => {
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

                    own_align.push((orig_part.clone(), subject.part.clone()));
                    rewritten.push(Placement::Align {
                        subject, object, twist, pitch, gap, offsets, span,
                    });
                }

                Placement::Mirror { subject, source, plane, axis, span } => {
                    own_mirror.insert(subject.clone(), source.clone());
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

        // Phase D: nothing unresolved may survive into geometry.
        self.check_no_free_idents(&parts, &relations, &env_default, e.span);

        // ── Poses ─────────────────────────────────────────────────────────
        let mut raw_poses: Vec<ResolvedPose> = Vec::new();
        let mut poses:     Vec<ResolvedPose> = Vec::new();
        {
            let mut errs: Vec<MoxiError> = Vec::new();
            pose::check_pose_names(&e.poses, &mut errs);
            let scope = pose::PoseScope {
                thing:          &e.name.name,
                own_align:      &own_align,
                own_mirror:     &own_mirror,
                part_names:     &part_names,
                instance_of:    &instance_of,
                instance_poses: &instance_poses,
            };
            for decl in &e.poses {
                if decl.name.name == "rest" { continue; }
                let lines = pose::unroll_pose(decl, &env_default, &pose_index_exprs, &mut errs);
                let raw = pose::resolve_lines(&decl.name.name, decl.span, lines, &scope, &mut errs);
                poses.push(pose::fold_pose(&raw, &env_default, &mut errs));
                raw_poses.push(raw);
            }
            self.errors.extend(errs);
        }

        // Register as a template for later entities to instance.
        self.templates.insert(e.name.name.clone(), EntityTemplate {
            params:        params_vec,
            lets:          lets_raw,
            parts:         parts.clone(),
            relations:     relations.clone(),
            exports,
            raw_shapes,
            raw_relations: raw_full,
            raw_exports,
            poses:         poses.clone(),
            raw_poses,
            decl:          decl_raw,
            reresolve,
        });

        Some(ResolvedEntity {
            name:        e.name.name,
            parts,
            relations,
            constraints: e.constraints,
            resolve:     e.resolve,
            poses,
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

    // ── Phase D: static value checks ──────────────────────────────────

    /// After substitution, any identifier still inside a shape argument or
    /// anchor argument is one the environment did not know. Before D these
    /// fell through to `arg_f64`'s default — `radius=lenth` silently drew
    /// a unit sphere. Now it is an error that lists what IS in scope.
    ///
    /// The one legitimate bare identifier is `spin(…, axis=z)`, skipped by
    /// argument key.
    fn check_no_free_idents(
        &mut self,
        parts:     &[ResolvedPart],
        relations: &[Placement],
        env:       &ParamEnv,
        span:      Span,
    ) {
        // An argument that survived folding with NO free identifier failed
        // to evaluate for another reason — an index out of range, a
        // division by zero, a list where a number was needed. Before this
        // check such an argument silently fell back to the shape's default
        // (rule 1: errors are API; a silent default is the opposite).
        for p in parts {
            if let Some(s) = &p.shape {
                let mut stuck: Vec<&Expr> = Vec::new();
                visit_shape_args(s, &mut |e| {
                    let literal = matches!(e, Expr::Int(_) | Expr::Float(_) | Expr::Ident(_));
                    if !literal && value::idents(e).is_empty() { stuck.push(e); }
                });
                for e in stuck {
                    if let Err(err) = value::eval(e, env) {
                        self.errors.push(MoxiError::ExprError {
                            message: format!("in part '{}': {}", p.name, err.message),
                            span,
                        });
                    }
                }
            }
        }

        let mut names: Vec<&str> = env.keys().map(|s| s.as_str()).collect();
        names.sort_unstable();
        let scope = if names.is_empty() { "nothing".to_string() } else { names.join(", ") };

        let mut found: Vec<Ident> = Vec::new();
        for p in parts {
            if let Some(s) = &p.shape {
                collect_shape_idents(s, &mut found);
            }
        }
        for r in relations {
            match r {
                Placement::Align { subject, object, twist, pitch, gap, offsets, .. } => {
                    collect_arg_idents(&subject.args, &mut found);
                    collect_arg_idents(&object.args, &mut found);
                    // Qualifiers are expressions now, so a typo in a pose
                    // parameter must be caught here too.
                    for e in [twist, pitch, gap, &offsets.shift.0, &offsets.shift.1,
                              &offsets.lean.0, &offsets.lean.1] {
                        found.extend(value::idents(e));
                    }
                }
                Placement::Mirror { plane, .. } => collect_arg_idents(&plane.args, &mut found),
            }
        }

        for id in found {
            self.errors.push(MoxiError::ExprError {
                message: format!("'{}' is not defined — in scope: {scope}", id.name),
                span:    id.span,
            });
        }
    }

    /// A generator `where` may name only the per-cell variables. Checked
    /// here so the error is static and spanned; the generator itself can
    /// then never meet an undefined name.
    fn check_generator(&mut self, g: &GeneratorDecl) {
        use crate::generator::WHERE_VARS;
        let Some(cond) = g.props.iter().find(|p| p.key == "where") else { return };
        for id in value::idents(&cond.value) {
            if !WHERE_VARS.contains(&id.name.as_str()) {
                self.errors.push(MoxiError::ExprError {
                    message: format!(
                        "'{}' is not defined in a generator `where` — available: {}",
                        id.name, WHERE_VARS.join(", ")),
                    span: id.span,
                });
            }
        }
    }

        // ── Phase E2: pure functions ─────────────────────────────────────────

    fn register_fn(&mut self, f: &FnDecl) {
        if BUILTIN_NAMES.contains(&f.name.name.as_str()) {
            self.errors.push(MoxiError::FnError {
                message: format!(
                    "'{}' is already a built-in function and cannot be redefined",
                    f.name.name
                ),
                span: f.name.span,
            });
            return;
        }
        if self.functions.insert(f.name.name.clone(), f.clone()).is_some() {
            self.errors.push(MoxiError::DuplicateName {
                name: f.name.name.clone(), span: f.name.span,
            });
        }
    }

    /// No `fn` may call itself, directly or through another `fn`, ever.
    /// Checked ONCE over the whole declared call graph — not a depth
    /// counter at call time — so the check is exact and the totality
    /// guarantee (rule 2) does not depend on catching a runaway at
    /// runtime. A call to something that ISN'T a declared fn (a builtin,
    /// or an unknown name) is not this check's concern; expansion reports
    /// unknown names on its own.
    fn check_fn_call_graph_is_acyclic(&mut self) {
        fn calls_of(body: &Expr, fns: &HashMap<String, FnDecl>, out: &mut Vec<String>) {
            match body {
                Expr::Call { name, args } => {
                    if fns.contains_key(name) { out.push(name.clone()); }
                    for a in args { calls_of(&a.value, fns, out); }
                }
                Expr::BinOp { lhs, rhs, .. } => { calls_of(lhs, fns, out); calls_of(rhs, fns, out); }
                Expr::Not(e) => calls_of(e, fns, out),
                Expr::If { cond, then, else_ } => {
                    calls_of(cond, fns, out); calls_of(then, fns, out); calls_of(else_, fns, out);
                }
                Expr::List(items) => for e in items { calls_of(e, fns, out); },
                Expr::Comprehension { start, end, body, .. } => {
                    calls_of(start, fns, out); calls_of(end, fns, out); calls_of(body, fns, out);
                }
                Expr::Index { base, index } => { calls_of(base, fns, out); calls_of(index, fns, out); }
                Expr::Int(_) | Expr::Float(_) | Expr::Str(_) | Expr::Ident(_) => {}
            }
        }

        let names: Vec<String> = self.functions.keys().cloned().collect();
        for start in &names {
            // DFS from `start`; if we ever reach `start` again, report the
            // cycle at the declaration that started the search.
            let mut stack = vec![start.clone()];
            let mut seen: HashSet<String> = HashSet::new();
            let mut chain: Vec<String> = Vec::new();
            while let Some(cur) = stack.pop() {
                if !seen.insert(cur.clone()) { continue; }
                chain.push(cur.clone());
                let Some(decl) = self.functions.get(&cur) else { continue };
                let mut callees = Vec::new();
                calls_of(&decl.body, &self.functions, &mut callees);
                for callee in callees {
                    if &callee == start {
                        let span = self.functions[start].span;
                        self.errors.push(MoxiError::FnError {
                            message: format!(
                                "'{start}' is recursive (through: {}) — functions must not call themselves, even indirectly",
                                chain.join(" -> ")
                            ),
                            span,
                        });
                        return; // one report per compile is enough
                    }
                    stack.push(callee);
                }
            }
        }
    }

    /// Substitute every call to a declared `fn` with its body, with
    /// parameters replaced by the caller's ARGUMENT EXPRESSIONS (not
    /// their folded values — the caller may still be inside a `for` body
    /// or another unresolved context). Recurses into the substituted body
    /// so a function that calls another function expands fully. Builtins
    /// and unknown names pass through untouched; `value::eval` reports
    /// unknown names when the result is finally folded.
    fn expand_fn_calls(&mut self, expr: &Expr) -> Expr {
        match expr {
            Expr::Call { name, args } => {
                let expanded_args: Vec<NamedArg> = args.iter()
                    .map(|a| NamedArg { key: a.key.clone(), value: self.expand_fn_calls(&a.value) })
                    .collect();

                let Some(decl) = self.functions.get(name).cloned() else {
                    return Expr::Call { name: name.clone(), args: expanded_args };
                };

                if expanded_args.len() != decl.params.len() {
                    self.errors.push(MoxiError::FnError {
                        message: format!(
                            "'{name}' takes {} argument{} ({}), got {}",
                            decl.params.len(),
                            if decl.params.len() == 1 { "" } else { "s" },
                            decl.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", "),
                            expanded_args.len(),
                        ),
                        span: decl.span,
                    });
                    return Expr::Int(0);
                }

                let subst: HashMap<&str, &Expr> = decl.params.iter()
                    .map(|p| p.name.as_str())
                    .zip(expanded_args.iter().map(|a| &a.value))
                    .collect();

                let substituted = substitute_idents(&decl.body, &subst);
                self.expand_fn_calls(&substituted)
            }
            Expr::BinOp { op, lhs, rhs } => Expr::BinOp {
                op: op.clone(),
                lhs: Box::new(self.expand_fn_calls(lhs)),
                rhs: Box::new(self.expand_fn_calls(rhs)),
            },
            Expr::Not(e) => Expr::Not(Box::new(self.expand_fn_calls(e))),
            Expr::If { cond, then, else_ } => Expr::If {
                cond:  Box::new(self.expand_fn_calls(cond)),
                then:  Box::new(self.expand_fn_calls(then)),
                else_: Box::new(self.expand_fn_calls(else_)),
            },
            Expr::List(items) => Expr::List(items.iter().map(|e| self.expand_fn_calls(e)).collect()),
            Expr::Comprehension { var, start, end, body } => Expr::Comprehension {
                var:   var.clone(),
                start: Box::new(self.expand_fn_calls(start)),
                end:   Box::new(self.expand_fn_calls(end)),
                body:  Box::new(self.expand_fn_calls(body)),
            },
            Expr::Index { base, index } => Expr::Index {
                base:  Box::new(self.expand_fn_calls(base)),
                index: Box::new(self.expand_fn_calls(index)),
            },
            other => other.clone(),
        }
    }

    /// Phase E3: resolve `tmpl` again with some parameter defaults
    /// replaced, under a key like `Row(n=5)`, and return that template.
    /// Cached: two instances with the same overrides share one resolution.
    /// Finite — a thing can only instance things declared before it.
    fn specialize(
        &mut self,
        tmpl_ident: &Ident,
        tmpl:       &EntityTemplate,
        args:       &[NamedArg],
        env:        &ParamEnv,
        instance:   &str,
    ) -> Option<(String, EntityTemplate)> {
        let mut values: Vec<(String, Value)> = Vec::new();
        for arg in args {
            if !tmpl.params.iter().any(|(n, _)| n == &arg.key) {
                let valid = if tmpl.params.is_empty() {
                    format!("thing '{}' takes no parameters", tmpl_ident.name)
                } else {
                    format!("parameters of '{}': {}", tmpl_ident.name,
                        tmpl.params.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", "))
                };
                self.errors.push(MoxiError::InstanceError {
                    instance: instance.to_string(),
                    message:  format!("unknown parameter '{}' — {valid}", arg.key),
                    span:     tmpl_ident.span,
                });
                return None;
            }
            match value::eval(&arg.value, env) {
                Ok(v) => values.push((arg.key.clone(), v)),
                Err(err) => {
                    self.errors.push(MoxiError::InstanceError {
                        instance: instance.to_string(),
                        message:  format!("argument '{}' could not be evaluated: {}", arg.key, err.message),
                        span:     tmpl_ident.span,
                    });
                    return None;
                }
            }
        }
        values.sort_by(|a, b| a.0.cmp(&b.0));

        let key = format!("{}({})", tmpl_ident.name,
            values.iter().map(|(k, v)| format!("{k}={}", value_key(v))).collect::<Vec<_>>().join(", "));

        if !self.templates.contains_key(&key) {
            let mut decl = tmpl.decl.clone();
            for p in &mut decl.params {
                if let Some((_, v)) = values.iter().find(|(k, _)| k == &p.key) {
                    p.value = v.to_expr();
                }
            }
            decl.name.name = key.clone();
            let _ = self.resolve_entity(decl);
        }
        self.templates.get(&key).cloned().map(|t| (key, t))
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
            ShapeExpr::Cylinder { args } | ShapeExpr::Sphere { args }
            | ShapeExpr::Capsule { args } | ShapeExpr::Box_ { args }
            | ShapeExpr::Cone { args } | ShapeExpr::Ellipsoid { args }
            | ShapeExpr::Torus { args } | ShapeExpr::Blob { args } => args,
            other => panic!("shape_arg reads primitive args only, got {other:?}"),
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

    // ── Phase D: values and bindings ──────────────────────────────────

    const LET_SRC: &str = r#"
material Steel { color = gray }

thing Gear(teeth=12, radius=6) {
    let pitch = 360 / teeth
    let rim   = if teeth > 10 { radius * 2 } else { radius }
    part Disc { shape = cylinder(height=2, radius=rim), material = Steel }
    resolve voxel_size = 1.0
}

thing Box {
    part Small { thing = Gear(teeth=6) }
    part Big   { thing = Gear }
    resolve voxel_size = 1.0
}
"#;

    /// `let` folds into shape arguments; `if` is an expression; and both
    /// are re-evaluated per instance when a parameter is overridden.
    #[test]
    fn lets_fold_and_follow_instance_parameters() {
        let (scene, errors) = resolve_src(LET_SRC);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");

        // Template defaults: teeth=12 > 10, so rim = radius*2 = 12.
        assert_eq!(shape_arg(&scene, "Gear", "Disc", "radius"), 12.0);
        // Override teeth=6: rim = radius = 6. The let followed the param.
        assert_eq!(shape_arg(&scene, "Box", "Small.Disc", "radius"), 6.0);
        assert_eq!(shape_arg(&scene, "Box", "Big.Disc",   "radius"), 12.0);
    }

    // ── Phase D.2: lists ─────────────────────────────────────────────

    /// A list is a table: a loop reads its element by index, and the
    /// table itself can be computed by a comprehension over a fn.
    #[test]
    fn lists_are_tables_for_loops() {
        let src = r#"
fn taper(i, n) = sin(180 * (i + 0.5) / n)
material M { color = red }
thing Rib(reach=6) {
    part Bone { shape = cylinder(height=reach, radius=0.4), material = M }
    resolve voxel_size = 1.0
}
thing Cage(pairs=4) {
    let reaches = [for i in 0..pairs { 2 + 6 * taper(i, pairs) }]
    let widths  = [1, 2, 3, 4]
    for i in 0..pairs {
        part R[i] { thing = Rib(reach=reaches[i]) }
        part W[i] { shape = sphere(radius=widths[i]), material = M }
    }
    resolve voxel_size = 1.0
}
"#;
        let (scene, errors) = resolve_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        // i=1, pairs=4: sin(180*1.5/4) = sin(67.5°) ≈ 0.92388 → 2 + 6·that ≈ 7.5433
        let r = shape_arg(&scene, "Cage", "R[1].Bone", "height");
        assert!((r - 7.5433).abs() < 1e-3, "got {r}");
        assert_eq!(shape_arg(&scene, "Cage", "W[3]", "radius"), 4.0);
    }

    /// A comprehension variable shadows a loop variable of the same name
    /// without either leaking into the other.
    #[test]
    fn comprehension_variable_is_scoped_from_the_loop_variable() {
        let src = r#"
material M { color = red }
thing T {
    for i in 0..2 {
        let xs = [for i in 0..3 { i * 10 }]
        part P[i] { shape = sphere(radius=xs[i] + 1), material = M }
    }
    resolve voxel_size = 1.0
}
"#;
        let (scene, errors) = resolve_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(shape_arg(&scene, "T", "P[0]", "radius"), 1.0);
        assert_eq!(shape_arg(&scene, "T", "P[1]", "radius"), 11.0);
    }

    #[test]
    fn a_list_can_be_an_instance_argument() {
        let src = r#"
material M { color = red }
thing Row(heights=[1, 2, 3]) {
    for i in 0..len(heights) {
        part P[i] { shape = cylinder(height=heights[i], radius=0.3), material = M }
    }
    resolve voxel_size = 1.0
}
thing Yard {
    part A { thing = Row }
    part B { thing = Row(heights=[5, 6]) }
    resolve voxel_size = 1.0
}
"#;
        let (scene, errors) = resolve_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(shape_arg(&scene, "Yard", "A.P[2]", "height"), 3.0);
        assert_eq!(shape_arg(&scene, "Yard", "B.P[1]", "height"), 6.0);
        let names: Vec<&str> = scene.entities.iter().find(|e| e.name == "Yard").unwrap()
            .parts.iter().map(|p| p.name.as_str()).collect();
        assert!(!names.contains(&"B.P[2]"), "the shorter table makes fewer parts");
    }

    #[test]
    fn a_bad_index_is_an_error_that_names_the_length() {
        let src = r#"
material M { color = red }
thing T {
    let xs = [1, 2, 3]
    part P { shape = sphere(radius=xs[3]), material = M }
    resolve voxel_size = 1.0
}
"#;
        let (_, errors) = resolve_src(src);
        assert!(errors.iter().any(|e| e.to_string().contains("out of range for a list of 3")),
            "got: {errors:?}");
    }

    /// Pre-D.2, an argument that could not fold for a reason other than a
    /// free name — `1 / 0`, a bad index — silently took the shape's default.
    #[test]
    fn an_argument_that_cannot_fold_is_an_error_not_a_default() {
        let src = r#"
material M { color = red }
thing T(d=0) {
    part P { shape = sphere(radius=1 / d), material = M }
    resolve voxel_size = 1.0
}
"#;
        let (_, errors) = resolve_src(src);
        assert!(errors.iter().any(|e| e.to_string().contains("division by zero")), "got: {errors:?}");
    }

    /// Before D, a typo in a shape argument fell through to the default
    /// and silently drew the wrong shape. Now it is an error that says
    /// what IS in scope — params and lets alike.
    #[test]
    fn undefined_name_in_shape_arg_lists_scope() {
        let src = LET_SRC.replace("radius=rim)", "radius=rmi)");
        let (_, errors) = resolve_src(&src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::ExprError { message, .. }
                if message.contains("'rmi' is not defined")
                && message.contains("pitch") && message.contains("radius") && message.contains("rim")
                && message.contains("teeth"))),
            "expected ExprError listing pitch/radius/rim/teeth, got: {errors:?}");
    }

    /// A `let` may only see what came before it.
    #[test]
    fn let_cannot_reference_a_later_let() {
        let src = LET_SRC.replace(
            "let pitch = 360 / teeth\n    let rim",
            "let pitch = 360 / teeth + late\n    let rim",
        ).replace(
            "part Disc",
            "let late = 1\n    part Disc",
        );
        let (_, errors) = resolve_src(&src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::ExprError { message, .. }
                if message.contains("in `let pitch`") && message.contains("'late' is not defined"))),
            "expected an in-`let pitch` error, got: {errors:?}");
    }

    /// `axis=z` in a spin is an identifier by design, not an undefined
    /// name — the one place a bare ident is a legitimate argument.
    #[test]
    fn spin_axis_ident_is_not_an_undefined_name() {
        let src = r#"
material M { color = red }
thing T {
    part P { shape = spin(cylinder(height=4, radius=1), axis=z, degrees=90), material = M }
    resolve voxel_size = 1.0
}
"#;
        let (_, errors) = resolve_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    }

    /// A generator `where` is checked statically against its vocabulary.
    #[test]
    fn generator_where_with_unknown_variable_is_static_error() {
        let src = r#"
material M { color = green }
thing Land { part Ground { shape = heightfield(radius=10, max_height=5), material = M } resolve voxel_size = 1.0 }
thing Tree { part Trunk { shape = cylinder(height=3, radius=0.5), material = M } resolve voxel_size = 1.0 }
generator Forest {
    scatter Tree
    count = 5, min_spacing = 2, seed = 1
    where = elevaton > 2
}
print Land detail=low
"#;
        let (_, errors) = resolve_src(src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::ExprError { message, .. }
                if message.contains("'elevaton'") && message.contains("elevation"))),
            "expected a static where-check error, got: {errors:?}");
    }

    // ── Phase E2: pure functions ────────────────────────────────────────

    /// A fn call folds inside a `let`, no instance-argument pass-through
    /// involved — that's a separate capability (E4), not this one. The
    /// shape argument itself references the `let` directly, which is
    /// already how every non-fn `let` reaches geometry.
    const FN_SRC: &str = r#"
material Bone { color = ivory }

fn taper(i, n) = sin(180 * (i + 0.5) / n)

thing Cage {
    let widest = taper(5, 12) * 8
    part Bone { shape = cylinder(height=widest, radius=0.4), material = Bone }
    resolve voxel_size = 1.0
}
"#;

    #[test]
    fn fn_calls_expand_and_fold_in_a_let() {
        let (scene, errors) = resolve_src(FN_SRC);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        // sin(180*5.5/12) ≈ sin(82.5°) ≈ 0.99144; *8 ≈ 7.9315
        let r = shape_arg(&scene, "Cage", "Bone", "height");
        assert!((r - 7.9315).abs() < 1e-3, "got {r}");
    }

    #[test]
    fn wrong_fn_arity_is_an_fn_error() {
        let src = FN_SRC.replace("taper(5, 12)", "taper(5)");
        let (_, errors) = resolve_src(&src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::FnError { message, .. }
                if message.contains("'taper' takes 2 arguments (i, n), got 1"))),
            "expected an FnError, got: {errors:?}");
    }

    #[test]
    fn direct_recursion_is_rejected() {
        let src = "fn bad(x) = bad(x) + 1\nmaterial M { color = red }\nthing T { part P { shape = sphere(radius=1) } resolve voxel_size = 1.0 }\n";
        let (_, errors) = resolve_src(src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::FnError { message, .. } if message.contains("recursive"))),
            "expected a recursion error, got: {errors:?}");
    }

    #[test]
    fn indirect_recursion_through_another_fn_is_rejected() {
        let src = "fn a(x) = b(x)\nfn b(x) = a(x) + 1\nmaterial M { color = red }\nthing T { part P { shape = sphere(radius=1) } resolve voxel_size = 1.0 }\n";
        let (_, errors) = resolve_src(src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::FnError { message, .. } if message.contains("recursive"))),
            "expected a recursion error, got: {errors:?}");
    }

    #[test]
    fn redefining_a_builtin_name_is_rejected() {
        let src = "fn sin(x) = x\nmaterial M { color = red }\nthing T { part P { shape = sphere(radius=1) } resolve voxel_size = 1.0 }\n";
        let (_, errors) = resolve_src(src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::FnError { message, .. } if message.contains("already a built-in"))),
            "expected an FnError, got: {errors:?}");
    }

    /// The ribcage case: a fn call whose arguments are themselves
    /// UNRESOLVED at the point of substitution — `i` is a stand-in for a
    /// future loop variable here, since Phase E's `for` doesn't exist yet.
    /// Substitution must be purely syntactic so this still expands
    /// correctly once `i` is bound by something else.
    #[test]
    fn fn_args_may_be_unresolved_identifiers() {
        let src = r#"
fn taper(i, n) = sin(180 * (i + 0.5) / n)

material Bone { color = ivory }

thing Rib(i=0, pairs=12) {
    let t = taper(i, pairs)
    part Bone { shape = cylinder(height=t*8, radius=0.4), material = Bone }
    resolve voxel_size = 1.0
}
"#;
        let (scene, errors) = resolve_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let r = shape_arg(&scene, "Rib", "Bone", "height");
        // i=0, pairs=12 default: sin(180*0.5/12) ≈ sin(7.5°) ≈ 0.1305; *8
        assert!((r - 1.0442).abs() < 1e-3, "got {r}");
    }

    // ── Phase E3/E4: loops, indices, specialization ───────────────────

    fn part_names(scene: &ResolvedScene, thing: &str) -> Vec<String> {
        scene.entities.iter().find(|e| e.name == thing).unwrap()
            .parts.iter().map(|p| p.name.clone()).collect()
    }

    const ROW_SRC: &str = r#"
material M { color = red }

thing Row(n=4) {
    part Base { shape = box(width=20, height=1, depth=2), material = M }
    for i in 0..n {
        let h = 2 + i
        part Post[i] { shape = cylinder(height=h, radius=0.3), material = M }
        relation { Post[i].bottom on Base.top shift=(i * 3, 0) }
    }
    resolve voxel_size = 1.0
}
"#;

    #[test]
    fn loops_unroll_into_indexed_parts() {
        let (scene, errors) = resolve_src(ROW_SRC);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let names = part_names(&scene, "Row");
        for i in 0..4 {
            assert!(names.contains(&format!("Post[{i}]")), "missing Post[{i}] in {names:?}");
        }
        assert_eq!(shape_arg(&scene, "Row", "Post[2]", "height"), 4.0, "loop let: 2 + i");
    }

    #[test]
    fn nested_loops_make_a_grid() {
        let src = r#"
material M { color = red }
thing Grid {
    part Base { shape = box(width=10, height=1, depth=10), material = M }
    for i in 0..2 {
        for j in 0..3 {
            part C[i][j] { shape = sphere(radius=0.4), material = M }
            relation { C[i][j].bottom on Base.top shift=(i * 2, j * 2) }
        }
    }
    resolve voxel_size = 1.0
}
"#;
        let (scene, errors) = resolve_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let names = part_names(&scene, "Grid");
        assert_eq!(names.iter().filter(|n| n.starts_with("C[")).count(), 6);
        assert!(names.contains(&"C[1][2]".to_string()));
    }

    /// The chaining idiom: first element outside, loop from 1, index
    /// arithmetic reaches the previous one.
    #[test]
    fn index_arithmetic_chains_elements() {
        let src = r#"
material M { color = red }
thing Stack(n=4) {
    part V[0] { shape = box(width=2, height=1, depth=2), material = M }
    for k in 1..n {
        part V[k] { shape = box(width=2, height=1, depth=2), material = M }
        relation { V[k].bottom on V[k-1].top }
    }
    resolve voxel_size = 1.0
}
"#;
        let (scene, errors) = resolve_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let stack = scene.entities.iter().find(|e| e.name == "Stack").unwrap();
        assert!(stack.relations.iter().any(|p| matches!(p,
            Placement::Align { subject, object, .. }
                if subject.part == "V[3]" && object.part == "V[2]")));
    }

    #[test]
    fn fns_and_builtins_work_inside_loop_bodies() {
        let src = r#"
fn taper(i, n) = sin(180 * (i + 0.5) / n)
material M { color = red }
thing Fan(n=6) {
    part Hub { shape = sphere(radius=1), material = M }
    for i in 0..n {
        part Blade[i] { shape = capsule(height=1 + 4 * taper(i, n), radius=0.2), material = M }
        relation { Blade[i].bottom on Hub.top shift=(i, 0) }
    }
    resolve voxel_size = 1.0
}
"#;
        let (scene, errors) = resolve_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        // i=2, n=6: sin(180*2.5/6) = sin(75°) ≈ 0.96593; 1 + 4*that ≈ 4.8637
        let h = shape_arg(&scene, "Fan", "Blade[2]", "height");
        assert!((h - 4.8637).abs() < 1e-3, "got {h}");
    }

    /// E4: an instance argument folds under the caller's lets.
    #[test]
    fn instance_args_fold_under_the_callers_env() {
        let src = r#"
material Bone { color = ivory }
thing Rib(reach=6) {
    part Bone { shape = cylinder(height=reach, radius=0.4), material = Bone }
    resolve voxel_size = 1.0
}
thing Cage {
    let widest = 4
    part R0 { thing = Rib(reach=widest * 2) }
    for i in 0..3 {
        part R[i] { thing = Rib(reach=2 + i) }
    }
    resolve voxel_size = 1.0
}
"#;
        let (scene, errors) = resolve_src(src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert_eq!(shape_arg(&scene, "Cage", "R0.Bone", "height"), 8.0);
        assert_eq!(shape_arg(&scene, "Cage", "R[2].Bone", "height"), 4.0);
    }

    /// A thing whose part COUNT depends on a parameter is re-resolved
    /// per override, not substituted into.
    #[test]
    fn structural_overrides_specialize_the_template() {
        let src = format!("{ROW_SRC}{}", r#"
thing Yard {
    part Short { thing = Row(n=2) }
    part Long  { thing = Row(n=5) }
    part Dflt  { thing = Row }
    resolve voxel_size = 1.0
}
"#);
        let (scene, errors) = resolve_src(&src);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        let names = part_names(&scene, "Yard");
        let count = |pre: &str| names.iter().filter(|n| n.starts_with(pre)).count();
        assert_eq!(count("Short.Post["), 2);
        assert_eq!(count("Long.Post["), 5);
        assert_eq!(count("Dflt.Post["), 4);
    }

    #[test]
    fn a_fractional_bound_is_an_error() {
        let src = ROW_SRC.replace("0..n", "0..2.5");
        let (_, errors) = resolve_src(&src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::ExprError { message, .. } if message.contains("whole number"))),
            "got: {errors:?}");
    }

    #[test]
    fn a_loop_variable_may_not_shadow_a_parameter() {
        let src = ROW_SRC.replace("for i in", "for n in").replace("Post[i]", "Post[n]")
            .replace("2 + i", "2").replace("i * 3", "0");
        let (_, errors) = resolve_src(&src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::ExprError { message, .. } if message.contains("shadows a parameter"))),
            "got: {errors:?}");
    }

    #[test]
    fn the_iteration_budget_is_enforced() {
        let src = ROW_SRC.replace("0..n", "0..5000");
        let (_, errors) = resolve_src(&src);
        assert!(errors.iter().any(|e| matches!(e,
            MoxiError::ExprError { message, .. } if message.contains("4096"))),
            "got: {errors:?}");
    }

}