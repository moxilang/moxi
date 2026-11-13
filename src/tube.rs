//! `tube` and `lathe` (Form, stage 2): shapes built from a list of points.
//!
//! `tube(path=[[x, y, z], …], radius=r | [r0, r1, …])` is a smooth tube
//! through every point of `path`. `lathe(profile=[[r, y], …])` revolves a
//! smooth silhouette about +Y, closed to the axis at both ends.
//!
//! Both curves are centripetal Catmull-Rom splines through the given points,
//! with mirrored end points, so two points give a straight line. Each span
//! is sampled at 1, 2, 4, … 64 chords, the fewest whose midpoints stay
//! within a tolerance of the curve; the tolerance depends only on the
//! shape's own numbers, never on the grain, so the mesher, the voxelizer,
//! the raymarcher, the extents and the anchors all see the SAME polyline.
//!
//! The tube is the union of round cones between consecutive samples
//! (radius interpolated by arc length) and the lathe is the revolved
//! polygon of the samples. Both distances are exact Euclidean distances, so
//! the mesher may treat them like spheres and boxes (`mesh::is_lipschitz`,
//! `mesh::exact_outside`).
//!
//! `angle = 0` on a tube that turns: the reference direction starts from a
//! cylinder's (+Z, with +X at 90°), turned by the smallest rotation that
//! takes +Y onto the start tangent, and is carried along the curve without
//! roll (rotation-minimizing frames by double reflection, Wang et al. 2008).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::{Expr, NamedArg};
use crate::frame::Vec3;

/// The most chords per span between two given points.
const MAX_CHORDS: usize = 64;

// ── Reading the arguments ────────────────────────────────────────────────

fn num(e: &Expr) -> Option<f64> {
    match e {
        Expr::Float(f) => Some(*f),
        Expr::Int(n) => Some(*n as f64),
        _ => None,
    }
}

fn arg<'a>(args: &'a [NamedArg], key: &str) -> Option<&'a Expr> {
    args.iter().find(|a| a.key == key).map(|a| &a.value)
}

/// A list of points of `dim` numbers each, at least two of them.
fn points(args: &[NamedArg], key: &str, dim: usize, example: &str) -> Result<Vec<Vec<f64>>, String> {
    let Some(e) = arg(args, key) else {
        return Err(format!("needs `{key}`, a list of at least two points: `{key}={example}`"));
    };
    let Expr::List(items) = e else {
        return Err(format!("`{key}` must be a list of points, e.g. `{key}={example}`"));
    };
    if items.len() < 2 {
        return Err(format!("`{key}` needs at least two points, got {}", items.len()));
    }
    let mut out = Vec::with_capacity(items.len());
    for (i, it) in items.iter().enumerate() {
        let Expr::List(xs) = it else {
            return Err(format!("point {i} of `{key}` must be a list of {dim} numbers, e.g. `{example}`"));
        };
        let v: Option<Vec<f64>> = xs.iter().map(num).collect();
        match v {
            Some(v) if v.len() == dim && v.iter().all(|x| x.is_finite()) => out.push(v),
            _ => return Err(format!(
                "point {i} of `{key}` must be {dim} numbers known when the thing is built, got {} item{}",
                xs.len(), if xs.len() == 1 { "" } else { "s" })),
        }
    }
    for i in 1..out.len() {
        if out[i] == out[i - 1] {
            return Err(format!("points {} and {i} of `{key}` are the same; drop one", i - 1));
        }
    }
    Ok(out)
}

/// `radius`: a number, or a list of numbers spread evenly along the length.
fn radii(args: &[NamedArg]) -> Result<Vec<f64>, String> {
    let rs = match arg(args, "radius") {
        None => vec![0.5],
        Some(Expr::List(xs)) => {
            let v: Option<Vec<f64>> = xs.iter().map(num).collect();
            match v {
                Some(v) if !v.is_empty() => v,
                _ => return Err("`radius` must be a number or a list of numbers, e.g. `radius=[1.2, 0.3]`".into()),
            }
        }
        Some(e) => match num(e) {
            Some(r) => vec![r],
            None => return Err("`radius` must be a number or a list of numbers known when the thing is built".into()),
        },
    };
    if let Some(r) = rs.iter().find(|r| !(r.is_finite() && **r > 0.0)) {
        return Err(format!("every `radius` must be greater than 0, got {r}"));
    }
    Ok(rs)
}

/// Whether a tube has its own `path` (a span's tube has none until the
/// solver gives it one).
pub fn has_path(args: &[NamedArg]) -> bool {
    args.iter().any(|a| a.key == "path")
}

/// The tube's arguments with `path` set to these points (a span's).
pub fn with_path(args: &[NamedArg], path: &[Vec3]) -> Vec<NamedArg> {
    let mut out: Vec<NamedArg> = args.iter().filter(|a| a.key != "path").cloned().collect();
    out.insert(0, NamedArg { key: "path".into(), value: Expr::List(path.iter()
        .map(|p| Expr::List(vec![Expr::Float(p.x), Expr::Float(p.y), Expr::Float(p.z)])).collect()) });
    out
}

/// Check a tube's arguments; the message is the whole complaint.
pub fn check_tube(args: &[NamedArg]) -> Result<(), String> {
    points(args, "path", 3, "[[0, 0, 0], [0, 8, 2]]")?;
    radii(args)?;
    Ok(())
}

/// Check a lathe's arguments.
pub fn check_lathe(args: &[NamedArg]) -> Result<(), String> {
    let p = points(args, "profile", 2, "[[3, 0], [4, 5], [1.5, 9]]")?;
    if let Some((i, _)) = p.iter().enumerate().find(|(_, q)| q[0] < 0.0) {
        return Err(format!("point {i} of `profile` has a negative radius; a profile is [radius, height] with radius ≥ 0"));
    }
    if p[p.len() - 1][1] <= p[0][1] {
        return Err("a `profile` runs from bottom to top: its last height must be above its first".into());
    }
    Ok(())
}

// ── The curve ────────────────────────────────────────────────────────────

/// One centripetal Catmull-Rom span as a cubic Hermite curve on u ∈ [0, 1].
struct Span { p1: Vec3, p2: Vec3, m1: Vec3, m2: Vec3 }

impl Span {
    fn at(&self, u: f64) -> Vec3 {
        let (u2, u3) = (u * u, u * u * u);
        let h00 = 2.0 * u3 - 3.0 * u2 + 1.0;
        let h10 = u3 - 2.0 * u2 + u;
        let h01 = -2.0 * u3 + 3.0 * u2;
        let h11 = u3 - u2;
        self.p1.scale(h00).add(self.m1.scale(h10)).add(self.p2.scale(h01)).add(self.m2.scale(h11))
    }
    fn tangent(&self, u: f64) -> Vec3 {
        let u2 = u * u;
        let d00 = 6.0 * u2 - 6.0 * u;
        let d10 = 3.0 * u2 - 4.0 * u + 1.0;
        let d01 = -6.0 * u2 + 6.0 * u;
        let d11 = 3.0 * u2 - 2.0 * u;
        self.p1.scale(d00).add(self.m1.scale(d10)).add(self.p2.scale(d01)).add(self.m2.scale(d11))
    }
}

fn spans(pts: &[Vec3]) -> Vec<Span> {
    let n = pts.len();
    let get = |i: isize| -> Vec3 {
        if i < 0 { pts[0].scale(2.0).sub(pts[1]) }
        else if i as usize >= n { pts[n - 1].scale(2.0).sub(pts[n - 2]) }
        else { pts[i as usize] }
    };
    (0..n - 1).map(|i| {
        let i = i as isize;
        let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
        // centripetal knots: t_{k+1} = t_k + |P_{k+1} − P_k|^½
        let dt = |a: Vec3, b: Vec3| b.sub(a).length().sqrt().max(1e-12);
        let (d0, d1, d2) = (dt(p0, p1), dt(p1, p2), dt(p2, p3));
        let m1 = p1.sub(p0).scale(1.0 / d0).sub(p2.sub(p0).scale(1.0 / (d0 + d1))).add(p2.sub(p1).scale(1.0 / d1)).scale(d1);
        let m2 = p2.sub(p1).scale(1.0 / d1).sub(p3.sub(p1).scale(1.0 / (d1 + d2))).add(p3.sub(p2).scale(1.0 / d2)).scale(d1);
        Span { p1, p2, m1, m2 }
    }).collect()
}

/// Distance from `p` to the line segment `[a, b]`.
fn to_segment(p: Vec3, a: Vec3, b: Vec3) -> f64 {
    let ab = b.sub(a);
    let l2 = ab.dot(ab);
    let t = if l2 > 0.0 { (p.sub(a).dot(ab) / l2).clamp(0.0, 1.0) } else { 0.0 };
    p.sub(a.add(ab.scale(t))).length()
}

/// Sample the curve: the given points are always samples; each span gets
/// the fewest chords (a power of two, ≤ 64) whose midpoints are within
/// `tol` of the curve. Returns (points, tangents).
fn sample(pts: &[Vec3], tol: f64) -> (Vec<Vec3>, Vec<Vec3>) {
    sample_with(pts, tol, -1.0, f64::MAX)
}

/// `sample`, with two more limits on every chord: the curve turns by no
/// more than the angle whose cosine is `min_turn_cos` across it, and it is
/// no longer than `max_len`. A tube needs both: its surface is a chain of
/// round cones, and the normal jumps where two meet by the angle between
/// them, which shading shows as a ring long before the position is off.
fn sample_with(pts: &[Vec3], tol: f64, min_turn_cos: f64, max_len: f64) -> (Vec<Vec3>, Vec<Vec3>) {
    let mut out = vec![pts[0]];
    let mut tan = Vec::new();
    for (k, s) in spans(pts).iter().enumerate() {
        let mut n = 1;
        while n < MAX_CHORDS {
            let ok = (0..n).all(|i| {
                let (u0, u1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
                let (a, b) = (s.at(u0), s.at(u1));
                let turn = match (s.tangent(u0).normalize(), s.tangent(u1).normalize()) {
                    (Some(t0), Some(t1)) => t0.dot(t1),
                    _ => 1.0,
                };
                to_segment(s.at(0.5 * (u0 + u1)), a, b) <= tol
                    && turn >= min_turn_cos
                    && b.sub(a).length() <= max_len
            });
            if ok { break; }
            n *= 2;
        }
        if k == 0 { tan.push(s.tangent(0.0)); }
        for i in 1..=n {
            let u = i as f64 / n as f64;
            // the given points are hit exactly, not up to rounding
            out.push(if i == n { s.p2 } else { s.at(u) });
            tan.push(s.tangent(u));
        }
    }
    // a zero tangent (a cusp) takes its chord's direction
    let m = out.len();
    let tan = (0..m).map(|i| {
        let chord = if i + 1 < m { out[i + 1].sub(out[i]) } else { out[i].sub(out[i - 1]) };
        tan[i].normalize().or_else(|| chord.normalize()).unwrap_or(Vec3::Y)
    }).collect();
    (out, tan)
}

fn arclength(pts: &[Vec3]) -> Vec<f64> {
    let mut s = vec![0.0];
    for i in 1..pts.len() { s.push(s[i - 1] + pts[i].sub(pts[i - 1]).length()); }
    s
}

/// Value at arc-length fraction `f` of evenly spread control values: a
/// monotone cubic through them (Fritsch–Butland tangents, as in PCHIP), so
/// the radius has no kink at a control value (a kink is a ring in the
/// shading) and stays between the two values it runs between (no bulge
/// past a value written, never below the smallest). Two values give a
/// straight taper.
fn spread_smooth(vals: &[f64], f: f64) -> f64 {
    let n = vals.len();
    if n == 1 { return vals[0]; }
    let x = f.clamp(0.0, 1.0) * (n - 1) as f64;
    let i = (x.floor() as usize).min(n - 2);
    let t = x - i as f64;
    let d = |k: usize| vals[k + 1] - vals[k];
    // slope at control value k, per step between values: the ends take
    // their one step's; inside, the harmonic mean of the two steps when
    // they agree in sign (at most twice the smaller, so the cubic between
    // them is monotone) and flat at a peak or a valley
    let m = |k: usize| -> f64 {
        if k == 0 { return d(0); }
        if k == n - 1 { return d(n - 2); }
        let (a, b) = (d(k - 1), d(k));
        if a * b <= 0.0 { 0.0 } else { 2.0 * a * b / (a + b) }
    };
    let (m0, m1) = (m(i), m(i + 1));
    let (t2, t3) = (t * t, t * t * t);
    vals[i] * (2.0 * t3 - 3.0 * t2 + 1.0) + m0 * (t3 - 2.0 * t2 + t)
        + vals[i + 1] * (-2.0 * t3 + 3.0 * t2) + m1 * (t3 - t2)
}

/// Rodrigues: the rotation by the smallest angle taking unit `a` to unit `b`.
fn min_rotation(a: Vec3, b: Vec3, v: Vec3) -> Vec3 {
    let c = a.dot(b);
    if c > 1.0 - 1e-12 { return v; }
    if c < -1.0 + 1e-12 {
        // half turn about an axis perpendicular to a (X for a = ±Y)
        let k = a.cross(Vec3::X).normalize().unwrap_or(Vec3::Z).cross(a).normalize().unwrap_or(Vec3::X);
        return k.scale(2.0 * k.dot(v)).sub(v);
    }
    let k = a.cross(b);
    let s = k.length();
    let k = k.scale(1.0 / s);
    v.scale(c).add(k.cross(v).scale(s)).add(k.scale(k.dot(v) * (1.0 - c)))
}

// ── Tube ─────────────────────────────────────────────────────────────────

/// A sampled tube: the polyline, its radii, rotation-minimizing frames and
/// a bounding box per round-cone segment.
#[derive(Debug)]
pub struct Tube {
    pub pts:  Vec<Vec3>,
    pub rad:  Vec<f64>,
    /// arc length at each sample
    pub s:    Vec<f64>,
    pub tan:  Vec<Vec3>,
    /// the `angle = 0` direction, carried along without roll
    pub nrm:  Vec<Vec3>,
    boxes:    Vec<(Vec3, Vec3)>,
    pub lo:   Vec3,
    pub hi:   Vec3,
}

impl Tube {
    pub fn new(args: &[NamedArg]) -> Result<Tube, String> {
        let path: Vec<Vec3> = points(args, "path", 3, "[[0, 0, 0], [0, 8, 2]]")?
            .into_iter().map(|v| Vec3::new(v[0], v[1], v[2])).collect();
        let rs = radii(args)?;
        let scale = path.iter().fold(0.0f64, |m, p| m.max(p.sub(path[0]).length()));
        let rmin = rs.iter().cloned().fold(f64::MAX, f64::min);
        let tol = (0.05 * rmin).min(0.005 * scale.max(rmin));
        // chords short enough that the radius's curve between two control
        // values spans 16 of them (the given path is no longer than the
        // curve, so this is if anything shorter), and turning ≤ 2.5° each
        let polyline: f64 = path.windows(2).map(|w| w[1].sub(w[0]).length()).sum();
        let max_len = if rs.len() > 1 && rs.iter().any(|r| *r != rs[0]) {
            polyline / (16 * (rs.len() - 1)) as f64
        } else { f64::MAX };
        let (pts, tan) = sample_with(&path, tol, 2.5f64.to_radians().cos(), max_len);
        let s = arclength(&pts);
        let len = s[s.len() - 1];
        let rad: Vec<f64> = s.iter().map(|&si| spread_smooth(&rs, if len > 0.0 { si / len } else { 0.0 })).collect();

        // rotation-minimizing frames by double reflection
        let mut nrm = vec![min_rotation(Vec3::Y, tan[0], Vec3::Z)];
        for i in 0..pts.len() - 1 {
            let v1 = pts[i + 1].sub(pts[i]);
            let c1 = v1.dot(v1);
            let r = nrm[i];
            let (rl, tl) = if c1 > 0.0 {
                (r.sub(v1.scale(2.0 / c1 * v1.dot(r))), tan[i].sub(v1.scale(2.0 / c1 * v1.dot(tan[i]))))
            } else { (r, tan[i]) };
            let v2 = tan[i + 1].sub(tl);
            let c2 = v2.dot(v2);
            let next = if c2 > 1e-24 { rl.sub(v2.scale(2.0 / c2 * v2.dot(rl))) } else { rl };
            // re-orthogonalize against rounding drift
            let t = tan[i + 1];
            nrm.push(next.sub(t.scale(next.dot(t))).normalize().unwrap_or(next));
        }

        let boxes: Vec<(Vec3, Vec3)> = (0..pts.len() - 1).map(|i| {
            let (a, b, r) = (pts[i], pts[i + 1], rad[i].max(rad[i + 1]));
            (Vec3::new(a.x.min(b.x) - r, a.y.min(b.y) - r, a.z.min(b.z) - r),
             Vec3::new(a.x.max(b.x) + r, a.y.max(b.y) + r, a.z.max(b.z) + r))
        }).collect();
        let lo = boxes.iter().fold(Vec3::new(f64::MAX, f64::MAX, f64::MAX), |m, b| Vec3::new(m.x.min(b.0.x), m.y.min(b.0.y), m.z.min(b.0.z)));
        let hi = boxes.iter().fold(Vec3::new(f64::MIN, f64::MIN, f64::MIN), |m, b| Vec3::new(m.x.max(b.1.x), m.y.max(b.1.y), m.z.max(b.1.z)));
        Ok(Tube { pts, rad, s, tan, nrm, boxes, lo, hi })
    }

    pub fn len(&self) -> f64 { self.s[self.s.len() - 1] }

    /// Exact distance to the union of round cones. A segment whose box is
    /// farther than the best so far cannot be nearer, so it is skipped. The
    /// box distance is never negative: once `p` is inside a cone (best < 0)
    /// only boxes that hold `p` can go deeper, and those must not be skipped.
    pub fn distance(&self, p: Vec3) -> f64 {
        let mut best = f64::MAX;
        for (i, b) in self.boxes.iter().enumerate() {
            let gap = |v: f64, a: f64, c: f64| (a - v).max(v - c).max(0.0);
            let bd = Vec3::new(gap(p.x, b.0.x, b.1.x), gap(p.y, b.0.y, b.1.y), gap(p.z, b.0.z, b.1.z)).length();
            if bd > best.max(0.0) { continue; }
            best = best.min(round_cone(p, self.pts[i], self.pts[i + 1], self.rad[i], self.rad[i + 1]));
        }
        best
    }

    /// The point at arc-length fraction `t`: (position, tangent, angle-0
    /// direction, radius, d radius / d length).
    pub fn at(&self, t: f64) -> (Vec3, Vec3, Vec3, f64, f64) {
        let target = t.clamp(0.0, 1.0) * self.len();
        let m = self.pts.len();
        let i = (0..m - 1).find(|&i| self.s[i + 1] >= target).unwrap_or(m - 2);
        let seg = self.s[i + 1] - self.s[i];
        let u = if seg > 0.0 { ((target - self.s[i]) / seg).clamp(0.0, 1.0) } else { 0.0 };
        let pos = self.pts[i].add(self.pts[i + 1].sub(self.pts[i]).scale(u));
        let dir = self.pts[i + 1].sub(self.pts[i]).normalize().unwrap_or(self.tan[i]);
        // the frame is the segment's: its direction, and the transported
        // reference projected off it
        let n0 = self.nrm[i].scale(1.0 - u).add(self.nrm[i + 1].scale(u));
        let nrm = n0.sub(dir.scale(n0.dot(dir))).normalize().unwrap_or(self.nrm[i]);
        let r = self.rad[i] * (1.0 - u) + self.rad[i + 1] * u;
        let dr = if seg > 0.0 { (self.rad[i + 1] - self.rad[i]) / seg } else { 0.0 };
        (pos, dir, nrm, r, dr)
    }
}

fn sgn(x: f64) -> f64 { if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 } }

/// Exact distance to the round cone joining the sphere (a, r1) to (b, r2)
/// (Quilez). When one sphere holds the other, the bigger sphere.
pub fn round_cone(p: Vec3, a: Vec3, b: Vec3, r1: f64, r2: f64) -> f64 {
    let ba = b.sub(a);
    let l2 = ba.dot(ba);
    let rr = r1 - r2;
    let a2 = l2 - rr * rr;
    if a2 <= 0.0 {
        return (p.sub(a).length() - r1).min(p.sub(b).length() - r2);
    }
    let il2 = 1.0 / l2;
    let pa = p.sub(a);
    let y = pa.dot(ba);
    let z = y - l2;
    let w = pa.scale(l2).sub(ba.scale(y));
    let x2 = w.dot(w);
    let y2 = y * y * l2;
    let z2 = z * z * l2;
    let k = sgn(rr) * rr * rr * x2;
    if sgn(z) * a2 * z2 > k { return (x2 + z2).sqrt() * il2 - r2; }
    if sgn(y) * a2 * y2 < k { return (x2 + y2).sqrt() * il2 - r1; }
    ((x2 * a2 * il2).sqrt() + y * rr) * il2 - r1
}

// ── Lathe ────────────────────────────────────────────────────────────────

/// A sampled lathe profile, in the (radius, height) half-plane.
#[derive(Debug)]
pub struct Lathe {
    /// the profile samples, bottom to top, radius ≥ 0
    pub pts: Vec<(f64, f64)>,
    /// arc length along the profile at each sample
    pub s:   Vec<f64>,
    pub rmax: f64,
    pub ylo:  f64,
    pub yhi:  f64,
}

impl Lathe {
    pub fn new(args: &[NamedArg]) -> Result<Lathe, String> {
        check_lathe(args)?;
        let prof: Vec<Vec3> = points(args, "profile", 2, "")?
            .into_iter().map(|v| Vec3::new(v[0], v[1], 0.0)).collect();
        let scale = prof.iter().fold(0.0f64, |m, p| m.max(p.sub(prof[0]).length()));
        let (smp, _) = sample(&prof, 0.002 * scale);
        let pts: Vec<(f64, f64)> = smp.iter().map(|p| (p.x.max(0.0), p.y)).collect();
        let as3: Vec<Vec3> = pts.iter().map(|&(r, y)| Vec3::new(r, y, 0.0)).collect();
        let s = arclength(&as3);
        let rmax = pts.iter().fold(0.0f64, |m, p| m.max(p.0));
        let ylo = pts.iter().fold(f64::MAX, |m, p| m.min(p.1));
        let yhi = pts.iter().fold(f64::MIN, |m, p| m.max(p.1));
        Ok(Lathe { pts, s, rmax, ylo, yhi })
    }

    /// The closed polygon: the axis point under the first sample, the
    /// samples, the axis point over the last.
    pub(crate) fn polygon(&self) -> Vec<(f64, f64)> {
        let mut v = Vec::with_capacity(self.pts.len() + 2);
        v.push((0.0, self.pts[0].1));
        v.extend(self.pts.iter().copied());
        v.push((0.0, self.pts[self.pts.len() - 1].1));
        v
    }

    /// Exact distance: distance to every edge except the closing one on
    /// the axis (it is inside the solid), signed by the even-odd rule on
    /// the whole polygon.
    pub fn distance(&self, p: Vec3) -> f64 {
        let q = ((p.x * p.x + p.z * p.z).sqrt(), p.y);
        let v = self.polygon();
        let n = v.len();
        let mut d2 = f64::MAX;
        let mut inside = false;
        for i in 0..n {
            let (a, b) = (v[i], v[(i + 1) % n]);
            if i + 1 < n {
                let e = (b.0 - a.0, b.1 - a.1);
                let w = (q.0 - a.0, q.1 - a.1);
                let l2 = e.0 * e.0 + e.1 * e.1;
                let t = if l2 > 0.0 { ((w.0 * e.0 + w.1 * e.1) / l2).clamp(0.0, 1.0) } else { 0.0 };
                let (dx, dy) = (w.0 - e.0 * t, w.1 - e.1 * t);
                d2 = d2.min(dx * dx + dy * dy);
            }
            // even-odd crossing test on a horizontal ray toward +r
            if (a.1 > q.1) != (b.1 > q.1) {
                let x = a.0 + (q.1 - a.1) / (b.1 - a.1) * (b.0 - a.0);
                if q.0 < x { inside = !inside; }
            }
        }
        if inside { -d2.sqrt() } else { d2.sqrt() }
    }

    /// The profile point at arc-length fraction `t`: ((r, y), outward
    /// normal (nr, ny)).
    pub fn at(&self, t: f64) -> ((f64, f64), (f64, f64)) {
        let len = self.s[self.s.len() - 1];
        let target = t.clamp(0.0, 1.0) * len;
        let m = self.pts.len();
        let i = (0..m - 1).find(|&i| self.s[i + 1] >= target).unwrap_or(m - 2);
        let seg = self.s[i + 1] - self.s[i];
        let u = if seg > 0.0 { ((target - self.s[i]) / seg).clamp(0.0, 1.0) } else { 0.0 };
        let (a, b) = (self.pts[i], self.pts[i + 1]);
        let pos = (a.0 + (b.0 - a.0) * u, a.1 + (b.1 - a.1) * u);
        // going up the profile with the solid toward the axis, the outward
        // normal is the tangent turned clockwise: (dy, −dr)
        let (dr, dy) = (b.0 - a.0, b.1 - a.1);
        let l = (dr * dr + dy * dy).sqrt().max(1e-300);
        (pos, (dy / l, -dr / l))
    }
}

// ── A small cache: the mesher asks for the same tube millions of times ─────

thread_local! {
    static TUBES:  RefCell<HashMap<Vec<u64>, Rc<Result<Tube, String>>>>  = RefCell::new(HashMap::new());
    static LATHES: RefCell<HashMap<Vec<u64>, Rc<Result<Lathe, String>>>> = RefCell::new(HashMap::new());
}

/// Every number of the arguments, by bit pattern, with the structure
/// marked: the cache key. Equal keys mean equal arguments.
fn key(args: &[NamedArg]) -> Vec<u64> {
    fn walk(e: &Expr, out: &mut Vec<u64>) {
        match e {
            Expr::List(xs) => {
                out.push(u64::MAX - 1);
                for x in xs { walk(x, out); }
                out.push(u64::MAX - 2);
            }
            other => match num(other) {
                Some(v) => out.push((v + 0.0).to_bits()),
                None => out.push(u64::MAX - 3),
            },
        }
    }
    let mut out = Vec::new();
    for a in args {
        out.push(u64::MAX);
        out.extend(a.key.bytes().map(u64::from));
        walk(&a.value, &mut out);
    }
    out
}

pub fn tube(args: &[NamedArg]) -> Rc<Result<Tube, String>> {
    let k = key(args);
    TUBES.with(|c| {
        if let Some(t) = c.borrow().get(&k) { return t.clone(); }
        let t = Rc::new(Tube::new(args));
        let mut m = c.borrow_mut();
        if m.len() > 256 { m.clear(); }
        m.insert(k, t.clone());
        t
    })
}

pub fn lathe(args: &[NamedArg]) -> Rc<Result<Lathe, String>> {
    let k = key(args);
    LATHES.with(|c| {
        if let Some(t) = c.borrow().get(&k) { return t.clone(); }
        let t = Rc::new(Lathe::new(args));
        let mut m = c.borrow_mut();
        if m.len() > 256 { m.clear(); }
        m.insert(k, t.clone());
        t
    })
}

/// Distance for `geometry::distance`; a malformed shape is empty (the
/// resolver has already reported it).
pub fn tube_distance(args: &[NamedArg], p: Vec3) -> f64 {
    match &*tube(args) { Ok(t) => t.distance(p), Err(_) => f64::MAX }
}

pub fn lathe_distance(args: &[NamedArg], p: Vec3) -> f64 {
    match &*lathe(args) { Ok(l) => l.distance(p), Err(_) => f64::MAX }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchors::resolve_anchor;
    use crate::ast::ShapeExpr;

    fn f(v: f64) -> Expr { Expr::Float(v) }
    fn pts(ps: &[&[f64]]) -> Expr { Expr::List(ps.iter().map(|p| Expr::List(p.iter().map(|v| f(*v)).collect())).collect()) }
    fn na(k: &str, v: Expr) -> NamedArg { NamedArg { key: k.into(), value: v } }
    fn tube_args(path: &[&[f64]], radius: Expr) -> Vec<NamedArg> { vec![na("path", pts(path)), na("radius", radius)] }
    fn lcg(seed: &mut u64) -> f64 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (*seed >> 11) as f64 / (1u64 << 53) as f64
    }


    #[test]
    fn radius_curve_hits_its_values_stays_between_them_and_has_no_kink() {
        assert_eq!(spread_smooth(&[2.0], 0.3), 2.0);
        // two values: a straight taper
        for k in 0..=10 {
            let f = k as f64 / 10.0;
            assert!((spread_smooth(&[1.2, 0.3], f) - (1.2 - 0.9 * f)).abs() < 1e-12);
        }
        let mut seed = 11;
        for _ in 0..300 {
            let n = 2 + (lcg(&mut seed) * 6.0) as usize;
            let v: Vec<f64> = (0..n).map(|_| 0.1 + lcg(&mut seed) * 5.0).collect();
            let step = 1.0 / (n - 1) as f64;
            for (k, vk) in v.iter().enumerate() {
                assert!((spread_smooth(&v, k as f64 * step) - vk).abs() < 1e-12, "{v:?} at {k}");
            }
            for j in 0..=2000 {
                let f = j as f64 / 2000.0;
                let i = ((f / step).floor() as usize).min(n - 2);
                let (lo, hi) = (v[i].min(v[i + 1]), v[i].max(v[i + 1]));
                let r = spread_smooth(&v, f);
                assert!(r >= lo - 1e-12 && r <= hi + 1e-12, "{v:?}: {r} at {f} outside [{lo}, {hi}]");
            }
            // one slope on both sides of every inner value
            let h = 1e-7;
            for k in 1..n - 1 {
                let f = k as f64 * step;
                let left = (spread_smooth(&v, f) - spread_smooth(&v, f - h)) / h;
                let right = (spread_smooth(&v, f + h) - spread_smooth(&v, f)) / h;
                assert!((left - right).abs() < 1e-4 * (1.0 + left.abs()), "{v:?}: kink at {k}: {left} vs {right}");
            }
        }
    }

    #[test]
    fn a_straight_tube_swells_where_its_radius_says() {
        // two path points are one chord unless the radius has a shape
        let rl = Expr::List([1.0, 1.6, 0.7].iter().map(|v| f(*v)).collect());
        let t = Tube::new(&tube_args(&[&[0.0, 0.0, 0.0], &[0.0, 10.0, 0.0]], rl)).unwrap();
        assert!(t.pts.len() > 16, "{} samples", t.pts.len());
        assert!(t.distance(Vec3::new(1.6, 5.0, 0.0)).abs() < 1e-3, "{}", t.distance(Vec3::new(1.6, 5.0, 0.0)));
        assert!(t.distance(Vec3::new(0.0, 5.0, 1.6)).abs() < 1e-3);
        // the ends are the values written
        assert!((t.rad[0] - 1.0).abs() < 1e-12 && (t.rad[t.rad.len() - 1] - 0.7).abs() < 1e-12);
    }

    #[test]
    fn a_tube_has_no_rings() {
        // where two round cones meet, the surface normal turns by the angle
        // between their sides; keep it under 3° along a bent, swelling limb
        let rl = Expr::List([3.9, 3.2, 3.5, 4.5, 3.6].iter().map(|v| f(*v)).collect());
        let t = Tube::new(&tube_args(&[&[10.0, 0.0, 0.0], &[16.0, 6.5, 1.5], &[13.5, 16.0, 2.5]], rl)).unwrap();
        let side = |i: usize| {
            let l = t.pts[i + 1].sub(t.pts[i]).length();
            (t.rad[i + 1] - t.rad[i]) / l
        };
        let mut worst: f64 = 0.0;
        for i in 0..t.pts.len() - 2 {
            let (d0, d1) = (t.pts[i + 1].sub(t.pts[i]).normalize().unwrap(), t.pts[i + 2].sub(t.pts[i + 1]).normalize().unwrap());
            let bend = d0.dot(d1).clamp(-1.0, 1.0).acos();
            let slope = (side(i + 1).atan() - side(i).atan()).abs();
            worst = worst.max(bend + slope);
        }
        assert!(worst.to_degrees() < 3.0, "worst normal jump {}°", worst.to_degrees());
    }


    #[test]
    fn inside_a_tube_the_distance_is_the_deepest_cone() {
        // the box skip must not stop at the first cone that holds the point
        let rl = Expr::List([3.9, 3.2, 3.5, 4.5, 3.6].iter().map(|v| f(*v)).collect());
        let t = Tube::new(&tube_args(&[&[10.0, 0.0, 0.0], &[16.0, 6.5, 1.5], &[13.5, 16.0, 2.5]], rl)).unwrap();
        let mut seed = 5;
        for _ in 0..3000 {
            let p = Vec3::new(4.0 + lcg(&mut seed) * 18.0, -5.0 + lcg(&mut seed) * 26.0, -5.0 + lcg(&mut seed) * 12.0);
            let brute = (0..t.pts.len() - 1).map(|i| round_cone(p, t.pts[i], t.pts[i + 1], t.rad[i], t.rad[i + 1])).fold(f64::MAX, f64::min);
            assert_eq!(t.distance(p), brute, "at {p:?}");
        }
    }

    #[test]
    fn round_cone_is_the_distance_to_its_swept_spheres_outside() {
        // the convex hull of two spheres is the union of the spheres swept
        // between them with the radius interpolated, so outside it the
        // distance is min over s of |p − c(s)| − r(s)
        let (a, b, r1, r2) = (Vec3::new(1.0, -2.0, 0.5), Vec3::new(4.0, 3.0, -1.0), 1.5, 0.4);
        let mut seed = 3;
        let mut checked = 0;
        for _ in 0..4000 {
            let p = Vec3::new(lcg(&mut seed) * 16.0 - 6.0, lcg(&mut seed) * 16.0 - 8.0, lcg(&mut seed) * 12.0 - 6.0);
            let brute = (0..=20_000).map(|i| {
                let s = i as f64 / 20_000.0;
                p.sub(a.add(b.sub(a).scale(s))).length() - (r1 + (r2 - r1) * s)
            }).fold(f64::MAX, f64::min);
            let d = round_cone(p, a, b, r1, r2);
            if brute > 0.0 {
                assert!((d - brute).abs() < 1e-5, "at {p:?}: {d} vs {brute}");
                checked += 1;
            } else {
                assert!(d <= 1e-9, "inside points are inside");
            }
        }
        assert!(checked > 3000);
    }

    #[test]
    fn a_tube_passes_through_its_points_and_tapers() {
        let args = tube_args(&[&[0.0, 0.0, 0.0], &[0.0, 8.0, 2.0], &[3.0, 14.0, 6.0]], Expr::List(vec![f(1.2), f(0.3)]));
        let t = Tube::new(&args).unwrap();
        assert_eq!(t.pts[0], Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(*t.pts.last().unwrap(), Vec3::new(3.0, 14.0, 6.0));
        assert!(t.pts.contains(&Vec3::new(0.0, 8.0, 2.0)), "every given point is a sample");
        assert!((t.distance(Vec3::new(0.0, 0.0, 0.0)) + 1.2).abs() < 1e-9, "the start is 1.2 deep");
        assert!((t.distance(Vec3::new(3.0, 14.0, 6.0)) + 0.3).abs() < 1e-9, "the end is 0.3 deep");
        // the curve's polyline stays within its tolerance of the true curve
        for s in spans(&[Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 8.0, 2.0), Vec3::new(3.0, 14.0, 6.0)]) {
            for i in 0..=100 {
                let c = s.at(i as f64 / 100.0);
                let near = (0..t.pts.len() - 1).map(|k| to_segment(c, t.pts[k], t.pts[k + 1])).fold(f64::MAX, f64::min);
                assert!(near < 0.05 * 0.3 + 1e-9, "the samples follow the curve: {near}");
            }
        }
        // two points: a straight round cone
        let two = Tube::new(&tube_args(&[&[0.0, 0.0, 0.0], &[0.0, 10.0, 0.0]], f(1.0))).unwrap();
        assert_eq!(two.pts.len(), 2);
        assert!((two.distance(Vec3::new(3.0, 5.0, 0.0)) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn side_on_a_quarter_circle_is_pinned() {
        // a quarter circle of radius 10 in the XY plane; a plane curve's
        // rotation-minimizing frame keeps the plane's normal, so angle 0
        // stays +Z the whole way
        let c = std::f64::consts::FRAC_1_SQRT_2 * 10.0;
        let args = tube_args(&[&[10.0, 0.0, 0.0], &[c, c, 0.0], &[0.0, 10.0, 0.0]], f(1.0));
        let shape = ShapeExpr::Tube { args };
        let side = |t: f64, angle: f64| resolve_anchor(&shape, "side", &[na("t", f(t)), na("angle", f(angle))]).unwrap().frame;
        let a = side(1.0, 0.0);
        assert!(a.pos.sub(Vec3::new(0.0, 10.0, 1.0)).length() < 1e-9, "side(t=1, angle=0) = {:?}", a.pos);
        assert!(a.rot.col(1).sub(Vec3::Z).length() < 1e-9, "its normal is +Z");
        // angle 90 is direction × (angle-0 direction): with the last
        // chord's direction, exactly; outward from the bend, roughly +Y
        let t = Tube::new(&match &shape { ShapeExpr::Tube { args } => args.clone(), _ => unreachable!() }).unwrap();
        let n = t.pts.len();
        let dir = t.pts[n - 1].sub(t.pts[n - 2]).normalize().unwrap();
        let b = side(1.0, 90.0);
        assert!(b.pos.sub(Vec3::new(0.0, 10.0, 0.0).add(dir.cross(Vec3::Z))).length() < 1e-9, "{:?}", b.pos);
        assert!(b.pos.y > 10.9 && b.pos.x.abs() < 0.4, "outward from the bend: {:?}", b.pos);
        let s0 = side(0.0, 0.0);
        assert!(s0.pos.sub(Vec3::new(10.0, 0.0, 1.0)).length() < 1e-9, "and starts at +Z too: {:?}", s0.pos);
    }

    #[test]
    fn every_tube_anchor_lies_on_its_surface() {
        let args = tube_args(&[&[0.0, 0.0, 0.0], &[2.0, 6.0, -1.0], &[-1.0, 11.0, 3.0], &[4.0, 15.0, 5.0]],
                             Expr::List(vec![f(1.4), f(0.9), f(0.35)]));
        let t = Tube::new(&args).unwrap();
        let shape = ShapeExpr::Tube { args };
        let mut seed = 11;
        for _ in 0..300 {
            let (tt, ang) = (lcg(&mut seed), lcg(&mut seed) * 360.0 - 180.0);
            let a = resolve_anchor(&shape, "side", &[na("t", f(tt)), na("angle", f(ang))]).unwrap().frame;
            // exactly on its own round cone; on the inside of a bend the
            // next cone may cover it, by less than the sampling tolerance
            assert!(t.distance(a.pos) <= 1e-9 && t.distance(a.pos) > -0.05 * 0.35, "side({tt}, {ang}) is {} off the surface", t.distance(a.pos));
            // and its normal points out: a step along it leaves the tube
            assert!(t.distance(a.pos.add(a.rot.col(1).scale(0.05))) > 0.04);
        }
        for end in ["start", "end"] {
            let a = resolve_anchor(&shape, end, &[]).unwrap().frame;
            assert!(t.distance(a.pos).abs() < 1e-9, "{end} is on the cap");
            assert!(t.distance(a.pos.add(a.rot.col(1).scale(0.1))) > 0.099, "{end} points out");
        }
    }

    #[test]
    fn a_straight_lathe_is_exactly_a_cylinder() {
        let args = vec![na("profile", pts(&[&[2.0, 0.0], &[2.0, 5.0]]))];
        let l = Lathe::new(&args).unwrap();
        let cyl = ShapeExpr::Cylinder { args: vec![na("height", f(5.0)), na("radius", f(2.0))] };
        let mut seed = 5;
        for _ in 0..5000 {
            let p = Vec3::new(lcg(&mut seed) * 10.0 - 5.0, lcg(&mut seed) * 11.0 - 3.0, lcg(&mut seed) * 10.0 - 5.0);
            let want = crate::geometry::distance(&cyl, p, 1.0);
            assert!((l.distance(p) - want).abs() < 1e-9, "at {p:?}: {} vs {want}", l.distance(p));
        }
    }

    #[test]
    fn every_lathe_anchor_lies_on_its_surface() {
        let args = vec![na("profile", pts(&[&[3.0, 0.0], &[4.2, 2.5], &[3.1, 6.0], &[2.0, 8.0], &[2.4, 9.0]]))];
        let l = Lathe::new(&args).unwrap();
        let shape = ShapeExpr::Lathe { args };
        let mut seed = 2;
        for _ in 0..300 {
            let (tt, ang) = (lcg(&mut seed), lcg(&mut seed) * 360.0);
            let a = resolve_anchor(&shape, "side", &[na("t", f(tt)), na("angle", f(ang))]).unwrap().frame;
            assert!(l.distance(a.pos).abs() < 1e-9, "side({tt}, {ang})");
            assert!(l.distance(a.pos.add(a.rot.col(1).scale(0.05))) > 0.0, "points out");
        }
        let top = resolve_anchor(&shape, "top", &[]).unwrap().frame.pos;
        assert_eq!((top.x, top.y, top.z), (0.0, 9.0, 0.0));
        let rim = resolve_anchor(&shape, "rim_bottom", &[na("angle", f(90.0))]).unwrap().frame.pos;
        assert!(rim.sub(Vec3::new(3.0, 0.0, 0.0)).length() < 1e-12, "{rim:?}");
        // extents hold the whole solid: the widest sample is wider than 4.2
        let e = crate::anchors::analytic_extents(&shape);
        assert!(e.max.x >= 4.2 && e.min.y == 0.0 && e.max.y == 9.0);
    }

    #[test]
    fn bad_arguments_say_what_is_wrong() {
        let e = |args: Vec<NamedArg>| check_tube(&args).unwrap_err();
        assert!(e(vec![]).contains("needs `path`"));
        assert!(e(vec![na("path", pts(&[&[0.0, 0.0, 0.0]]))]).contains("at least two points"));
        assert!(e(vec![na("path", pts(&[&[0.0, 0.0], &[1.0, 1.0]]))]).contains("must be 3 numbers"));
        assert!(e(tube_args(&[&[0.0, 0.0, 0.0], &[0.0, 0.0, 0.0]], f(1.0))).contains("are the same"));
        assert!(e(tube_args(&[&[0.0, 0.0, 0.0], &[0.0, 1.0, 0.0]], f(0.0))).contains("greater than 0"));
        let l = |p: &[&[f64]]| check_lathe(&[na("profile", pts(p))]).unwrap_err();
        assert!(l(&[&[1.0, 5.0], &[1.0, 2.0]]).contains("bottom to top"));
        assert!(l(&[&[-1.0, 0.0], &[1.0, 2.0]]).contains("negative radius"));
    }
}
