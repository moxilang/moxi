//! Marks: flat things drawn on a surface (0.9, proposals/MARKS.md).
//!
//! `part EyeR { mark = oval(width=8.6, height=10.8), material = Eye }`
//! is a part with no depth. Its outline lies in its local XY plane, its
//! front faces +Z, and its one anchor `center` faces −Z, so `EyeR on
//! Body.side(…)` lays it on the body facing out. Until marks are drawn
//! (step 2) they are solved, reported and checked, and every exporter
//! leaves them out with a note.

use crate::ast::{Expr, MarkKind, NamedArg};
use crate::geometry::arg_f64;

/// The outline's half-extents in its own plane: (half width, half height).
/// A band's height is its width along the host's axis.
pub fn half_extents(kind: MarkKind, args: &[NamedArg]) -> (f64, f64) {
    match kind {
        MarkKind::Oval | MarkKind::Rect =>
            (arg_f64(args, "width", 1.0) / 2.0, arg_f64(args, "height", 1.0) / 2.0),
        MarkKind::Band => { let w = arg_f64(args, "width", 1.0) / 2.0; (w, w) }
        MarkKind::Poly => points(args).iter().fold((0.0f64, 0.0f64), |(x, y), p| (x.max(p[0].abs()), y.max(p[1].abs()))),
    }
}

/// `poly(points=[[x, y], …])`, as numbers (non-numbers dropped; the
/// argument check reports them).
pub fn points(args: &[NamedArg]) -> Vec<[f64; 2]> {
    let num = |e: &Expr| match e { Expr::Float(f) => Some(*f), Expr::Int(n) => Some(*n as f64), _ => None };
    match args.iter().find(|a| a.key == "points").map(|a| &a.value) {
        Some(Expr::List(ps)) => ps.iter().filter_map(|p| match p {
            Expr::List(xy) if xy.len() == 2 => Some([num(&xy[0])?, num(&xy[1])?]),
            _ => None,
        }).collect(),
        _ => Vec::new(),
    }
}

/// The arguments each outline takes, in the order the guide lists them.
pub fn arg_names(kind: MarkKind) -> &'static [&'static str] {
    match kind {
        MarkKind::Oval => &["width", "height"],
        MarkKind::Rect => &["width", "height", "round"],
        MarkKind::Poly => &["points"],
        MarkKind::Band => &["width"],
    }
}

/// What is wrong with a mark's arguments, once they are numbers: an
/// unknown or missing argument, a size that is not positive, a polygon of
/// fewer than three points. `None` when it is fine.
pub fn check(kind: MarkKind, args: &[NamedArg]) -> Option<String> {
    let names = arg_names(kind);
    let call = format!("{}({})", kind.name(), names.iter().map(|n| format!("{n}=…")).collect::<Vec<_>>().join(", "));
    for a in args {
        if !names.contains(&a.key.as_str()) {
            return Some(format!("`{}` takes {}, not `{}`: {call}", kind.name(), names.join(", "), a.key));
        }
    }
    if kind == MarkKind::Poly {
        let n = points(args).len();
        let given = args.iter().find(|a| a.key == "points").map(|a| match &a.value { Expr::List(ps) => ps.len(), _ => 0 });
        return match given {
            None => Some(format!("`poly` needs its outline: {call}, e.g. points=[[0, 2], [2, -1], [-2, -1]]")),
            Some(g) if g != n => Some("`poly` points are pairs of numbers, [[x, y], …], in the mark's own plane".into()),
            Some(_) if n < 3 => Some(format!("`poly` needs at least 3 points to close an outline, not {n}")),
            _ => None,
        };
    }
    for &k in names {
        if k == "round" { continue; }
        match args.iter().find(|a| a.key == k) {
            None => return Some(format!("`{}` needs `{k}`: {call}", kind.name())),
            Some(a) => match a.value {
                Expr::Float(v) if v > 0.0 => {}
                Expr::Int(v) if v > 0 => {}
                Expr::Float(_) | Expr::Int(_) => return Some(format!("`{}` {k} must be more than 0", kind.name())),
                _ => {}
            },
        }
    }
    if kind == MarkKind::Rect {
        let (hw, hh) = half_extents(kind, args);
        let r = arg_f64(args, "round", 0.0);
        if r < 0.0 || r > hw.min(hh) {
            return Some(format!("`rect` round is at most half its smaller side ({}), not {}",
                crate::mesh::num(hw.min(hh)), crate::mesh::num(r)));
        }
    }
    None
}

/// Distance to the outline's box, flat in the XY plane: never negative, a
/// mark has no inside. Only what reads distance without knowing about marks
/// (bounds, contacts) sees this; nothing meshes it.
pub fn distance(kind: MarkKind, args: &[NamedArg], x: f64, y: f64, z: f64) -> f64 {
    let (hw, hh) = half_extents(kind, args);
    let (dx, dy) = ((x.abs() - hw).max(0.0), (y.abs() - hh).max(0.0));
    (dx * dx + dy * dy + z * z).sqrt()
}
