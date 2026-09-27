//! Phase D — the value domain and the ONE expression evaluator.
//!
//! Two consumers, one semantics: the resolver folds parameters and `let`
//! bindings at resolve time; the generator evaluates `where` per terrain
//! cell. Before D the generator had its own private interpreter with its
//! own quirks (unknown names silently allowed, `==` with a tolerance);
//! now there is one, and its errors name what IS in scope.
//!
//! Totality: nothing here loops or recurses on anything but the
//! expression tree, which is finite by construction.

use std::collections::HashMap;

use crate::ast::{BinOp, Expr, Ident, NamedArg};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(f64),
    Bool(bool),
    /// Phase D.2. A point is a list of three numbers; a path is a list of
    /// points. There is no separate vector type.
    List(Vec<Value>),
}

/// Elements one comprehension may produce. Totality already holds — the
/// bounds are constants — this keeps a typo from eating memory.
pub const MAX_LIST_LEN: usize = 4096;

impl Value {
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Num(_)  => "number",
            Value::Bool(_) => "boolean",
            Value::List(_) => "list",
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self { Value::List(v) => Some(v), _ => None }
    }

    pub fn as_num(&self) -> Option<f64> {
        match self { Value::Num(n) => Some(*n), _ => None }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self { Value::Bool(b) => Some(*b), _ => None }
    }

    /// A value back into the AST — how substitution writes a folded
    /// result into a shape argument. Booleans become 0/1 so that
    /// `point(free=1)`-style integer flags keep working.
    pub fn to_expr(&self) -> Expr {
        match self {
            Value::Num(n)  => Expr::Float(*n),
            Value::Bool(b) => Expr::Int(*b as i64),
            Value::List(v) => Expr::List(v.iter().map(Value::to_expr).collect()),
        }
    }
}

pub type Env = HashMap<String, Value>;

/// An evaluation failure. The message is the public API: it is what the
/// model reads, so it always says what was expected and what is in scope.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalError {
    pub message: String,
}

fn scope_list(env: &Env) -> String {
    let mut names: Vec<&str> = env.keys().map(|s| s.as_str()).collect();
    names.sort_unstable();
    if names.is_empty() { "nothing".to_string() } else { names.join(", ") }
}

fn undefined(name: &str, env: &Env) -> EvalError {
    EvalError { message: format!("'{name}' is not defined — in scope: {}", scope_list(env)) }
}

fn expect_num(what: &str, v: &Value) -> Result<f64, EvalError> {
    v.as_num().ok_or_else(|| EvalError {
        message: format!("{what} needs a number, got a {}", v.kind()),
    })
}

fn expect_bool(what: &str, v: &Value) -> Result<bool, EvalError> {
    v.as_bool().ok_or_else(|| EvalError {
        message: format!("{what} needs a boolean (a comparison, `and`/`or`/`not`), got a {}", v.kind()),
    })
}

/// A whole number, for bounds and indices.
fn expect_int(what: &str, v: &Value) -> Result<i64, EvalError> {
    let n = expect_num(what, v)?;
    if n.fract() != 0.0 {
        return Err(EvalError { message: format!("{what} needs a whole number, got {n}") });
    }
    Ok(n as i64)
}

pub fn eval(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
    match expr {
        Expr::Int(n)   => Ok(Value::Num(*n as f64)),
        Expr::Float(f) => Ok(Value::Num(*f)),
        Expr::Ident(i) => env.get(&i.name).cloned().ok_or_else(|| undefined(&i.name, env)),

        Expr::Not(inner) => {
            let b = expect_bool("`not`", &eval(inner, env)?)?;
            Ok(Value::Bool(!b))
        }

        // Only the taken branch is evaluated, so `if d > 0 { n / d } else
        // { 0 }` is legal even when d is zero.
        Expr::If { cond, then, else_ } => {
            let c = expect_bool("`if` condition", &eval(cond, env)?)?;
            if c { eval(then, env) } else { eval(else_, env) }
        }

        Expr::BinOp { op, lhs, rhs } => {
            let l = eval(lhs, env)?;
            let r = eval(rhs, env)?;
            match op {
                BinOp::And => Ok(Value::Bool(expect_bool("`and`", &l)? && expect_bool("`and`", &r)?)),
                BinOp::Or  => Ok(Value::Bool(expect_bool("`or`", &l)?  || expect_bool("`or`", &r)?)),

                BinOp::Add => Ok(Value::Num(expect_num("`+`", &l)? + expect_num("`+`", &r)?)),
                BinOp::Sub => Ok(Value::Num(expect_num("`-`", &l)? - expect_num("`-`", &r)?)),
                BinOp::Mul => Ok(Value::Num(expect_num("`*`", &l)? * expect_num("`*`", &r)?)),
                BinOp::Div => {
                    let (a, b) = (expect_num("`/`", &l)?, expect_num("`/`", &r)?);
                    if b == 0.0 {
                        return Err(EvalError { message: "division by zero".to_string() });
                    }
                    Ok(Value::Num(a / b))
                }

                BinOp::Lt   => Ok(Value::Bool(expect_num("`<`",  &l)? <  expect_num("`<`",  &r)?)),
                BinOp::Gt   => Ok(Value::Bool(expect_num("`>`",  &l)? >  expect_num("`>`",  &r)?)),
                BinOp::LtEq => Ok(Value::Bool(expect_num("`<=`", &l)? <= expect_num("`<=`", &r)?)),
                BinOp::GtEq => Ok(Value::Bool(expect_num("`>=`", &l)? >= expect_num("`>=`", &r)?)),

                BinOp::Eq | BinOp::Neq => {
                    // Lists compare element-wise, same kinds only.
                    let same = match (&l, &r) {
                        (Value::Num(a),  Value::Num(b))  => a == b,
                        (Value::Bool(a), Value::Bool(b)) => a == b,
                        (Value::List(_), Value::List(_)) => l == r,
                        _ => return Err(EvalError {
                            message: format!("cannot compare a {} with a {}", l.kind(), r.kind()),
                        }),
                    };
                    Ok(Value::Bool(if *op == BinOp::Eq { same } else { !same }))
                }
            }
        }

        Expr::Str(_) => Err(EvalError { message: "strings are not values yet".to_string() }),

        // ── Phase D.2: lists ─────────────────────────────────────────────
        Expr::List(items) => {
            let vals: Result<Vec<Value>, EvalError> = items.iter().map(|e| eval(e, env)).collect();
            Ok(Value::List(vals?))
        }

        // `[for i in a..b { body }]`: the bounds fold first; the body is
        // evaluated once per whole number in the half-open range with the
        // variable bound. Shadowing an outer name is allowed and scoped.
        Expr::Comprehension { var, start, end, body } => {
            let a = expect_int("the start of the `for` range", &eval(start, env)?)?;
            let b = expect_int("the end of the `for` range", &eval(end, env)?)?;
            let count = (b - a).max(0) as usize;
            if count > MAX_LIST_LEN {
                return Err(EvalError { message: format!(
                    "a list may hold at most {MAX_LIST_LEN} elements; `[for {} in {a}..{b} …]` would make {count}",
                    var.name) });
            }
            let mut out = Vec::with_capacity(count);
            let mut scope = env.clone();
            for k in a..b {
                scope.insert(var.name.clone(), Value::Num(k as f64));
                out.push(eval(body, &scope)?);
            }
            Ok(Value::List(out))
        }

        // `xs[i]`: 0-based; a bad index says how long the list is.
        Expr::Index { base, index } => {
            let list = eval(base, env)?;
            let Some(items) = list.as_list() else {
                return Err(EvalError { message: format!(
                    "only a list can be indexed with `[…]`, got a {}", list.kind()) });
            };
            let i = expect_int("a list index", &eval(index, env)?)?;
            if i < 0 || i as usize >= items.len() {
                return Err(EvalError { message: format!(
                    "index {i} is out of range for a list of {} (valid: 0..{})",
                    items.len(), items.len()) });
            }
            Ok(items[i as usize].clone())
        }

        Expr::Call { name, args } => call_builtin(name, args, env),
    }
}

// ── Math builtins (Phase E1) ────────────────────────────────────────────
//
// Positional, evaluated left to right: `sin(90)`, `clamp(x, 0, 1)`. Angles
// are DEGREES, matching every other angle in the language (pitch, twist,
// yaw, angle) — a function that silently expected radians would be
// exactly the kind of mismatch that compiles clean and produces wrong
// geometry, which rule 1 exists to prevent.
//
// The vocabulary list here is duplicated by BUILTIN_NAMES in spec.rs for
// the generated reference; keep the two in step, same as
// generator::WHERE_VARS mirrors its own check.

pub const BUILTIN_NAMES: &[&str] = &[
    "sin", "cos", "tan", "sqrt", "abs", "floor", "round", "pow",
    "min", "max", "clamp", "lerp", "len",
];

fn call_builtin(name: &str, args: &[NamedArg], env: &Env) -> Result<Value, EvalError> {
    let vals: Result<Vec<Value>, EvalError> =
        args.iter().map(|a| eval(&a.value, env)).collect();
    let vals = vals?;

    let arity_err = |want: usize| EvalError {
        message: format!(
            "'{name}' takes {want} argument{}, got {}",
            if want == 1 { "" } else { "s" }, vals.len()
        ),
    };
    let num = |v: &Value, what: &str| expect_num_pub(what, v);
    let one = || -> Result<f64, EvalError> {
        if vals.len() != 1 { return Err(arity_err(1)); }
        num(&vals[0], name)
    };
    let two = || -> Result<(f64, f64), EvalError> {
        if vals.len() != 2 { return Err(arity_err(2)); }
        Ok((num(&vals[0], name)?, num(&vals[1], name)?))
    };

    match name {
        // Phase D.2
        "len" => {
            if vals.len() != 1 { return Err(arity_err(1)); }
            match vals[0].as_list() {
                Some(items) => Ok(Value::Num(items.len() as f64)),
                None => Err(EvalError { message: format!(
                    "'len' needs a list, got a {}", vals[0].kind()) }),
            }
        }
        "sin"   => Ok(Value::Num(one()?.to_radians().sin())),
        "cos"   => Ok(Value::Num(one()?.to_radians().cos())),
        "tan"   => Ok(Value::Num(one()?.to_radians().tan())),
        "sqrt"  => {
            let x = one()?;
            if x < 0.0 {
                return Err(EvalError { message: format!("sqrt of a negative number ({x})") });
            }
            Ok(Value::Num(x.sqrt()))
        }
        "abs"   => Ok(Value::Num(one()?.abs())),
        "floor" => Ok(Value::Num(one()?.floor())),
        "round" => Ok(Value::Num(one()?.round())),
        "pow"   => { let (a, b) = two()?; Ok(Value::Num(a.powf(b))) }
        "min"   => { let (a, b) = two()?; Ok(Value::Num(a.min(b))) }
        "max"   => { let (a, b) = two()?; Ok(Value::Num(a.max(b))) }
        "clamp" => {
            if vals.len() != 3 { return Err(arity_err(3)); }
            let (x, lo, hi) = (num(&vals[0], name)?, num(&vals[1], name)?, num(&vals[2], name)?);
            Ok(Value::Num(x.clamp(lo, hi)))
        }
        "lerp" => {
            if vals.len() != 3 { return Err(arity_err(3)); }
            let (a, b, t) = (num(&vals[0], name)?, num(&vals[1], name)?, num(&vals[2], name)?);
            Ok(Value::Num(a + (b - a) * t))
        }
        other => Err(EvalError {
            message: format!(
                "'{other}()' is not a function — available: {}",
                BUILTIN_NAMES.join(", ")
            ),
        }),
    }
}

fn expect_num_pub(what: &str, v: &Value) -> Result<f64, EvalError> {
    v.as_num().ok_or_else(|| EvalError {
        message: format!("{what} needs a number argument, got a {}", v.kind()),
    })
}

/// Every FREE identifier in an expression, with its span — for "is this
/// name in scope" checks that need to point at the exact token. A
/// comprehension's variable is bound inside its body and is not reported.
pub fn idents(expr: &Expr) -> Vec<Ident> {
    let mut out = Vec::new();
    collect(expr, &mut out);
    out
}

fn collect(expr: &Expr, out: &mut Vec<Ident>) {
    match expr {
        Expr::Ident(i) => out.push(i.clone()),
        Expr::Not(e) => collect(e, out),
        Expr::If { cond, then, else_ } => {
            collect(cond, out); collect(then, out); collect(else_, out);
        }
        Expr::BinOp { lhs, rhs, .. } => { collect(lhs, out); collect(rhs, out); }
        Expr::Call { args, .. } => for a in args { collect(&a.value, out) },
        Expr::List(items) => for e in items { collect(e, out) },
        Expr::Comprehension { var, start, end, body } => {
            collect(start, out);
            collect(end, out);
            let mut inner = Vec::new();
            collect(body, &mut inner);
            out.extend(inner.into_iter().filter(|i| i.name != var.name));
        }
        Expr::Index { base, index } => { collect(base, out); collect(index, out); }
        Expr::Int(_) | Expr::Float(_) | Expr::Str(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Span;

    fn id(name: &str) -> Expr { Expr::Ident(Ident { name: name.into(), span: Span::new(1, 1) }) }
    fn num(n: f64) -> Expr { Expr::Float(n) }
    fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
        Expr::BinOp { op, lhs: Box::new(l), rhs: Box::new(r) }
    }

    #[test]
    fn arithmetic_and_comparison() {
        let mut env = Env::new();
        env.insert("n".into(), Value::Num(3.0));
        let e = bin(BinOp::Mul, id("n"), num(2.0));
        assert_eq!(eval(&e, &env), Ok(Value::Num(6.0)));
        let c = bin(BinOp::Gt, id("n"), num(2.0));
        assert_eq!(eval(&c, &env), Ok(Value::Bool(true)));
    }

    #[test]
    fn undefined_name_lists_scope() {
        let mut env = Env::new();
        env.insert("length".into(), Value::Num(9.0));
        env.insert("girth".into(), Value::Num(0.8));
        let err = eval(&id("lenth"), &env).unwrap_err();
        assert_eq!(err.message, "'lenth' is not defined — in scope: girth, length");
    }

    /// The untaken branch is never evaluated: a division by zero there is
    /// not an error. This is what makes `if` usable as a guard.
    #[test]
    fn if_evaluates_only_the_taken_branch() {
        let mut env = Env::new();
        env.insert("d".into(), Value::Num(0.0));
        let e = Expr::If {
            cond:  Box::new(bin(BinOp::Gt, id("d"), num(0.0))),
            then:  Box::new(bin(BinOp::Div, num(1.0), id("d"))),
            else_: Box::new(num(-1.0)),
        };
        assert_eq!(eval(&e, &env), Ok(Value::Num(-1.0)));
    }

    #[test]
    fn type_errors_name_the_operator() {
        let env = Env::new();
        let e = bin(BinOp::Add, num(1.0), bin(BinOp::Lt, num(1.0), num(2.0)));
        let err = eval(&e, &env).unwrap_err();
        assert!(err.message.contains("`+` needs a number"), "got: {}", err.message);
    }

    #[test]
    fn idents_are_collected_with_spans() {
        let e = Expr::If {
            cond:  Box::new(id("a")),
            then:  Box::new(id("b")),
            else_: Box::new(bin(BinOp::Add, id("c"), num(1.0))),
        };
        let names: Vec<String> = idents(&e).into_iter().map(|i| i.name).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }
    
    fn call(name: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            name: name.into(),
            args: args.into_iter().map(|value| NamedArg { key: String::new(), value }).collect(),
        }
    }

    // ── Phase D.2: lists ─────────────────────────────────────────────

    fn list(items: Vec<Expr>) -> Expr { Expr::List(items) }

    #[test]
    fn list_literal_index_and_len() {
        let env = Env::new();
        let xs = list(vec![num(3.0), num(5.5), num(8.0)]);
        assert_eq!(eval(&xs, &env), Ok(Value::List(vec![Value::Num(3.0), Value::Num(5.5), Value::Num(8.0)])));
        let at = |i: f64| Expr::Index { base: Box::new(xs.clone()), index: Box::new(num(i)) };
        assert_eq!(eval(&at(1.0), &env), Ok(Value::Num(5.5)));
        assert_eq!(eval(&call("len", vec![xs.clone()]), &env), Ok(Value::Num(3.0)));

        let err = eval(&at(3.0), &env).unwrap_err();
        assert!(err.message.contains("out of range for a list of 3"), "got: {}", err.message);
        let err = eval(&at(1.5), &env).unwrap_err();
        assert!(err.message.contains("whole number"), "got: {}", err.message);
        let err = eval(&Expr::Index { base: Box::new(num(2.0)), index: Box::new(num(0.0)) }, &env).unwrap_err();
        assert!(err.message.contains("only a list can be indexed"), "got: {}", err.message);
    }

    #[test]
    fn comprehension_binds_its_variable_and_can_shadow() {
        let mut env = Env::new();
        env.insert("n".into(), Value::Num(4.0));
        env.insert("i".into(), Value::Num(100.0)); // shadowed inside the body
        let e = Expr::Comprehension {
            var:   Box::new(Ident { name: "i".into(), span: Span::new(1, 1) }),
            start: Box::new(num(0.0)),
            end:   Box::new(id("n")),
            body:  Box::new(bin(BinOp::Mul, id("i"), num(2.0))),
        };
        assert_eq!(eval(&e, &env), Ok(Value::List(vec![
            Value::Num(0.0), Value::Num(2.0), Value::Num(4.0), Value::Num(6.0)])));
        // The variable is not free.
        let names: Vec<String> = idents(&e).into_iter().map(|i| i.name).collect();
        assert_eq!(names, vec!["n"]);
    }

    /// A point is a list of three numbers; a path is a list of points.
    #[test]
    fn nested_lists_make_points_and_paths() {
        let env = Env::new();
        let path = Expr::Comprehension {
            var:   Box::new(Ident { name: "t".into(), span: Span::new(1, 1) }),
            start: Box::new(num(0.0)),
            end:   Box::new(num(3.0)),
            body:  Box::new(list(vec![id("t"), bin(BinOp::Mul, id("t"), num(2.0)), num(0.0)])),
        };
        let v = eval(&path, &env).unwrap();
        let items = v.as_list().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[2], Value::List(vec![Value::Num(2.0), Value::Num(4.0), Value::Num(0.0)]));
        // Round-trips to the AST as nested literals.
        assert!(matches!(v.to_expr(), Expr::List(_)));
    }

    #[test]
    fn comprehension_is_capped() {
        let env = Env::new();
        let e = Expr::Comprehension {
            var:   Box::new(Ident { name: "i".into(), span: Span::new(1, 1) }),
            start: Box::new(num(0.0)),
            end:   Box::new(num(5000.0)),
            body:  Box::new(id("i")),
        };
        let err = eval(&e, &env).unwrap_err();
        assert!(err.message.contains("4096"), "got: {}", err.message);
    }

    #[test]
    fn trig_takes_degrees() {
        let env = Env::new();
        let s = eval(&call("sin", vec![num(90.0)]), &env).unwrap();
        assert!((s.as_num().unwrap() - 1.0).abs() < 1e-9, "sin(90) must be 1, degrees not radians");
        let c = eval(&call("cos", vec![num(180.0)]), &env).unwrap();
        assert!((c.as_num().unwrap() + 1.0).abs() < 1e-9);
    }

    #[test]
    fn clamp_and_lerp() {
        let env = Env::new();
        assert_eq!(eval(&call("clamp", vec![num(15.0), num(0.0), num(10.0)]), &env), Ok(Value::Num(10.0)));
        assert_eq!(eval(&call("lerp", vec![num(0.0), num(10.0), num(0.25)]), &env), Ok(Value::Num(2.5)));
    }

    #[test]
    fn min_max_pow_sqrt_abs_floor_round() {
        let env = Env::new();
        assert_eq!(eval(&call("min", vec![num(3.0), num(-1.0)]), &env), Ok(Value::Num(-1.0)));
        assert_eq!(eval(&call("max", vec![num(3.0), num(-1.0)]), &env), Ok(Value::Num(3.0)));
        assert_eq!(eval(&call("pow", vec![num(2.0), num(10.0)]), &env), Ok(Value::Num(1024.0)));
        assert_eq!(eval(&call("sqrt", vec![num(9.0)]), &env), Ok(Value::Num(3.0)));
        assert_eq!(eval(&call("abs", vec![num(-4.5)]), &env), Ok(Value::Num(4.5)));
        assert_eq!(eval(&call("floor", vec![num(4.7)]), &env), Ok(Value::Num(4.0)));
        assert_eq!(eval(&call("round", vec![num(4.5)]), &env), Ok(Value::Num(5.0)));
    }

    #[test]
    fn sqrt_of_negative_is_an_error() {
        let env = Env::new();
        let err = eval(&call("sqrt", vec![num(-4.0)]), &env).unwrap_err();
        assert!(err.message.contains("negative"), "got: {}", err.message);
    }

    #[test]
    fn wrong_arity_names_the_function_and_the_count() {
        let env = Env::new();
        let err = eval(&call("clamp", vec![num(1.0)]), &env).unwrap_err();
        assert!(err.message.contains("'clamp'") && err.message.contains("3 argument"),
                "got: {}", err.message);
    }

    #[test]
    fn unknown_function_lists_the_vocabulary() {
        let env = Env::new();
        let err = eval(&call("sine", vec![num(1.0)]), &env).unwrap_err();
        assert!(err.message.contains("'sine()' is not a function") && err.message.contains("sin"),
                "got: {}", err.message);
    }

    /// `taper` from the ribcage design: a smooth 0→1→0 curve across a
    /// count, built only from things E1 provides.
    #[test]
    fn taper_curve_is_expressible() {
        let mut env = Env::new();
        env.insert("i".into(), Value::Num(0.0));
        env.insert("n".into(), Value::Num(12.0));
        // sin(180 * (i + 0.5) / n)
        let e = call("sin", vec![bin(
            BinOp::Mul, num(180.0),
            bin(BinOp::Div, bin(BinOp::Add, id("i"), num(0.5)), id("n")),
        )]);
        let v = eval(&e, &env).unwrap().as_num().unwrap();
        assert!(v > 0.0 && v < 1.0, "first rib should be partway up the taper, got {v}");
    }
}