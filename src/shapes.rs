//! Named shapes and shape loops (Form, stage 1).
//!
//! `shape Slab(w, turn) = spin(box(…), degrees=turn)` is a declaration;
//! `Slab(30, 12)` anywhere a shape may stand is a call; `for k in 0..9 { … }`
//! among the operands of `union`, `intersect` or `difference` adds one
//! operand per turn. All three are gone after `expand`, which runs first in
//! the resolver: nothing downstream (geometry, anchors, scene, backends)
//! ever sees a `Call` or a `Repeat`.
//!
//! A call substitutes its arguments for the parameters in the body, all at
//! once. The body sees only its own parameters and loop variables, never the
//! caller's names, so a shape means the same thing wherever it is called.
//! Loop bounds fold to whole numbers here, before any thing is resolved, so
//! they may use literals and the enclosing shape's parameters but not a
//! thing's own `param`s or `let`s. That keeps the language total: a loop
//! has at most `MAX_OPERANDS` turns.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::{MoxiError, Span};
use crate::resolver::substitute_idents;
use crate::value::{self, Value};

/// The most operands one loop (or all the loops of one combinator) may add.
pub const MAX_OPERANDS: usize = 4096;
/// Calls nested deeper than this are refused (a safety net: the declared
/// call graph is already checked for cycles).
const MAX_DEPTH: usize = 64;

type Bindings = HashMap<String, Expr>;

struct Expander<'a> {
    shapes: &'a HashMap<String, &'a ShapeDecl>,
    errors: Vec<MoxiError>,
}

/// Expand every named-shape call and shape loop in the document. Shape
/// declarations stay in place (nothing reads them afterwards).
pub fn expand(mut doc: Document) -> (Document, Vec<MoxiError>) {
    let mut errors = Vec::new();
    let mut decls: HashMap<String, &ShapeDecl> = HashMap::new();
    for item in &doc.items {
        if let TopLevel::ShapeDecl(d) = item {
            if decls.insert(d.name.name.clone(), d).is_some() {
                errors.push(MoxiError::ShapeError {
                    message: format!("shape '{}' is declared twice — each shape name once", d.name.name),
                    span: d.span,
                });
            }
        }
    }
    let uses_shapes = !decls.is_empty() || doc.items.iter().any(item_has_shape_syntax);
    if !uses_shapes { return (doc, errors); }

    check_acyclic(&decls, &mut errors);
    let mut names: Vec<&&ShapeDecl> = decls.values().collect();
    names.sort_by_key(|d| (d.span.line, d.span.col));
    for d in names { check_free_names(d, &mut errors); }
    if !errors.is_empty() { return (doc, errors); }

    let decls_owned: Vec<ShapeDecl> = decls.values().map(|d| (*d).clone()).collect();
    let map: HashMap<String, &ShapeDecl> = decls_owned.iter().map(|d| (d.name.name.clone(), d)).collect();
    let mut ex = Expander { shapes: &map, errors: Vec::new() };
    for item in &mut doc.items {
        match item {
            TopLevel::EntityDecl(e) => {
                ex.parts(&mut e.parts);
                ex.loops(&mut e.loops);
            }
            TopLevel::WorldDecl(w) => {
                if let Some(t) = &mut w.terrain {
                    t.base = ex.shape(&t.base, &Bindings::new(), 0);
                }
            }
            _ => {}
        }
    }
    errors.extend(ex.errors);
    (doc, errors)
}

/// Cheap pre-check: does any part carry a call or a loop? (So documents
/// that use neither are returned untouched, byte for byte.)
fn item_has_shape_syntax(item: &TopLevel) -> bool {
    fn parts(ps: &[PartDecl]) -> bool {
        ps.iter().any(|p| p.shape.as_ref().is_some_and(has_syntax))
    }
    fn loops(ls: &[ForBlock]) -> bool {
        ls.iter().any(|l| parts(&l.parts) || loops(&l.loops))
    }
    match item {
        TopLevel::EntityDecl(e) => parts(&e.parts) || loops(&e.loops),
        TopLevel::WorldDecl(w) => w.terrain.as_ref().is_some_and(|t| has_syntax(&t.base)),
        _ => false,
    }
}

fn has_syntax(s: &ShapeExpr) -> bool {
    children(s).any(has_syntax) || matches!(s, ShapeExpr::Call { .. } | ShapeExpr::Repeat { .. })
}

/// The shape children of a node (operands, wrapped shapes), loops' bodies
/// included.
fn children(s: &ShapeExpr) -> Box<dyn Iterator<Item = &ShapeExpr> + '_> {
    match s {
        ShapeExpr::Shell { inner, .. } | ShapeExpr::At { inner, .. } | ShapeExpr::Spin { inner, .. }
        | ShapeExpr::Mirror { inner, .. } | ShapeExpr::Scale { inner, .. } => Box::new(std::iter::once(&**inner)),
        ShapeExpr::Extrude { profile, .. } => Box::new(std::iter::once(&**profile)),
        ShapeExpr::Union { shapes, .. } | ShapeExpr::Intersect { shapes } => Box::new(shapes.iter()),
        ShapeExpr::Difference { base, cuts } => Box::new(std::iter::once(&**base).chain(cuts.iter())),
        ShapeExpr::Repeat { body, .. } => Box::new(std::iter::once(&**body)),
        _ => Box::new(std::iter::empty()),
    }
}

fn calls_in(s: &ShapeExpr, out: &mut Vec<String>) {
    if let ShapeExpr::Call { name, .. } = s { out.push(name.name.clone()); }
    for c in children(s) { calls_in(c, out); }
}

/// A shape may not call itself, even through others.
fn check_acyclic(decls: &HashMap<String, &ShapeDecl>, errors: &mut Vec<MoxiError>) {
    let mut names: Vec<&String> = decls.keys().collect();
    names.sort();
    for start in names {
        // depth-first, remembering the path so the report can name the cycle
        let mut path: Vec<String> = vec![start.clone()];
        let mut seen: HashSet<String> = HashSet::new();
        fn walk(cur: &str, start: &str, decls: &HashMap<String, &ShapeDecl>,
                path: &mut Vec<String>, seen: &mut HashSet<String>) -> bool {
            let Some(d) = decls.get(cur) else { return false };
            let mut callees = Vec::new();
            calls_in(&d.body, &mut callees);
            for c in callees {
                if c == start { path.push(c); return true; }
                if decls.contains_key(&c) && seen.insert(c.clone()) {
                    path.push(c.clone());
                    if walk(&c, start, decls, path, seen) { return true; }
                    path.pop();
                }
            }
            false
        }
        if walk(start, start, decls, &mut path, &mut seen) {
            errors.push(MoxiError::ShapeError {
                message: format!(
                    "shape '{start}' is recursive ({}) — a shape must not call itself, even through others",
                    path.join(" -> ")),
                span: decls[start].span,
            });
            return; // one report per compile is enough
        }
    }
}

/// A shape body may name only its own parameters and the variables of the
/// loops around the name — never the caller's names. Without this check a
/// free `q` would be bound later by whichever thing calls the shape.
fn check_free_names(d: &ShapeDecl, errors: &mut Vec<MoxiError>) {
    fn exprs<'a>(s: &'a ShapeExpr, scope: &mut Vec<String>, out: &mut Vec<(Ident, Vec<String>)>) {
        let take = |e: &Expr, scope: &[String], out: &mut Vec<(Ident, Vec<String>)>| {
            for id in value::idents(e) {
                if !scope.contains(&id.name) { out.push((id, scope.to_vec())); }
            }
        };
        let args = |a: &'a [NamedArg], skip_axis: bool| a.iter().filter(move |x| !(skip_axis && x.key == "axis"));
        match s {
            ShapeExpr::Spin { args: a, .. } | ShapeExpr::Mirror { args: a, .. } =>
                for x in args(a, true) { take(&x.value, scope, out) },
            ShapeExpr::Call { args: a, .. } => for e in a { take(e, scope, out) },
            ShapeExpr::Repeat { var, start, end, body } => {
                take(start, scope, out);
                take(end, scope, out);
                scope.push(var.name.clone());
                exprs(body, scope, out);
                scope.pop();
                return;
            }
            ShapeExpr::Box_ { args: a } | ShapeExpr::Sphere { args: a } | ShapeExpr::Cylinder { args: a }
            | ShapeExpr::Cone { args: a } | ShapeExpr::Ellipsoid { args: a } | ShapeExpr::Blob { args: a }
            | ShapeExpr::Heightfield { args: a } | ShapeExpr::Capsule { args: a } | ShapeExpr::Torus { args: a }
            | ShapeExpr::Shell { args: a, .. } | ShapeExpr::Extrude { args: a, .. } | ShapeExpr::Union { args: a, .. }
            | ShapeExpr::At { args: a, .. } | ShapeExpr::Scale { args: a, .. } =>
                for x in args(a, false) { take(&x.value, scope, out) },
            ShapeExpr::Intersect { .. } | ShapeExpr::Difference { .. } => {}
        }
        for c in children(s) { exprs(c, scope, out); }
    }
    let mut scope: Vec<String> = d.params.iter().map(|p| p.name.clone()).collect();
    let mut found = Vec::new();
    exprs(&d.body, &mut scope, &mut found);
    let params: Vec<&str> = d.params.iter().map(|p| p.name.as_str()).collect();
    let mut seen: HashSet<String> = HashSet::new();
    for (id, in_scope) in found {
        if !seen.insert(id.name.clone()) { continue; }
        let names = if in_scope.is_empty() { "nothing — it has no parameters".to_string() } else { in_scope.join(", ") };
        errors.push(MoxiError::ShapeError {
            message: format!(
                "shape '{}' uses '{}', which is not one of its parameters — in scope: {names}. \
                 A shape sees only its own parameters ({}); pass the value in: `shape {}({}, {}) = …`",
                d.name.name, id.name, if params.is_empty() { "none".to_string() } else { params.join(", ") },
                d.name.name, params.join(", "), id.name).replace("(, ", "("),
            span: id.span,
        });
    }
}

impl Expander<'_> {
    fn parts(&mut self, parts: &mut [PartDecl]) {
        for p in parts {
            if let Some(sh) = &p.shape {
                p.shape = Some(self.shape(sh, &Bindings::new(), 0));
            }
        }
    }

    fn loops(&mut self, loops: &mut [ForBlock]) {
        for l in loops {
            self.parts(&mut l.parts);
            self.loops(&mut l.loops);
        }
    }

    fn err(&mut self, message: String, span: Span) {
        self.errors.push(MoxiError::ShapeError { message, span });
    }

    fn expr(e: &Expr, env: &Bindings) -> Expr {
        if env.is_empty() { return e.clone(); }
        let refs: HashMap<&str, &Expr> = env.iter().map(|(k, v)| (k.as_str(), v)).collect();
        substitute_idents(e, &refs)
    }

    fn args(args: &[NamedArg], env: &Bindings) -> Vec<NamedArg> {
        args.iter().map(|a| NamedArg { key: a.key.clone(), value: Self::expr(&a.value, env) }).collect()
    }

    /// `spin` and `mirror`: `axis=y` is a word, never a name to substitute —
    /// a parameter or loop variable called `y` must not rewrite it.
    fn axis_args(args: &[NamedArg], env: &Bindings) -> Vec<NamedArg> {
        args.iter().map(|a| NamedArg {
            key:   a.key.clone(),
            value: if a.key == "axis" { a.value.clone() } else { Self::expr(&a.value, env) },
        }).collect()
    }

    /// Fold a loop bound to a whole number, or say why not.
    fn bound(&mut self, e: &Expr, env: &Bindings, what: &str, span: Span) -> Option<i64> {
        let folded = Self::expr(e, env);
        match value::eval(&folded, &value::Env::new()) {
            Ok(Value::Num(v)) if v.fract() == 0.0 => Some(v as i64),
            Ok(Value::Num(v)) => {
                self.err(format!("the {what} of a shape loop must be a whole number, got {v}"), span);
                None
            }
            Ok(other) => {
                self.err(format!("the {what} of a shape loop must be a number, got a {}", other.kind()), span);
                None
            }
            Err(err) => {
                self.err(format!(
                    "the {what} of a shape loop must be known where the loop is written: {} \
                     (use numbers, or parameters of the enclosing `shape`; a thing's own parameters \
                     are not available inside shape loops)", err.message), span);
                None
            }
        }
    }

    /// Expand one operand list: a `for` contributes one operand per turn.
    fn operands(&mut self, ops: &[ShapeExpr], env: &Bindings, depth: usize) -> Vec<ShapeExpr> {
        let mut out = Vec::new();
        for op in ops {
            match op {
                ShapeExpr::Repeat { var, start, end, body } => {
                    let (Some(a), Some(b)) = (
                        self.bound(start, env, "start", var.span),
                        self.bound(end, env, "end", var.span),
                    ) else { continue };
                    let turns = (b - a).max(0) as usize;
                    if out.len() + turns > MAX_OPERANDS {
                        self.err(format!(
                            "`for {} in {a}..{b}` adds {turns} operands; a shape holds at most {MAX_OPERANDS}",
                            var.name), var.span);
                        continue;
                    }
                    for i in a..b {
                        let mut inner = env.clone();
                        inner.insert(var.name.clone(), Expr::Int(i));
                        out.extend(self.operands(std::slice::from_ref(body), &inner, depth));
                    }
                }
                other => out.push(self.shape(other, env, depth)),
            }
        }
        out
    }

    fn shape(&mut self, s: &ShapeExpr, env: &Bindings, depth: usize) -> ShapeExpr {
        use ShapeExpr::*;
        let b = |ex: &mut Self, inner: &ShapeExpr| Box::new(ex.shape(inner, env, depth));
        match s {
            Box_ { args }        => Box_ { args: Self::args(args, env) },
            Sphere { args }      => Sphere { args: Self::args(args, env) },
            Cylinder { args }    => Cylinder { args: Self::args(args, env) },
            Cone { args }        => Cone { args: Self::args(args, env) },
            Ellipsoid { args }   => Ellipsoid { args: Self::args(args, env) },
            Blob { args }        => Blob { args: Self::args(args, env) },
            Heightfield { args } => Heightfield { args: Self::args(args, env) },
            Capsule { args }     => Capsule { args: Self::args(args, env) },
            Torus { args }       => Torus { args: Self::args(args, env) },
            Shell { inner, args }  => Shell { inner: b(self, inner), args: Self::args(args, env) },
            Extrude { profile, args } => Extrude { profile: b(self, profile), args: Self::args(args, env) },
            At { inner, args }     => At { inner: b(self, inner), args: Self::args(args, env) },
            Spin { inner, args }   => Spin { inner: b(self, inner), args: Self::axis_args(args, env) },
            Mirror { inner, args } => Mirror { inner: b(self, inner), args: Self::axis_args(args, env) },
            Scale { inner, args }  => Scale { inner: b(self, inner), args: Self::args(args, env) },
            Union { shapes, args } => {
                let ops = self.operands(shapes, env, depth);
                Union { shapes: ops, args: Self::args(args, env) }
            }
            Intersect { shapes } => Intersect { shapes: self.operands(shapes, env, depth) },
            Difference { base, cuts } => {
                if let Repeat { var, .. } = &**base {
                    self.err("the base of a `difference` must be one shape, not a `for` loop — \
                              loops may stand among the cuts".to_string(), var.span);
                }
                Difference { base: b(self, base), cuts: self.operands(cuts, env, depth) }
            }
            Repeat { var, .. } => {
                self.err(format!(
                    "`for {} in …` is only allowed among the operands of union, intersect or difference",
                    var.name), var.span);
                Union { shapes: vec![], args: vec![] }
            }
            Call { name, args } => self.call(name, args, env, depth),
        }
    }

    fn call(&mut self, name: &Ident, args: &[Expr], env: &Bindings, depth: usize) -> ShapeExpr {
        let empty = ShapeExpr::Union { shapes: vec![], args: vec![] };
        let Some(decl) = self.shapes.get(&name.name).copied() else {
            let mut declared: Vec<&str> = self.shapes.keys().map(String::as_str).collect();
            declared.sort();
            let list = if declared.is_empty() { "none declared".to_string() } else { declared.join(", ") };
            self.err(format!(
                "unknown shape '{}' — declared shapes: {list} (the built-ins are box, sphere, cylinder, cone, \
                 ellipsoid, blob, heightfield, capsule, torus, shell, extrude, union, intersect, difference, \
                 at, spin, mirror, scale)", name.name), name.span);
            return empty;
        };
        if args.len() != decl.params.len() {
            let ps: Vec<&str> = decl.params.iter().map(|p| p.name.as_str()).collect();
            self.err(format!("shape '{}' takes {} argument{} ({}), got {}",
                name.name, ps.len(), if ps.len() == 1 { "" } else { "s" }, ps.join(", "), args.len()), name.span);
            return empty;
        }
        if depth >= MAX_DEPTH {
            self.err(format!("shape '{}' is nested more than {MAX_DEPTH} calls deep", name.name), name.span);
            return empty;
        }
        // Arguments are evaluated in the CALLER's scope; the body sees only
        // its own parameters.
        let bound: Bindings = decl.params.iter().zip(args)
            .map(|(p, a)| (p.name.clone(), Self::expr(a, env)))
            .collect();
        self.shape(&decl.body, &bound, depth + 1)
    }
}

#[cfg(test)]
mod tests {
    use crate::pipeline::compile_to_scene;
    use crate::scene::{Scene, Shape};

    /// A moxi fence around a script body, with one material.
    fn fence(body: &str) -> String {
        format!("```moxi\nmaterial M {{ color = \"#888888\" }}\n{body}\n```\n")
    }
    fn scene(body: &str) -> Scene {
        compile_to_scene(fence(body).as_str()).unwrap_or_else(|e| panic!("{e:?}"))
    }
    fn errors(body: &str) -> String {
        let es = compile_to_scene(fence(body).as_str()).expect_err("an error");
        es.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n")
    }
    fn operands(s: &Scene, part: &str) -> usize {
        let p = s.layers[0].parts.iter().find(|p| p.name == part).expect("part");
        match &p.shape { Shape::Union { shapes, .. } | Shape::Intersect { shapes } => shapes.len(), _ => panic!("not a union") }
    }

    const CRAG: &str = include_str!("../scripts/CRAG.md");

    #[test]
    fn a_shape_call_substitutes_its_arguments() {
        let s = scene("shape Ball(r) = sphere(radius=r * 2)\nthing T { part A { shape = Ball(1.5), material = M }\n  resolve voxel_size = 1.0 }\nprint T");
        let a = &s.layers[0].parts[0];
        assert!(matches!(a.shape, Shape::Sphere { radius } if (radius - 3.0).abs() < 1e-9), "{:?}", a.shape);
    }

    #[test]
    fn shapes_call_shapes_and_see_only_their_own_names() {
        // `r` in Outer is not visible inside Inner: Inner's own `r` is bound by the call
        let s = scene("shape Inner(r) = sphere(radius=r)\nshape Outer(r) = at(Inner(r + 1), y=r)\n\
                       thing T { part A { shape = Outer(2), material = M }\n resolve voxel_size = 1.0 }\nprint T");
        let Shape::At { inner, y, .. } = &s.layers[0].parts[0].shape else { panic!("{:?}", s.layers[0].parts[0].shape) };
        assert!((y - 2.0).abs() < 1e-9);
        assert!(matches!(**inner, Shape::Sphere { radius } if (radius - 3.0).abs() < 1e-9));
    }

    #[test]
    fn a_loop_among_operands_adds_one_operand_per_turn() {
        let s = scene("thing T { part A { shape = union(box(width=1,height=1,depth=1), for k in 0..5 { at(sphere(radius=1), x=k) }), material = M }\n\
                       resolve voxel_size = 1.0 }\nprint T");
        assert_eq!(operands(&s, "A"), 6, "one box and five spheres");
        let s = scene("thing T { part A { shape = intersect(for k in 2..5 { sphere(radius=k) }), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert_eq!(operands(&s, "A"), 3);
        let s = scene("thing T { part A { shape = difference(box(width=9,height=9,depth=9), for k in 0..3 { at(sphere(radius=1), x=k) }), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        let Shape::Difference { cuts, .. } = &s.layers[0].parts[0].shape else { panic!("difference") };
        assert_eq!(cuts.len(), 3);
    }

    #[test]
    fn loop_bounds_may_be_shape_parameters_and_loops_nest() {
        let s = scene("shape Row(n) = union(for i in 0..n { at(sphere(radius=1), x=3 * i) })\n\
                       thing T { part A { shape = union(for j in 0..3 { at(Row(4), z=5 * j) }), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert_eq!(operands(&s, "A"), 3);
        let Shape::Union { shapes, .. } = &s.layers[0].parts[0].shape else { panic!() };
        let Shape::At { inner, .. } = &shapes[0] else { panic!() };
        assert!(matches!(&**inner, Shape::Union { shapes, .. } if shapes.len() == 4));
    }

    #[test]
    fn an_empty_loop_adds_nothing() {
        let s = scene("thing T { part A { shape = union(sphere(radius=1), for k in 3..3 { sphere(radius=2) }), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert_eq!(operands(&s, "A"), 1);
    }

    #[test]
    fn an_unknown_shape_lists_the_declared_ones() {
        let e = errors("shape Slab(w) = box(width=w, height=1, depth=1)\nshape Rock(r) = sphere(radius=r)\n\
                        thing T { part A { shape = Slabb(3), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert!(e.contains("unknown shape 'Slabb'") && e.contains("Rock, Slab"), "{e}");
        let e = errors("thing T { part A { shape = Slab(3), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert!(e.contains("none declared"), "{e}");
    }

    #[test]
    fn a_recursive_shape_names_the_cycle() {
        let e = errors("shape A(r) = union(sphere(radius=r), B(r))\nshape B(r) = at(A(r), y=1)\n\
                        thing T { part P { shape = A(1), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert!(e.contains("recursive") && e.contains("A -> B -> A"), "{e}");
        let e = errors("shape S(r) = union(S(r))\nthing T { part P { shape = box(width=1,height=1,depth=1), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert!(e.contains("'S' is recursive"), "{e}");
    }

    #[test]
    fn arity_bounds_and_size_errors_say_what_is_wrong() {
        let e = errors("shape Slab(w, turn) = box(width=w, height=turn, depth=1)\nthing T { part A { shape = Slab(1), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert!(e.contains("takes 2 arguments (w, turn), got 1"), "{e}");
        let e = errors("thing T(n=3) { part A { shape = union(for k in 0..n { sphere(radius=1) }), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert!(e.contains("must be known where the loop is written"), "{e}");
        let e = errors("thing T { part A { shape = union(for k in 0..2.5 { sphere(radius=1) }), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert!(e.contains("whole number"), "{e}");
        let e = errors("thing T { part A { shape = union(for k in 0..5000 { sphere(radius=1) }), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert!(e.contains("at most 4096"), "{e}");
        let e = errors("thing T { part A { shape = difference(for k in 0..2 { sphere(radius=1) }, sphere(radius=1)), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert!(e.contains("base of a `difference`"), "{e}");
        let e = errors("shape Ball(r) = sphere(radius=q * r)\nthing T(q=2) { part A { shape = Ball(1), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert!(e.contains("shape 'Ball' uses 'q', which is not one of its parameters — in scope: r"), "{e}");
        assert!(e.contains("`shape Ball(r, q) = …`"), "{e}");
        // spin's axis is a word, not a name; loop variables are in scope inside their loop
        let s = scene("shape Turn(y) = spin(box(width=4, height=1, depth=1), axis=y, degrees=y)\n\
                       thing T { part A { shape = union(for x in 0..2 { mirror(Turn(45), axis=x) }), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        let Shape::Union { shapes, .. } = &s.layers[0].parts[0].shape else { panic!() };
        let Shape::Mirror { inner, nx, .. } = &shapes[1] else { panic!("{:?}", shapes[1]) };
        assert_eq!(*nx, 1.0, "axis=x stayed the x axis though a loop variable is called x");
        let Shape::Spin { axis, degrees, .. } = &**inner else { panic!("{inner:?}") };
        assert_eq!((format!("{axis:?}").to_lowercase().contains('y'), *degrees), (true, 45.0), "axis=y stayed y; degrees=y took 45");
        scene("shape Fan(n) = union(for k in 0..n { spin(box(width=4, height=1, depth=1), axis=y, degrees=30 * k) })\n\
               thing T { part A { shape = Fan(3), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        let e = errors("shape A(r) = sphere(radius=r)\nshape A(r) = sphere(radius=r)\nthing T { part P { shape = A(1), material = M }\nresolve voxel_size = 1.0 }\nprint T");
        assert!(e.contains("declared twice"), "{e}");
    }

    #[test]
    fn rand_is_a_fixed_number_in_the_unit_interval() {
        use crate::ast::{Expr, NamedArg};
        use crate::value::{eval, Env, Value};
        let call = |args: &[f64]| Expr::Call {
            name: "rand".into(),
            args: args.iter().map(|v| NamedArg { key: String::new(), value: Expr::Float(*v) }).collect(),
        };
        let r = |args: &[f64]| match eval(&call(args), &Env::new()).expect("rand") { Value::Num(v) => v, _ => panic!() };
        assert_eq!(r(&[7.0, 3.0]), r(&[7.0, 3.0]), "same arguments, same number");
        assert_ne!(r(&[7.0, 3.0]), r(&[7.0, 4.0]));
        assert_ne!(r(&[7.0]), r(&[7.0, 0.0]), "the count of arguments counts");
        assert_eq!(r(&[0.0]), r(&[-0.0]));
        let xs: Vec<f64> = (0..1000).map(|i| r(&[1.0, i as f64])).collect();
        assert!(xs.iter().all(|v| (0.0..1.0).contains(v)));
        let mean = xs.iter().sum::<f64>() / 1000.0;
        assert!((mean - 0.5).abs() < 0.05, "roughly uniform, mean {mean}");
        // pinned: a change to the hash reshuffles every script that uses `rand`
        assert!((r(&[7.0, 3.0]) - 0.837254873402).abs() < 1e-11, "the hash is part of the language");
        assert!((r(&[1.0]) - 0.730875267271).abs() < 1e-11);
        for bad in [&[][..], &[1.0, 2.0, 3.0, 4.0, 5.0][..]] {
            let e = eval(&call(bad), &Env::new()).unwrap_err().message;
            assert!(e.contains("takes 1 to 4 numbers"), "{e}");
        }
    }

    #[test]
    fn origin_is_the_frames_own_origin_and_ignores_at() {
        use crate::anchors::{resolve_anchor, AnchorKind};
        use crate::ast::{Expr, NamedArg, ShapeExpr};
        let boxed = ShapeExpr::Box_ { args: vec![] };
        let moved = ShapeExpr::At {
            inner: Box::new(boxed),
            args:  vec![NamedArg { key: "x".into(), value: Expr::Float(5.0) }],
        };
        let a = resolve_anchor(&moved, "origin", &[]).expect("origin");
        assert_eq!((a.frame.pos.x, a.frame.pos.y, a.frame.pos.z), (0.0, 0.0, 0.0), "not moved by at");
        assert!(matches!(a.kind, AnchorKind::Free), "no direction");
        let c = resolve_anchor(&moved, "center", &[]).expect("center");
        assert!((c.frame.pos.x - 5.0).abs() < 1e-9, "while center does move");
    }

    #[test]
    fn two_parts_share_a_frame_through_origin() {
        let s = scene("thing T {\n part A { shape = at(sphere(radius=1), x=9), material = M }\n part B { shape = at(sphere(radius=1), y=4), material = M }\n\
                       relation { B.origin on A.origin }\n resolve voxel_size = 1.0 }\nprint T");
        let f = |n: &str| s.layers[0].parts.iter().find(|p| p.name == n).unwrap().frame.pos;
        assert_eq!(f("A"), f("B"), "same frame: the shapes keep their own offsets inside it");
    }

    #[test]
    fn origin_carries_rotation_and_works_on_instances() {
        // A is tipped by its mate; B on A.origin takes A's whole frame, rotation included
        let s = scene("thing T {\n part Base { shape = box(width=9,height=1,depth=9), material = M }\n\
                       part A { shape = cylinder(height=4, radius=1), material = M }\n part B { shape = at(sphere(radius=1), y=3), material = M }\n\
                       relation { A.bottom on Base.top lean=(30, 0)\n B.origin on A.origin }\n resolve voxel_size = 1.0 }\nprint T");
        let p = |n: &str| s.layers[0].parts.iter().find(|p| p.name == n).unwrap().frame.clone();
        let (a, b) = (p("A"), p("B"));
        assert_eq!(format!("{:?}", a.pos), format!("{:?}", b.pos));
        assert_eq!(format!("{:?}", a.rot), format!("{:?}", b.rot), "same rotation: one frame");
        assert_ne!(format!("{:?}", a.rot), format!("{:?}", p("Base").rot), "and A really is tipped");
        // `Inst.origin` is the instance's own origin: its root part's frame
        let s = scene("thing Leg { part Bone { shape = cylinder(height=6, radius=1), material = M }\n resolve voxel_size = 1.0 }\n\
                       thing T {\n part Hip { shape = at(sphere(radius=2), x=5), material = M }\n part L { thing = Leg }\n\
                       relation { L.origin on Hip.origin }\n resolve voxel_size = 1.0 }\nprint T");
        let p = |n: &str| s.layers[0].parts.iter().find(|p| p.name == n).unwrap().frame.pos;
        assert_eq!(p("L.Bone"), p("Hip"));
        // an instance marked guide: all of its parts are guides
        let s = scene("thing Leg { part Bone { shape = cylinder(height=6, radius=1), material = M }\n resolve voxel_size = 1.0 }\n\
                       thing T {\n part Hip { shape = sphere(radius=2), material = M }\n part L { thing = Leg, guide }\n\
                       relation { L.Bone.bottom on Hip.top }\n resolve voxel_size = 1.0 }\nprint T");
        assert!(s.layers[0].parts.iter().any(|p| p.name == "L.Bone" && p.guide));
        assert!(s.at_rest().layers[0].parts.iter().all(|p| p.name != "L.Bone"));
    }

    #[test]
    fn a_guide_is_solved_referable_and_never_drawn() {
        let src = "thing T {\n part A { shape = box(width=4,height=2,depth=4), material = M }\n part G { shape = box(width=2,height=2,depth=2), guide }\n\
                   part C { shape = sphere(radius=1), material = M }\n relation { G.bottom on A.top\n C.bottom on G.top }\n resolve voxel_size = 1.0 }\nprint T";
        let s = scene(src);
        let g = s.layers[0].parts.iter().find(|p| p.name == "G").expect("a guide is in the scene data");
        assert!(g.guide);
        let c = s.layers[0].parts.iter().find(|p| p.name == "C").unwrap();
        assert!((c.frame.pos[1] - 4.0).abs() < 1e-9, "C stands on the guide's top (A -1..1, G 1..3, C's centre at 3 + 1); got {}", c.frame.pos[1]);
        assert!(s.at_rest().layers[0].parts.iter().all(|p| p.name != "G"), "no drawing backend sees it");
        let glb = crate::gltf::scene_to_gltf(&s, None);
        let names = glb.json["nodes"].as_array().unwrap().iter()
            .filter_map(|n| n["extras"]["moxi"]["part"].as_str()).collect::<Vec<_>>();
        assert!(names.contains(&"A") && !names.contains(&"G"), "{names:?}");
        let r = crate::report::from_source(fence(src).as_str(), None).expect("report");
        let text = r.to_text(false);
        assert!(text.contains("G (guide)"), "{text}");
        assert!(r.things[0].parts.iter().all(|p| p.name != "G"), "not a row, not a contact, not in a piece");
        // voxel output draws only A and C
        let w = crate::pipeline::compile_source(fence(src).as_str()).expect("voxels");
        assert_eq!(w.layers[0].parts, 2, "A and C");
    }

    #[test]
    fn the_crag_builds_from_one_shape_and_one_loop() {
        let s = compile_to_scene(CRAG).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(operands(&s, "Body"), 9);
        assert!(s.layers[0].parts.iter().any(|p| p.name == "Summit" && p.guide));
        // the seed changes the rock and nothing else about the script
        let other = CRAG.replace("seed=7", "seed=8");
        let t = compile_to_scene(other.as_str()).unwrap();
        assert_ne!(format!("{:?}", s.layers[0].parts[0].shape), format!("{:?}", t.layers[0].parts[0].shape));
    }

    #[test]
    fn documents_without_shapes_are_returned_untouched() {
        let (doc, errs) = super::expand(crate::ast::Document { items: vec![] });
        assert!(errs.is_empty() && doc.items.is_empty());
    }
}
