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

use crate::ast::{BinOp, Expr, Ident};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Num(f64),
    Bool(bool),
}

impl Value {
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Num(_)  => "number",
            Value::Bool(_) => "boolean",
        }
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
    pub fn to_expr(self) -> Expr {
        match self {
            Value::Num(n)  => Expr::Float(n),
            Value::Bool(b) => Expr::Int(b as i64),
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

fn expect_num(what: &str, v: Value) -> Result<f64, EvalError> {
    v.as_num().ok_or_else(|| EvalError {
        message: format!("{what} needs a number, got a {}", v.kind()),
    })
}

fn expect_bool(what: &str, v: Value) -> Result<bool, EvalError> {
    v.as_bool().ok_or_else(|| EvalError {
        message: format!("{what} needs a boolean (a comparison, `and`/`or`/`not`), got a {}", v.kind()),
    })
}

pub fn eval(expr: &Expr, env: &Env) -> Result<Value, EvalError> {
    match expr {
        Expr::Int(n)   => Ok(Value::Num(*n as f64)),
        Expr::Float(f) => Ok(Value::Num(*f)),
        Expr::Ident(i) => env.get(&i.name).copied().ok_or_else(|| undefined(&i.name, env)),

        Expr::Not(inner) => {
            let b = expect_bool("`not`", eval(inner, env)?)?;
            Ok(Value::Bool(!b))
        }

        // Only the taken branch is evaluated, so `if d > 0 { n / d } else
        // { 0 }` is legal even when d is zero.
        Expr::If { cond, then, else_ } => {
            let c = expect_bool("`if` condition", eval(cond, env)?)?;
            if c { eval(then, env) } else { eval(else_, env) }
        }

        Expr::BinOp { op, lhs, rhs } => {
            let l = eval(lhs, env)?;
            let r = eval(rhs, env)?;
            match op {
                BinOp::And => Ok(Value::Bool(expect_bool("`and`", l)? && expect_bool("`and`", r)?)),
                BinOp::Or  => Ok(Value::Bool(expect_bool("`or`", l)?  || expect_bool("`or`", r)?)),

                BinOp::Add => Ok(Value::Num(expect_num("`+`", l)? + expect_num("`+`", r)?)),
                BinOp::Sub => Ok(Value::Num(expect_num("`-`", l)? - expect_num("`-`", r)?)),
                BinOp::Mul => Ok(Value::Num(expect_num("`*`", l)? * expect_num("`*`", r)?)),
                BinOp::Div => {
                    let (a, b) = (expect_num("`/`", l)?, expect_num("`/`", r)?);
                    if b == 0.0 {
                        return Err(EvalError { message: "division by zero".to_string() });
                    }
                    Ok(Value::Num(a / b))
                }

                BinOp::Lt   => Ok(Value::Bool(expect_num("`<`",  l)? <  expect_num("`<`",  r)?)),
                BinOp::Gt   => Ok(Value::Bool(expect_num("`>`",  l)? >  expect_num("`>`",  r)?)),
                BinOp::LtEq => Ok(Value::Bool(expect_num("`<=`", l)? <= expect_num("`<=`", r)?)),
                BinOp::GtEq => Ok(Value::Bool(expect_num("`>=`", l)? >= expect_num("`>=`", r)?)),

                BinOp::Eq | BinOp::Neq => {
                    let same = match (l, r) {
                        (Value::Num(a),  Value::Num(b))  => a == b,
                        (Value::Bool(a), Value::Bool(b)) => a == b,
                        _ => return Err(EvalError {
                            message: format!("cannot compare a {} with a {}", l.kind(), r.kind()),
                        }),
                    };
                    Ok(Value::Bool(if *op == BinOp::Eq { same } else { !same }))
                }
            }
        }

        Expr::Str(_)  => Err(EvalError { message: "strings are not values yet".to_string() }),
        Expr::List(_) => Err(EvalError { message: "lists are not values yet".to_string() }),
        Expr::Call { name, .. } => Err(EvalError {
            message: format!("'{name}()' is not a function; there are no functions yet"),
        }),
    }
}

/// Every identifier in an expression, with its span — for "is this name
/// in scope" checks that need to point at the exact token.
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
}