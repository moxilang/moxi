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

// ── Drawing (step 2) ──────────────────────────────────────────────────────
//
// A mark is drawn as a fixed grid of points, the same count for any size of
// outline, so a pose that reshapes it moves points and never adds them
// (step 3 exports that as a morph). Each point of an oval, rect or poly is
// pushed along the mark's facing direction until it meets its host's exact
// surface (the distance field, not the host's mesh); each point of a band
// is pushed straight out from the host's axis. A point that meets no
// surface, or meets it where a cut has changed it, is dropped, and every
// triangle that uses it with it: that is a mark's hole over a mouth cut.

use crate::ast::ShapeExpr;
use crate::frame::{Frame, Vec3};
use crate::mesh::TriMesh;
use crate::scene::{Layer, Part, Shape};

/// A mark's outline with a pose's `width` and `height` (under any mirror).
pub fn resized(shape: &Shape, width: Option<f64>, height: Option<f64>) -> Shape {
    match shape {
        Shape::Mirror { inner, nx, ny, nz } => Shape::Mirror {
            inner: Box::new(resized(inner, width, height)), nx: *nx, ny: *ny, nz: *nz,
        },
        Shape::Mark { outline, width: w, height: h, round, points, image } => Shape::Mark {
            outline: outline.clone(),
            width:   width.unwrap_or(*w),
            height:  height.unwrap_or(*h),
            round:   *round,
            points:  points.clone(),
            image:   image.clone(),
        },
        other => other.clone(),
    }
}

/// The share of a mark's points that may meet no surface before the mark
/// is said to hang off its host.
pub const MAX_OFF: f64 = 0.5;
/// How much larger than its flat outline a drawn mark may come out: more
/// means it wraps a curve too tight for it.
pub const MAX_STRETCH: f64 = 1.5;

/// The part a mark lies on: its parent, or the part that parent was fused
/// or cut into. A mark on a mark (a shine on an eye) lies on the surface
/// the lower mark lies on.
pub fn host_of<'a>(layer: &'a Layer, mark: &Part) -> Option<&'a Part> {
    let mut name = mark.joint.as_ref()?.parent.clone()?;
    for _ in 0..64 {
        let p = layer.parts.iter().find(|p| p.name == name)?;
        if p.is_mark() {
            name = p.joint.as_ref()?.parent.clone()?;
            continue;
        }
        match p.absorbed.as_ref().and_then(|a| a.into.clone()) {
            Some(into) => name = into,
            None => return Some(p),
        }
    }
    None
}

/// The mark this mark lies on, if it lies on one.
pub fn lower_mark<'a>(layer: &'a Layer, mark: &Part) -> Option<&'a Part> {
    let name = mark.joint.as_ref()?.parent.as_ref()?;
    layer.parts.iter().find(|p| &p.name == name && p.is_mark())
}

/// How many marks lie under this one: each is drawn a lift higher.
pub(crate) fn depth_of(layer: &Layer, mark: &Part) -> usize {
    let mut d = 0;
    let mut m = lower_mark(layer, mark);
    while let Some(p) = m {
        d += 1;
        if d > 64 { break; }
        m = lower_mark(layer, p);
    }
    d
}

/// The outline sizes (width, height) of a mark's shape.
fn sizes(shape: &Shape) -> Option<(f64, f64)> {
    match shape {
        Shape::Mirror { inner, .. } => sizes(inner),
        Shape::Mark { width, height, .. } => Some((*width, *height)),
        _ => None,
    }
}

/// When the mark this one lies on is reshaped (a pose's size against its
/// rest size), this one is squashed with it about the lower mark's middle:
/// the lower mark's plane and its (width, height) factors.
fn squash_of(layer: &Layer, mark: &Part) -> Option<(Plane, f64, f64)> {
    let low = lower_mark(layer, mark)?;
    let (w, h) = sizes(&low.shape)?;
    let (w0, h0) = sizes(low.mark_poses.get("rest")?)?;
    let (sw, sh) = (if w0 > 0.0 && w > 0.0 { w / w0 } else { 1.0 }, if h0 > 0.0 && h > 0.0 { h / h0 } else { 1.0 });
    ((sw - 1.0).abs() > 1e-12 || (sh - 1.0).abs() > 1e-12).then(|| (Plane::of(low), sw, sh))
}

/// The host's surface as the mark sees it, in world coordinates.
struct Surface {
    expr:  ShapeExpr,
    /// The same without its top-level cuts: where the two disagree, a cut
    /// has changed the surface and the mark has a hole.
    uncut: Option<ShapeExpr>,
    inv:   Frame,
    vs:    f64,
    cell:  f64,
}

impl Surface {
    fn new(host: &Part, layer_vs: f64, cell: Option<f64>) -> Surface {
        let expr = host.shape.to_expr();
        let vs = host.grain(layer_vs);
        let cell = cell.unwrap_or_else(|| crate::mesh::auto_cell(&expr, vs));
        let uncut = match &expr { ShapeExpr::Difference { base, .. } => Some((**base).clone()), _ => None };
        Surface { expr, uncut, inv: host.frame.to_frame().inverse(), vs, cell }
    }
    fn d(&self, e: &ShapeExpr, p: Vec3) -> f64 { crate::geometry::distance(e, self.inv.apply_point(p), self.vs) }
    fn normal(&self, p: Vec3) -> Vec3 {
        let h = self.cell * 0.05;
        let g = |a: Vec3| self.d(&self.expr, p.add(a)) - self.d(&self.expr, p.sub(a));
        Vec3::new(g(Vec3::new(h, 0.0, 0.0)), g(Vec3::new(0.0, h, 0.0)), g(Vec3::new(0.0, 0.0, h)))
            .normalize().unwrap_or(Vec3::Y)
    }

    /// Where the line `p + t·dir` first meets the surface going from t = 0
    /// toward `dir`, for t up to `reach`: the distance is a lower bound, so
    /// stepping by it never jumps a crossing; the crossing is then halved
    /// down to a millionth of the cell.
    fn cross(&self, e: &ShapeExpr, p: Vec3, dir: Vec3, reach: f64) -> Option<f64> {
        let f = |t: f64| self.d(e, p.add(dir.scale(t)));
        let f0 = f(0.0);
        if f0 == 0.0 { return Some(0.0); }
        let floor = self.cell * 0.02;
        let (mut a, mut fa) = (0.0, f0);
        while a < reach {
            let b = (a + fa.abs().max(floor)).min(reach);
            let fb = f(b);
            if fb == 0.0 || fb.signum() != f0.signum() {
                let (mut lo, mut hi) = (a, b);
                while hi - lo > self.cell * 1e-6 {
                    let m = 0.5 * (lo + hi);
                    if f(m).signum() == f0.signum() { lo = m } else { hi = m }
                }
                return Some(0.5 * (lo + hi));
            }
            if b >= reach { break; }
            a = b;
            fa = fb;
        }
        None
    }

    /// The crossing nearest t = 0 along ±dir.
    fn nearest(&self, e: &ShapeExpr, p: Vec3, dir: Vec3, reach: f64) -> Option<f64> {
        let fwd = self.cross(e, p, dir, reach);
        let back = self.cross(e, p, dir.neg(), fwd.unwrap_or(reach)).map(|t| -t);
        match (fwd, back) {
            (Some(a), Some(b)) => Some(if b.abs() < a { b } else { a }),
            (a, b) => a.or(b),
        }
    }

    /// The surface point on the line, unless it is over a cut.
    fn land(&self, p: Vec3, dir: Vec3, reach: f64, both_ways: bool) -> Option<Vec3> {
        let find = |e: &ShapeExpr| if both_ways { self.nearest(e, p, dir, reach) } else { self.cross(e, p, dir, reach) };
        let t = find(&self.expr)?;
        if let Some(u) = &self.uncut {
            match find(u) {
                Some(tu) if (tu - t).abs() <= self.cell * 0.25 => {}
                _ => return None,
            }
        }
        Some(p.add(dir.scale(t)))
    }
}

/// A point of the outline at angle `a` (radians) and distance `r` from its
/// middle, in the mark's own plane.
fn at(a: f64, r: f64) -> (f64, f64) { (r * a.cos(), r * a.sin()) }

/// How far the outline reaches from its middle at angle `a`: the outline
/// must be star-shaped around (0, 0), which `check_star` makes sure of for a
/// poly (an oval and a rect always are).
fn reach_at(kind: MarkKind, args: &[NamedArg], pts: &[[f64; 2]], a: f64) -> f64 {
    let (c, s) = (a.cos(), a.sin());
    match kind {
        MarkKind::Oval | MarkKind::Band => {
            let (hw, hh) = half_extents(kind, args);
            1.0 / ((c / hw).powi(2) + (s / hh).powi(2)).sqrt()
        }
        MarkKind::Rect => {
            let (hw, hh) = half_extents(kind, args);
            let r = arg_f64(args, "round", 0.0);
            let inside = |x: f64, y: f64| {
                let (qx, qy) = (x.abs() - (hw - r), y.abs() - (hh - r));
                let out = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0);
                out <= r
            };
            let (mut lo, mut hi) = (0.0, (hw * hw + hh * hh).sqrt() * 1.01);
            for _ in 0..60 {
                let m = 0.5 * (lo + hi);
                if inside(m * c, m * s) { lo = m } else { hi = m }
            }
            lo
        }
        MarkKind::Poly => {
            // the ray from (0, 0) meets exactly one edge of a star-shaped outline
            let mut best = f64::INFINITY;
            for i in 0..pts.len() {
                let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
                let (ex, ey) = (q[0] - p[0], q[1] - p[1]);
                let den = c * ey - s * ex;
                if den.abs() < 1e-15 { continue; }
                let t = (p[0] * ey - p[1] * ex) / den;
                let u = (p[0] * s - p[1] * c) / den;
                if t > 0.0 && (-1e-12..=1.0 + 1e-12).contains(&u) { best = best.min(t); }
            }
            if best.is_finite() { best } else { 0.0 }
        }
    }
}

/// A poly's points must go once around its middle (0, 0), so every ray
/// from the middle meets the outline once.
pub fn check_star(pts: &[[f64; 2]]) -> Option<String> {
    let turn: Vec<f64> = (0..pts.len()).map(|i| {
        let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
        p[0] * q[1] - p[1] * q[0]
    }).collect();
    let all_pos = turn.iter().all(|t| *t > 0.0);
    let all_neg = turn.iter().all(|t| *t < 0.0);
    (!(all_pos || all_neg)).then(|| "a `poly`'s points must go once around its middle (0, 0), in order: \
        every line from (0, 0) outward crosses the outline once, so a crescent or a C does not draw; \
        split it into two marks".to_string())
}

/// The angles a grid goes round at, measured on the outline scaled to a
/// unit square (x by its half width, y by its half height): `n` even steps,
/// plus every corner of a rect or poly so the outline keeps its corners. In
/// those scaled angles a reshaped outline moves its points with it (a
/// squashed oval keeps its width), and a rect's corners never move (45°).
fn angles(kind: MarkKind, args: &[NamedArg], pts: &[[f64; 2]], n: usize) -> Vec<f64> {
    use std::f64::consts::TAU;
    let mut a: Vec<f64> = (0..n).map(|i| TAU * i as f64 / n as f64).collect();
    let (hw, hh) = half_extents(kind, args);
    let corner = |x: f64, y: f64| (y / hh).atan2(x / hw).rem_euclid(TAU);
    match kind {
        MarkKind::Rect if arg_f64(args, "round", 0.0) == 0.0 => {
            a.extend([corner(hw, hh), corner(-hw, hh), corner(-hw, -hh), corner(hw, -hh)]);
        }
        MarkKind::Poly => a.extend(pts.iter().map(|p| corner(p[0], p[1]))),
        _ => {}
    }
    // kept even where a corner falls on an even step: dropping it would make
    // the count depend on the outline's proportions, and a reshaped mark
    // must keep its points (the pair only makes a triangle of no area)
    a.sort_by(f64::total_cmp);
    a
}

/// How a mark's own plane sits in the world: its middle, the direction it
/// faces, and the way from its plane coordinates to the world.
struct Plane { frame: Frame, mirror: Option<Vec3> }

impl Plane {
    fn of(part: &Part) -> Plane {
        let mirror = match &part.shape {
            Shape::Mirror { nx, ny, nz, .. } => Vec3::new(*nx, *ny, *nz).normalize(),
            _ => None,
        };
        Plane { frame: part.frame.to_frame(), mirror }
    }
    fn local(&self, q: Vec3) -> Vec3 {
        match self.mirror { Some(n) => q.sub(n.scale(2.0 * q.dot(n))), None => q }
    }
    fn point(&self, x: f64, y: f64) -> Vec3 { self.frame.apply_point(self.local(Vec3::new(x, y, 0.0))) }
    /// A world point in this plane's own coordinates.
    fn coords(&self, w: Vec3) -> Vec3 { self.local(self.frame.inverse().apply_point(w)) }
    fn dir(&self, d: Vec3) -> Vec3 { self.frame.apply_dir(self.local(d)) }
}

/// The mark's own outline and arguments, under any mirror.
fn outline(shape: &Shape) -> Option<(MarkKind, Vec<NamedArg>)> {
    match shape {
        Shape::Mirror { inner, .. } => outline(inner),
        Shape::Mark { .. } => match shape.to_expr() {
            ShapeExpr::Mark { kind, args, .. } => Some((kind, args)),
            _ => None,
        },
        _ => None,
    }
}

/// How many points a mark is drawn with: (around, across) — angles and
/// rings for an outline, columns and rows for a band. A reshaped mark is
/// drawn with its rest grid, so its points match one for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid(pub usize, pub usize);

/// The mark, drawn on its host, in its own part's coordinates (what every
/// exporter writes a part in). `cell` is an exporter's `--cell`, if any.
/// An error says why it cannot be drawn: no host, mostly off its host, or
/// stretched over a curve too tight for it.
pub fn draw(layer: &Layer, mark: &Part, cell: Option<f64>) -> Result<TriMesh, String> {
    draw_grid(layer, mark, cell, None).map(|(m, _)| m)
}

/// `draw`, with the grid given (a reshaped mark takes its rest grid) or
/// chosen from the mark's size, and returned.
pub fn draw_grid(layer: &Layer, mark: &Part, cell: Option<f64>, grid: Option<Grid>) -> Result<(TriMesh, Grid), String> {
    draw_mark(layer, mark, cell, grid).map(|m| (m.mesh, m.grid))
}

/// A drawn mark: its mesh, the grid it was drawn on, and a texture
/// coordinate per point (for an image).
pub struct Drawn { pub mesh: TriMesh, pub grid: Grid, pub uvs: Vec<[f32; 2]> }

/// `draw_grid`, with texture coordinates.
pub fn draw_mark(layer: &Layer, mark: &Part, cell: Option<f64>, grid: Option<Grid>) -> Result<Drawn, String> {
    let (kind, args) = outline(&mark.shape).ok_or_else(|| format!("'{}' is not a mark", mark.name))?;
    let host = host_of(layer, mark).ok_or_else(|| format!("'{}' is a mark with nothing to lie on", mark.name))?;
    let surf = Surface::new(host, layer.voxel_size, cell);
    let plane = Plane::of(mark);
    let pts = points(&args);
    if kind == MarkKind::Poly { if let Some(e) = check_star(&pts) { return Err(format!("'{}': {e}", mark.name)); } }
    let (hw, hh) = half_extents(kind, &args);
    // on a reshaped lower mark, squashed with it about its middle
    let squash = squash_of(layer, mark);
    let flat_at = |x: f64, y: f64| -> Vec3 {
        let w = plane.point(x, y);
        match &squash {
            Some((low, sw, sh)) => { let q = low.coords(w); low.point(q.x * sw, q.y * sh) }
            None => w,
        }
    };
    let center = flat_at(0.0, 0.0);
    let face = plane.dir(Vec3::Z).normalize().unwrap_or(Vec3::Z);

    // (world point or None, the flat point it came from)
    let mut verts: Vec<(Option<Vec3>, Vec3)> = Vec::new();
    let mut tris: Vec<[usize; 3]> = Vec::new();
    // texture coordinates, one per point: across the outline's box, or
    // around and across a band
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    // `stride` points a row; a ring wraps (stride = cols), a band has its
    // seam twice (stride = cols + 1) so an image does not smear across it
    let mut wrap_grid = |verts: &mut Vec<(Option<Vec3>, Vec3)>, cols: usize, stride: usize, rows: usize, start: usize, closed_center: Option<usize>| {
        let id = |r: usize, c: usize| start + r * stride + c % stride;
        if let Some(c0) = closed_center {
            for c in 0..cols { tris.push([c0, id(0, c), id(0, c + 1)]); }
        }
        for r in 0..rows.saturating_sub(1) {
            for c in 0..cols {
                tris.push([id(r, c), id(r + 1, c), id(r + 1, c + 1)]);
                tris.push([id(r, c), id(r + 1, c + 1), id(r, c + 1)]);
            }
        }
        let _ = verts;
    };

    let size = hw.max(hh);
    let step = surf.cell.min(size / 6.0).max(1e-6);
    let used: Grid;
    if kind == MarkKind::Band {
        // the axis: the deepest point straight in from the band's middle
        let inward = face.neg();
        let through = surf.cross(&surf.expr, center.add(inward.scale(step)), inward, 1e6)
            .ok_or_else(|| format!("'{}' is a band, and its host has no inside under it", mark.name))? + step;
        let (mut lo, mut hi) = (0.0, through);
        let g = 0.618_033_988_75;
        for _ in 0..80 {
            let (m1, m2) = (hi - g * (hi - lo), lo + g * (hi - lo));
            if surf.d(&surf.expr, center.add(inward.scale(m1))) < surf.d(&surf.expr, center.add(inward.scale(m2))) { hi = m2 } else { lo = m1 }
        }
        let depth = 0.5 * (lo + hi);
        let axis_pt = center.add(inward.scale(depth));
        let along = plane.dir(Vec3::Y).normalize().unwrap_or(Vec3::Y);
        let side = along.cross(face).normalize().unwrap_or(Vec3::X);
        let width = 2.0 * hh;
        let Grid(cols, rows) = grid.unwrap_or(Grid(
            ((std::f64::consts::TAU * depth / step).ceil() as usize).clamp(48, 1024),
            ((width / step).ceil() as usize).clamp(2, 200) + 1));
        used = Grid(cols, rows);
        for r in 0..rows {
            let s = -width / 2.0 + width * r as f64 / (rows - 1) as f64;
            for c in 0..=cols {
                let a = std::f64::consts::TAU * c as f64 / cols as f64;
                let dir = face.scale(a.cos()).add(side.scale(a.sin()));
                let o = axis_pt.add(along.scale(s));
                let hit = if surf.d(&surf.expr, o) < 0.0 { surf.land(o, dir, depth * 4.0 + surf.cell, false) } else { None };
                verts.push((hit, o.add(dir.scale(depth))));
                uvs.push([c as f32 / cols as f32, 1.0 - r as f32 / (rows - 1) as f32]);
            }
        }
        wrap_grid(&mut verts, cols, cols + 1, rows, 0, None);
    } else {
        let Grid(ring_n, rings) = grid.unwrap_or(Grid(
            ((std::f64::consts::TAU * size / step).ceil() as usize).clamp(32, 720),
            ((size / step).ceil() as usize).clamp(3, 200)));
        used = Grid(ring_n, rings);
        let angs = angles(kind, &args, &pts, ring_n);
        let reach = size + 2.0 * surf.cell;
        verts.push((surf.land(center, face, reach, true), center));
        let uv = |x: f64, y: f64| [((x + hw) / (2.0 * hw)) as f32, (1.0 - (y + hh) / (2.0 * hh)) as f32];
        uvs.push(uv(0.0, 0.0));
        for k in 1..=rings {
            let v = k as f64 / rings as f64;
            for &s in &angs {
                // the scaled angle, back in the mark's own plane
                let a = (hh * s.sin()).atan2(hw * s.cos());
                let (x, y) = at(a, v * reach_at(kind, &args, &pts, a));
                let flat = flat_at(x, y);
                verts.push((surf.land(flat, face, reach, true), flat));
                uvs.push(uv(x, y));
            }
        }
        wrap_grid(&mut verts, angs.len(), angs.len(), rings, 1, Some(0));
    }

    // what is off the host, by area of the flat outline (the grid's points
    // crowd its middle, so a count would overweigh a hole there)
    let area = |a: Vec3, b: Vec3, c: Vec3| b.sub(a).cross(c.sub(a)).length() * 0.5;
    let flat_area = |t: &[usize; 3]| area(verts[t[0]].1, verts[t[1]].1, verts[t[2]].1);
    let total: f64 = tris.iter().map(flat_area).sum();
    let lost: f64 = tris.iter().filter(|t| t.iter().any(|&i| verts[i].0.is_none())).map(flat_area).sum();
    let off = if total > 0.0 { lost / total } else { 1.0 };
    if off > MAX_OFF {
        return Err(format!("'{}' hangs off '{}': {:.0}% of it meets no surface there (over an edge, or a cut); \
            move it onto the part, or make it smaller", mark.name, host.name, off * 100.0));
    }
    let kept: Vec<[usize; 3]> = tris.into_iter().filter(|t| t.iter().all(|&i| verts[i].0.is_some())).collect();
    // stretch, by the length of its edges drawn against flat, squared to
    // an area: an area ratio of a sliver (a shut eye's shine) is all noise,
    // the curve's depth across a width of nothing
    if kind != MarkKind::Band {
        let (mut drawn, mut flat) = (0.0, 0.0);
        for t in &kept {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                drawn += verts[a].0.unwrap().sub(verts[b].0.unwrap()).length();
                flat += verts[a].1.sub(verts[b].1).length();
            }
        }
        let stretch = if flat > 0.0 { (drawn / flat).powi(2) } else { 1.0 };
        if stretch > MAX_STRETCH {
            return Err(format!("'{}' wraps a curve of '{}' too tight for it: drawn, it is {:.1} times its flat size \
                (at most {MAX_STRETCH}); make it smaller, or use a `band`", mark.name, host.name, stretch));
        }
    }

    // lifted a tenth of the cell off the surface, in the part's own frame:
    // the host's mesh puts its corners on or inside the surface (measured:
    // under 0.002 cells outside), but between them, where the surface curves
    // inward, its flat triangles stand off it by c²/8R; a tenth of a cell
    // covers that down to curves of a fifth of a cell's radius
    let lift = surf.cell * 0.1 * (1 + depth_of(layer, mark)) as f64;
    let to_local = plane.frame.inverse();
    let (c0, c1, c2) = (plane.frame.rot.col(0), plane.frame.rot.col(1), plane.frame.rot.col(2));
    let flips = c0.dot(c1.cross(c2)) < 0.0;
    let mut mesh = TriMesh { positions: Vec::with_capacity(verts.len()), normals: Vec::with_capacity(verts.len()), indices: Vec::new() };
    let mut world_n: Vec<Vec3> = Vec::with_capacity(verts.len());
    for (hit, flat) in &verts {
        let (p, n) = match hit {
            Some(h) => { let n = surf.normal(*h); (h.add(n.scale(lift)), n) }
            None => (*flat, face),
        };
        world_n.push(n);
        let (lp, ln) = (to_local.apply_point(p), to_local.apply_dir(n));
        mesh.positions.push([lp.x as f32, lp.y as f32, lp.z as f32]);
        mesh.normals.push([ln.x as f32, ln.y as f32, ln.z as f32]);
    }
    for t in kept {
        // wound to face out of the host, in the part's frame
        let w = |i: usize| verts[i].0.unwrap();
        let geo = w(t[1]).sub(w(t[0])).cross(w(t[2]).sub(w(t[0])));
        let out = world_n[t[0]].add(world_n[t[1]]).add(world_n[t[2]]);
        let t = if (geo.dot(out) < 0.0) != flips { [t[0], t[2], t[1]] } else { t };
        mesh.indices.extend(t.iter().map(|&i| i as u32));
    }
    Ok(Drawn { mesh, grid: used, uvs })
}

// ── Images (step 4) ───────────────────────────────────────────────────────

/// What an image file is: its pixel size, its type, and whether it can be
/// see-through (a PNG with an alpha channel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo { pub width: u32, pub height: u32, pub mime: &'static str, pub alpha: bool }

/// Read a PNG's or JPEG's size from its header (no decoding: glTF and OBJ
/// carry the file as it is).
pub fn image_info(bytes: &[u8]) -> Result<ImageInfo, String> {
    if bytes.len() >= 26 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] && &bytes[12..16] == b"IHDR" {
        let be = |i: usize| u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        // color types 4 and 6 carry alpha
        return Ok(ImageInfo { width: be(16), height: be(20), mime: "image/png", alpha: matches!(bytes[25], 4 | 6) });
    }
    if bytes.len() >= 4 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF { i += 1; continue; }
            let m = bytes[i + 1];
            if m == 0xFF || m == 0x01 || (0xD0..=0xD8).contains(&m) { i += 2; continue; }
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            // a start-of-frame marker holds the size
            if matches!(m, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return Ok(ImageInfo { width: w, height: h, mime: "image/jpeg", alpha: false });
            }
            if m == 0xDA { break; }
            i += 2 + len;
        }
        return Err("a JPEG without a size in its header".into());
    }
    Err("not a PNG or JPEG (glTF carries only those two)".into())
}

/// The mark's image, read and checked against its outline: an image is
/// drawn across the outline's box, so the box and the picture must have
/// the same proportions (to 1%), or the error names the height that fits.
/// A band's image wraps it, so a band takes any picture.
pub fn read_image(mark: &Part, base: Option<&std::path::Path>) -> Result<Option<(Vec<u8>, ImageInfo)>, String> {
    let Some(file) = image_of(&mark.shape) else { return Ok(None) };
    let path = match base { Some(b) => b.join(&file), None => std::path::PathBuf::from(&file) };
    let bytes = std::fs::read(&path).map_err(|e| {
        let why = if e.kind() == std::io::ErrorKind::NotFound { "no such file".to_string() } else { e.to_string() };
        let at = if path == std::path::Path::new(&file) { String::new() } else { format!(" (at {})", path.display()) };
        format!("'{}': cannot read its image '{file}'{at}: {why}", mark.name)
    })?;
    let info = image_info(&bytes).map_err(|e| format!("'{}': its image '{file}' is {e}", mark.name))?;
    if let Some((kind, args)) = outline(&mark.shape) {
        if kind != MarkKind::Band && info.height > 0 {
            let (hw, hh) = half_extents(kind, &args);
            let (box_r, img_r) = (hw / hh, info.width as f64 / info.height as f64);
            if (box_r / img_r - 1.0).abs() > 0.01 {
                let fit = crate::mesh::num(((2.0 * hw / img_r) * 100.0).round() / 100.0);
                return Err(format!("'{}': its image '{file}' is {} × {}, but the mark is {} × {}: \
                    `height = {fit}` fits `width = {}` (or crop the picture)", mark.name, info.width, info.height,
                    crate::mesh::num(2.0 * hw), crate::mesh::num(2.0 * hh), crate::mesh::num(2.0 * hw)));
            }
        }
    }
    Ok(Some((bytes, info)))
}

/// The image file a mark's shape names, under any mirror.
pub fn image_of(shape: &Shape) -> Option<String> {
    match shape {
        Shape::Mirror { inner, .. } => image_of(inner),
        Shape::Mark { image, .. } => image.clone(),
        _ => None,
    }
}
