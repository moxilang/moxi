//! `moxi report` — the scene report.
//!
//! ISSUE-20261007-moxi-report, DOC-20261007-aim-facing-report §5.
//!
//! A report a model compares against the prompt: for every printed thing, its
//! bounding box, one row per part (parent, center, size, where the part's own
//! `top` and `north` point, in the words `aim=`/`facing=` will take), which
//! parts touch, and which pieces float free of the root's. The compiler cannot
//! know the spec; this tells the model what it actually built.
//!
//! # Source
//!
//! A fifth reader of `scene::Scene`, like the rasterizer, mesher, raymarcher
//! and glTF export. It NEVER re-solves: frames and parents are read off the
//! IR, so a posed report (`--pose P`) is just the posed scene. Nothing here
//! touches the resolver, the frame solver or a voxel grid.
//!
//! # Numbers
//!
//! - `center` is the world position of the part's own extents center; `size`
//!   is the part's extents in its OWN axes (a tilted arm is still 8×98×8).
//! - The bounding box is the union of per-part world boxes computed
//!   ANALYTICALLY from shape parameters (`world_aabb`): exact for spheres,
//!   boxes, cylinders, cones, capsules, tori and ellipsoids under any
//!   rotation, never smaller than the geometry. Boxing a rotated part's
//!   extents corners instead would put the lamp's bulb 5 units too high.
//! - **Touching** means two parts' surfaces are within one voxel
//!   (`tolerance` = the thing's `voxel_size`; overlap counts). Each part's
//!   surface is sampled by the mesher at its usual cell (`mesh::auto_cell`),
//!   and the other part's distance field (`geometry::distance`) is read at
//!   those points, pruned by bounding-box proximity. The tolerance is the one
//!   judgment in the report: a gap smaller than a voxel is below what the
//!   build can resolve, so it reads as touching.
//! - A **piece** is a connected component of the touching graph. Everything
//!   outside the root's piece is listed with its nearest gap.
//!
//! # Schema
//!
//! `--json` is a PUBLIC surface: `schema` is bumped on any incompatible
//! change. It lists every part (no family collapse) with vectors instead of
//! words, rounded to 1e-6.

use std::collections::HashMap;

use serde::Serialize;

use crate::anchors::{analytic_extents, Extents};
use crate::ast::ShapeExpr;
use crate::frame::{Frame, Vec3};
use crate::geometry::{arg_f64, distance, mirror_normal, reflect_mat, spin_rot};
use crate::mesh::{auto_cell, surface_nets};
use crate::pipeline::CompileError;
use crate::scene::{Layer, Scene};

pub const SCHEMA: u32 = 1;

/// More surface samples than this per part are thinned by a fixed stride, so
/// a terrain-sized part cannot make a contact test unbounded.
const MAX_SAMPLES: usize = 20_000;

// ── Output types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub schema: u32,
    /// The pose the scene was solved in; `None` for rest. Things that do not
    /// declare it stay at rest.
    pub pose:   Option<String>,
    /// The lumps broken for `--break`; absent for a plain report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broken: Option<Vec<String>>,
    /// The states on for `--state`, in the order they combine (later wins);
    /// absent for a plain report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub states: Option<Vec<String>>,
    pub things: Vec<ThingReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThingReport {
    pub thing:      String,
    pub voxel_size: f64,
    /// Two parts touch when their surfaces are within this distance.
    pub tolerance:  f64,
    /// `None` for a thing with no parts.
    pub bbox:       Option<Bbox>,
    pub parts:      Vec<PartRow>,
    /// Touching pairs, each in declaration order.
    pub touching:   Vec<[String; 2]>,
    /// Contacts the cut tears through (`--break`): touching parts that land in
    /// different pieces, other than across a broken mate itself. Absent when
    /// nothing in this thing was broken.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub torn:       Option<Vec<[String; 2]>>,
    /// Every piece, root's first (`piece` 1), the rest by first member. With
    /// `--break`: the root's remainder, then one piece per broken lump.
    pub pieces:     Vec<PieceOut>,
    /// Remarks that are not errors (DOC-20261011 §2, §7): an `aim` near a half
    /// turn with no `facing`, …; absent when there are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes:      Vec<String>,
    /// `--state`: every part whose look the states change, with the state
    /// that wins it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub looks:      Vec<LookRow>,
    /// `--state`: parts two or more of the on states change, and which wins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlaps:   Vec<String>,
    /// Parts a breakable reveals that are not there yet (left out of the
    /// rows, contacts and pieces); `--break` on their lump shows them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden:     Vec<HiddenRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HiddenRow {
    pub part:  String,
    /// The lump whose break reveals it.
    pub until: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LookRow {
    pub part:  String,
    /// The state whose look the part shows (the latest on state that changes it).
    pub state: String,
    pub look:  crate::scene::Look,
}

#[derive(Debug, Clone, Serialize)]
pub struct Bbox {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct PartRow {
    pub name:   String,
    /// The part it hangs from; `None` for the root.
    pub parent: Option<String>,
    /// World position of the part's own extents center.
    pub center: [f64; 3],
    /// Extents in the part's own axes.
    pub size:   [f64; 3],
    /// Where the part's own `top` (+Y) points.
    pub up:     [f64; 3],
    /// Where the part's own `north` (+Z) points.
    pub front:  [f64; 3],
    /// The piece it belongs to (1 = the root's).
    pub piece:  usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct PieceOut {
    pub piece: usize,
    pub parts: Vec<String>,
    /// `--break`: the part whose mate broke to make this piece.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub broken: Option<String>,
    /// Distance to the root's piece; absent for the root's own piece.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gap:   Option<f64>,
    /// The part of the root's piece that is nearest.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from:  Option<String>,
    /// The member of this piece that is nearest to `from`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via:   Option<String>,
}

// ── Entry points ───────────────────────────────────────────────────────────

/// Compile `source` (optionally in a named pose) and report on it.
pub fn from_source<'a>(source: impl Into<crate::imports::Program<'a>>, pose: Option<&str>) -> Result<Report, Vec<CompileError>> {
    let source: crate::imports::Program = source.into();
    let scene = crate::pipeline::compile_to_scene_posed(source, pose)?;
    let mut r = build(&scene, pose);
    attach_notes(&mut r, source, pose);
    Ok(r)
}

/// The solver's notes for each printed thing, in the pose being reported.
fn attach_notes(r: &mut Report, source: crate::imports::Program, pose: Option<&str>) {
    let Ok(notes) = crate::pipeline::solve_notes(source, pose) else { return };
    for (thing, note) in notes {
        if let Some(t) = r.things.iter_mut().find(|t| t.thing == thing) {
            t.notes.push(note);
        }
    }
}

/// As `from_source`, with the named lumps broken (`moxi report --break`).
pub fn from_source_broken<'a>(
    source: impl Into<crate::imports::Program<'a>>,
    pose:   Option<&str>,
    broken: &[String],
) -> Result<Report, Vec<CompileError>> {
    let source: crate::imports::Program = source.into();
    let scene = crate::pipeline::compile_to_scene_posed(source, pose)?;
    let mut r = build_broken(&scene, pose, broken)?;
    attach_notes(&mut r, source, pose);
    Ok(r)
}

/// As `from_source_broken`, with the named states on (`moxi report --state`).
pub fn from_source_full<'a>(
    source: impl Into<crate::imports::Program<'a>>,
    pose:   Option<&str>,
    broken: &[String],
    states: &[String],
) -> Result<Report, Vec<CompileError>> {
    let source: crate::imports::Program = source.into();
    let scene = crate::pipeline::compile_to_scene_posed(source, pose)?;
    let mut r = build_broken(&scene, pose, broken)?;
    apply_states(&mut r, &scene, states)?;
    attach_notes(&mut r, source, pose);
    Ok(r)
}

/// Turn the named states on (DOC-20261011 §3): each thing's looks under them,
/// and every part two or more of them change, with the later one winning.
/// A name no printed thing declares, or two states of one family, is an
/// error. A thing that lacks a named state just stays as it is.
pub fn apply_states(r: &mut Report, scene: &Scene, names: &[String]) -> Result<(), Vec<CompileError>> {
    let mut on: Vec<String> = Vec::new();
    for n in names {
        if !on.contains(n) { on.push(n.clone()); }
    }
    if on.is_empty() { return Ok(()); }
    let rerr = |message: String| CompileError { stage: "report".into(), message, line: None, col: None, file: None };
    let mut declared: Vec<&str> = Vec::new();
    for l in &scene.layers {
        for st in crate::scene::state_order(&l.states) {
            if !declared.contains(&st.name.as_str()) { declared.push(&st.name); }
        }
    }
    let mut errors: Vec<CompileError> = on.iter().filter(|n| !declared.contains(&n.as_str())).map(|n| {
        let list = if declared.is_empty() { "none (the script declares no `state`)".to_string() } else { declared.join(", ") };
        rerr(format!("--state: no state named `{n}` in any printed thing. States: {list}"))
    }).collect();
    for l in &scene.layers {
        let mut seen: Vec<(&str, &str)> = Vec::new();   // (family, state)
        for st in l.states.iter().filter(|s| on.contains(&s.name)) {
            let Some(f) = &st.family else { continue };
            if let Some((_, other)) = seen.iter().find(|(g, _)| *g == f.as_str()) {
                errors.push(rerr(format!("--state: `{other}` and `{}` are both in family `{f}` — at most one state \
                    of a family is on; pick one", st.name)));
            } else {
                seen.push((f, &st.name));
            }
        }
    }
    if !errors.is_empty() { return Err(errors); }

    for (t, l) in r.things.iter_mut().zip(&scene.layers) {
        let order: Vec<&crate::scene::State> = crate::scene::state_order(&l.states).into_iter()
            .filter(|s| on.contains(&s.name)).collect();
        for p in &l.parts {
            let changing: Vec<&crate::scene::State> = order.iter().copied().filter(|s| s.looks.contains_key(&p.name)).collect();
            let Some(win) = changing.last() else { continue };
            if changing.len() > 1 {
                let all = changing.iter().map(|s| format!("`{}`", s.name)).collect::<Vec<_>>().join(" and ");
                let verb = if changing.len() == 2 { "both change" } else { "all change" };
                t.overlaps.push(format!("{all} {verb} `{}`; `{}` wins (its whole look of `{}`)", p.name, win.name, p.name));
            }
            t.looks.push(LookRow { part: p.name.clone(), state: win.name.clone(), look: win.looks[&p.name].clone() });
        }
    }
    let mut ordered: Vec<String> = Vec::new();
    for d in declared {
        if on.iter().any(|n| n == d) { ordered.push(d.to_string()); }
    }
    r.states = Some(ordered);
    Ok(())
}

/// Report on an already-solved scene. Reads `scene` only.
pub fn build(scene: &Scene, pose: Option<&str>) -> Report {
    Report {
        schema: SCHEMA,
        pose:   pose.filter(|p| *p != "rest").map(str::to_string),
        broken: None,
        states: None,
        things: scene.layers.iter().map(|l| thing_report(l, &[])).collect(),
    }
}

/// Report on the scene as it is after the named lumps break. Each name must
/// be a part declared `breakable` in some printed thing; anything else is an
/// error that lists the breakables. Geometry is not moved: the cut shows as
/// pieces and torn contacts, nothing is re-solved.
pub fn build_broken(
    scene:  &Scene,
    pose:   Option<&str>,
    broken: &[String],
) -> Result<Report, Vec<CompileError>> {
    let mut names: Vec<String> = Vec::new();
    for b in broken {
        if !names.contains(b) { names.push(b.clone()); }
    }
    if names.is_empty() { return Ok(build(scene, pose)); }

    let mut declared: Vec<&str> = Vec::new();
    let mut any: Vec<&str> = Vec::new();
    for l in &scene.layers {
        for p in &l.parts {
            any.push(&p.name);
            if p.breakable.is_some() && !declared.contains(&p.name.as_str()) {
                declared.push(&p.name);
            }
        }
    }
    let errors: Vec<CompileError> = names.iter()
        .filter(|n| !declared.contains(&n.as_str()))
        .map(|n| {
            let list = if declared.is_empty() {
                "none (the script declares no `breakable`)".to_string()
            } else {
                declared.join(", ")
            };
            let why = if any.contains(&n.as_str()) {
                format!("`{n}` is a part but is not declared `breakable`")
            } else {
                format!("no part named `{n}` in any printed thing")
            };
            CompileError {
                stage:   "report".into(),
                message: format!("--break: {why}. Breakable parts: {list}"),
                line:    None,
                col:     None,
                file:    None,
            }
        })
        .collect();
    if !errors.is_empty() { return Err(errors); }

    Ok(Report {
        schema: SCHEMA,
        pose:   pose.filter(|p| *p != "rest").map(str::to_string),
        broken: Some(names.clone()),
        states: None,
        things: scene.layers.iter().map(|l| thing_report(l, &names)).collect(),
    })
}

impl Report {
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("report is always serializable")
    }

    /// The default, model-facing form. Families of indexed parts collapse
    /// into one row unless `full`.
    pub fn to_text(&self, full: bool) -> String {
        let mut blocks: Vec<String> = Vec::new();
        if let Some(p) = &self.pose {
            blocks.push(format!("pose {p}"));
        }
        if let Some(b) = &self.broken {
            blocks.push(format!("broken {}", b.join(", ")));
        }
        if let Some(st) = &self.states {
            blocks.push(format!("states {}", st.join(", ")));
        }
        for t in &self.things {
            blocks.push(thing_text(t, full));
        }
        let mut out = blocks.join("\n\n");
        out.push('\n');
        out
    }
}

// ── Building ───────────────────────────────────────────────────────────────

/// One part, ready for geometry queries.
struct Body {
    expr:   ShapeExpr,
    frame:  Frame,
    inv:    Frame,
    bbox:   Aabb,
    center: Vec3,
}

fn thing_report(layer: &Layer, broken: &[String]) -> ThingReport {
    // Revealed parts (Issue G) are not there until their lump breaks: the
    // report reads the thing without them, and with them once `--break`
    // names the lump that reveals them.
    let mut revealer: HashMap<&str, &str> = HashMap::new();
    for p in &layer.parts {
        for r in p.breakable.iter().flat_map(|b| &b.reveal) { revealer.insert(r.as_str(), p.name.as_str()); }
    }
    let still_hidden: Vec<String> = layer.parts.iter()
        .filter(|p| p.hidden && revealer.get(p.name.as_str()).is_none_or(|l| !broken.iter().any(|b| b == l)))
        .map(|p| p.name.clone()).collect();
    let mut t = if still_hidden.is_empty() {
        thing_report_shown(layer, broken)
    } else {
        let mut shown = layer.clone();
        shown.parts.retain(|p| !still_hidden.contains(&p.name));
        thing_report_shown(&shown, broken)
    };
    t.hidden = still_hidden.iter().map(|h| HiddenRow {
        part: h.clone(), until: revealer.get(h.as_str()).map(|s| s.to_string()).unwrap_or_default(),
    }).collect();
    // a broken lump's `after` swaps
    for p in layer.parts.iter().filter(|p| broken.contains(&p.name)) {
        for (part, look) in p.breakable.iter().flat_map(|b| &b.after) {
            t.looks.push(LookRow { part: part.clone(), state: format!("after {}", p.name), look: look.clone() });
        }
    }
    t
}

fn thing_report_shown(layer: &Layer, broken: &[String]) -> ThingReport {
    let vs  = layer.voxel_size;
    let tol = vs;

    let mut bodies: Vec<Body> = Vec::with_capacity(layer.parts.len());
    let mut rows:   Vec<PartRow> = Vec::with_capacity(layer.parts.len());
    for p in &layer.parts {
        let expr  = p.shape.to_expr();
        let frame = p.frame.to_frame();
        let e: Extents = analytic_extents(&expr);
        let center = frame.apply_point(e.center());
        let size   = e.max.sub(e.min);
        bodies.push(Body {
            bbox: world_aabb(&expr, &frame),
            inv:  frame.inverse(),
            expr, frame, center,
        });
        rows.push(PartRow {
            name:   p.name.clone(),
            parent: p.joint.as_ref().and_then(|j| j.parent.clone()),
            center: vec6(center),
            size:   vec6(size),
            up:     vec6(frame.rot.col(1)),
            front:  vec6(frame.rot.col(2)),
            piece:  0,
        });
    }

    let n = bodies.len();
    let bbox = bodies.iter().map(|b| b.bbox).reduce(Aabb::union).map(|b| Bbox {
        min: vec6(b.lo),
        max: vec6(b.hi),
    });

    // ── contacts ──────────────────────────────────────────────────────────
    let mut sampler = Sampler { vs, bodies: &bodies, cache: vec![None; n] };
    let mut dsu = Dsu::new(n);
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            if bodies[i].bbox.gap(&bodies[j].bbox) > tol { continue; }
            let g = sampler.min_sdf(i, j, tol).min(sampler.min_sdf(j, i, tol));
            if g <= tol {
                edges.push((i, j));
                dsu.union(i, j);
            }
        }
    }

    // ── pieces ────────────────────────────────────────────────────────────
    // The root is the first part nothing parents; a scene with none (cannot
    // happen once every part but one is placed) falls back to the first.
    let root = rows.iter().position(|r| r.parent.is_none()).unwrap_or(0);
    let mut piece_of = vec![0usize; n];
    let mut members: Vec<Vec<usize>> = Vec::new();
    if n > 0 {
        let root_rep = dsu.find(root);
        let mut rep_piece: HashMap<usize, usize> = HashMap::new();
        rep_piece.insert(root_rep, 0);
        members.push(Vec::new());
        for i in 0..n {
            let rep = dsu.find(i);
            let k = *rep_piece.entry(rep).or_insert_with(|| {
                members.push(Vec::new());
                members.len() - 1
            });
            piece_of[i] = k;
            members[k].push(i);
        }
    }
    for (r, k) in rows.iter_mut().zip(&piece_of) {
        r.piece = k + 1;
    }

    let mut pieces: Vec<PieceOut> = Vec::new();
    for (k, mem) in members.iter().enumerate() {
        let names: Vec<String> = mem.iter().map(|&i| rows[i].name.clone()).collect();
        if k == 0 {
            pieces.push(PieceOut { piece: 1, parts: names, broken: None, gap: None, from: None, via: None });
            continue;
        }
        let (gap, via, from) = nearest_to_root(&mut sampler, &bodies, mem, &members[0]);
        pieces.push(PieceOut {
            piece: k + 1,
            parts: names,
            broken: None,
            gap:   Some(round6(gap)),
            from:  Some(rows[from].name.clone()),
            via:   Some(rows[via].name.clone()),
        });
    }

    // ── the cut (`--break`) ───────────────────────────────────────────────
    // A lump is the broken part and everything mated under it, minus any
    // lump inside it that is broken too. Pieces are read off the mate tree,
    // so they hold even where the geometry overlaps.
    let lumps: Vec<usize> = layer.parts.iter().enumerate()
        .filter(|(_, p)| p.breakable.is_some() && broken.contains(&p.name))
        .map(|(i, _)| i)
        .collect();
    let mut torn: Option<Vec<[String; 2]>> = None;
    if !lumps.is_empty() {
        let lump_of: Vec<Option<usize>> = {
            let index: HashMap<&str, usize> =
                rows.iter().enumerate().map(|(i, r)| (r.name.as_str(), i)).collect();
            (0..n).map(|i| {
                let mut cur = Some(i);
                for _ in 0..=n {
                    let c = cur?;
                    if lumps.contains(&c) { return Some(c); }
                    cur = rows[c].parent.as_deref().and_then(|p| index.get(p).copied());
                }
                None
            }).collect()
        };
        pieces = std::iter::once(PieceOut {
                piece: 1, parts: Vec::new(), broken: None, gap: None, from: None, via: None,
            })
            .chain(lumps.iter().enumerate().map(|(k, &r)| PieceOut {
                piece: k + 2, parts: Vec::new(), broken: Some(rows[r].name.clone()),
                gap: None, from: None, via: None,
            }))
            .collect();
        for i in 0..n {
            let k = lump_of[i]
                .and_then(|r| lumps.iter().position(|&l| l == r))
                .map_or(0, |k| k + 1);
            rows[i].piece = k + 1;
            pieces[k].parts.push(rows[i].name.clone());
        }
        // The mate that broke is not a tear: a lump's root against its parent.
        let across = |a: usize, b: usize| {
            lump_of[a] == Some(a) && rows[a].parent.as_deref() == Some(rows[b].name.as_str())
        };
        // Nor is a part the break reveals against the lump that revealed it:
        // a stump stands where the lump left.
        let index: HashMap<&str, usize> = rows.iter().enumerate().map(|(i, r)| (r.name.as_str(), i)).collect();
        let mut revealed_by: HashMap<usize, usize> = HashMap::new();
        for &l in &lumps {
            for r in layer.parts[l].breakable.iter().flat_map(|b| &b.reveal) {
                if let Some(&i) = index.get(r.as_str()) { revealed_by.insert(i, l); }
            }
        }
        let shown = |a: usize, b: usize| revealed_by.get(&a).is_some_and(|&l| lump_of[b] == Some(l));
        torn = Some(
            edges.iter()
                .filter(|&&(i, j)| lump_of[i] != lump_of[j] && !across(i, j) && !across(j, i)
                    && !shown(i, j) && !shown(j, i))
                .map(|&(i, j)| [rows[i].name.clone(), rows[j].name.clone()])
                .collect(),
        );
    }

    ThingReport {
        thing:      layer.thing.clone(),
        voxel_size: vs,
        tolerance:  tol,
        bbox,
        touching:   edges.iter().map(|&(i, j)| [rows[i].name.clone(), rows[j].name.clone()]).collect(),
        torn,
        parts:      rows,
        pieces,
        notes:      Vec::new(),
        looks:      Vec::new(),
        overlaps:   Vec::new(),
        hidden:     Vec::new(),
    }
}

/// Nearest pair (member of `piece`, member of the root's piece), as
/// `(gap, piece_member, root_member)`. Candidates are tried in order of
/// bounding-box distance and the search stops once a box is farther than the
/// best gap found, so few distance fields are read.
fn nearest_to_root(
    sampler: &mut Sampler<'_>,
    bodies:  &[Body],
    piece:   &[usize],
    root:    &[usize],
) -> (f64, usize, usize) {
    let mut best = (f64::INFINITY, piece[0], root[0]);
    for &q in piece {
        let mut cands: Vec<(f64, usize)> =
            root.iter().map(|&r| (bodies[q].bbox.gap(&bodies[r].bbox), r)).collect();
        cands.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (lb, r) in cands {
            if lb >= best.0 { break; }
            let g = sampler.min_sdf(q, r, f64::INFINITY).min(sampler.min_sdf(r, q, f64::INFINITY));
            if g < best.0 { best = (g, q, r); }
        }
    }
    best
}

// ── Surface sampling ───────────────────────────────────────────────────────

struct Sampler<'a> {
    vs:     f64,
    bodies: &'a [Body],
    /// World-space surface samples, filled on first use.
    cache:  Vec<Option<Vec<Vec3>>>,
}

impl Sampler<'_> {
    fn ensure(&mut self, i: usize) {
        if self.cache[i].is_some() { return; }
        let b    = &self.bodies[i];
        let mesh = surface_nets(&b.expr, auto_cell(&b.expr, self.vs), self.vs);
        let mut pts: Vec<Vec3> = mesh.positions.iter()
            .map(|p| b.frame.apply_point(Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64)))
            .collect();
        // A part thinner than the cell may mesh to nothing: probe its center.
        if pts.is_empty() { pts.push(b.center); }
        if pts.len() > MAX_SAMPLES {
            let step = pts.len().div_ceil(MAX_SAMPLES);
            pts = pts.into_iter().step_by(step).collect();
        }
        self.cache[i] = Some(pts);
    }

    /// Smallest value of part `b`'s distance field over part `a`'s surface
    /// samples that lie within `bound` of `b`'s box (all of them when the
    /// bound is infinite). Negative where `a` reaches inside `b`; infinite
    /// when no sample is in range.
    fn min_sdf(&mut self, a: usize, b: usize, bound: f64) -> f64 {
        self.ensure(a);
        let verts = self.cache[a].as_ref().expect("filled by ensure");
        let ob    = &self.bodies[b];
        let zone  = bound.is_finite().then(|| ob.bbox.grown(bound));
        let mut best = f64::INFINITY;
        for v in verts {
            if let Some(z) = &zone {
                if !z.contains(*v) { continue; }
            }
            let d = distance(&ob.expr, ob.inv.apply_point(*v), self.vs);
            if d < best { best = d; }
        }
        best
    }
}

struct Dsu(Vec<usize>);

impl Dsu {
    fn new(n: usize) -> Self { Dsu((0..n).collect()) }

    fn find(&mut self, mut x: usize) -> usize {
        while self.0[x] != x {
            self.0[x] = self.0[self.0[x]];
            x = self.0[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb { self.0[ra.max(rb)] = ra.min(rb); }
    }
}

// ── World bounding boxes (analytic) ────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct Aabb {
    lo: Vec3,
    hi: Vec3,
}

impl Aabb {
    fn around(c: Vec3, half: Vec3) -> Aabb { Aabb { lo: c.sub(half), hi: c.add(half) } }

    fn of_points(a: Vec3, b: Vec3) -> Aabb {
        Aabb {
            lo: Vec3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            hi: Vec3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        }
    }

    fn union(self, o: Aabb) -> Aabb {
        Aabb {
            lo: Vec3::new(self.lo.x.min(o.lo.x), self.lo.y.min(o.lo.y), self.lo.z.min(o.lo.z)),
            hi: Vec3::new(self.hi.x.max(o.hi.x), self.hi.y.max(o.hi.y), self.hi.z.max(o.hi.z)),
        }
    }

    fn intersect(self, o: Aabb) -> Option<Aabb> {
        let r = Aabb {
            lo: Vec3::new(self.lo.x.max(o.lo.x), self.lo.y.max(o.lo.y), self.lo.z.max(o.lo.z)),
            hi: Vec3::new(self.hi.x.min(o.hi.x), self.hi.y.min(o.hi.y), self.hi.z.min(o.hi.z)),
        };
        (r.lo.x <= r.hi.x && r.lo.y <= r.hi.y && r.lo.z <= r.hi.z).then_some(r)
    }

    fn grown(self, g: f64) -> Aabb { Aabb::around(self.center(), self.hi.sub(self.lo).scale(0.5).add(Vec3::new(g, g, g))) }

    fn center(&self) -> Vec3 { self.lo.add(self.hi).scale(0.5) }

    fn contains(&self, p: Vec3) -> bool {
        p.x >= self.lo.x && p.x <= self.hi.x
            && p.y >= self.lo.y && p.y <= self.hi.y
            && p.z >= self.lo.z && p.z <= self.hi.z
    }

    /// Euclidean distance between two boxes; 0 when they overlap.
    fn gap(&self, o: &Aabb) -> f64 {
        let d = |alo: f64, ahi: f64, blo: f64, bhi: f64| (blo - ahi).max(alo - bhi).max(0.0);
        let (dx, dy, dz) = (
            d(self.lo.x, self.hi.x, o.lo.x, o.hi.x),
            d(self.lo.y, self.hi.y, o.lo.y, o.hi.y),
            d(self.lo.z, self.hi.z, o.lo.z, o.hi.z),
        );
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// World box of a shape under `f`, from its parameters. Exact for the round
/// primitives under any rotation, and for `at`/`spin`/`mirror`/`union`/
/// `shell`/`difference` by composing frames; shapes without a closed form
/// (`extrude`, `scale`) fall back to the 8 corners of their analytic
/// extents, which is a valid but looser bound. Never smaller than the
/// geometry (a `blob`'s noise can exceed its nominal sphere by a fraction
/// of its roughness).
fn world_aabb(shape: &ShapeExpr, f: &Frame) -> Aabb {
    let pos  = f.pos;
    let axis = f.rot.col(1); // where the shape's +Y points
    let m    = &f.rot.0;

    // Radius of a disc perpendicular to the axis, along each world axis.
    let perp = |r: f64| Vec3::new(
        r * (1.0 - axis.x * axis.x).max(0.0).sqrt(),
        r * (1.0 - axis.y * axis.y).max(0.0).sqrt(),
        r * (1.0 - axis.z * axis.z).max(0.0).sqrt(),
    );

    match shape {
        ShapeExpr::Sphere { args } | ShapeExpr::Blob { args } => {
            let r = arg_f64(args, "radius", 1.0);
            Aabb::around(pos, Vec3::new(r, r, r))
        }
        ShapeExpr::Ellipsoid { args } => {
            let (rx, ry, rz) = (arg_f64(args, "rx", 1.0), arg_f64(args, "ry", 1.0), arg_f64(args, "rz", 1.0));
            let h = |i: usize| ((m[i][0] * rx).powi(2) + (m[i][1] * ry).powi(2) + (m[i][2] * rz).powi(2)).sqrt();
            Aabb::around(pos, Vec3::new(h(0), h(1), h(2)))
        }
        // `round` only trims corners, so the plain box stays a valid bound.
        ShapeExpr::Box_ { args } => {
            let hw = [
                arg_f64(args, "width", 2.0) / 2.0,
                arg_f64(args, "height", 2.0) / 2.0,
                arg_f64(args, "depth", 2.0) / 2.0,
            ];
            let h = |i: usize| m[i][0].abs() * hw[0] + m[i][1].abs() * hw[1] + m[i][2].abs() * hw[2];
            Aabb::around(pos, Vec3::new(h(0), h(1), h(2)))
        }
        ShapeExpr::Cylinder { args } => {
            let (h, r) = (arg_f64(args, "height", 1.0), arg_f64(args, "radius", 0.5));
            let ends = Aabb::of_points(pos, pos.add(axis.scale(h)));
            Aabb { lo: ends.lo.sub(perp(r)), hi: ends.hi.add(perp(r)) }
        }
        ShapeExpr::Heightfield { args } => {
            let (h, r) = (arg_f64(args, "max_height", 20.0), arg_f64(args, "radius", 50.0));
            let ends = Aabb::of_points(pos, pos.add(axis.scale(h)));
            Aabb { lo: ends.lo.sub(perp(r)), hi: ends.hi.add(perp(r)) }
        }
        ShapeExpr::Cone { args } => {
            let (h, r) = (arg_f64(args, "height", 1.0), arg_f64(args, "radius", 0.5));
            let base = Aabb { lo: pos.sub(perp(r)), hi: pos.add(perp(r)) };
            let apex = pos.add(axis.scale(h));
            base.union(Aabb::of_points(apex, apex))
        }
        ShapeExpr::Capsule { args } => {
            let (h, r) = (arg_f64(args, "height", 1.0), arg_f64(args, "radius", 0.5));
            let ends = Aabb::of_points(pos, pos.add(axis.scale(h)));
            Aabb { lo: ends.lo.sub(Vec3::new(r, r, r)), hi: ends.hi.add(Vec3::new(r, r, r)) }
        }
        ShapeExpr::Torus { args } => {
            let (major, minor) = (arg_f64(args, "major_radius", 2.0), arg_f64(args, "minor_radius", 0.5));
            let ring = perp(major);
            Aabb::around(pos, ring.add(Vec3::new(minor, minor, minor)))
        }
        ShapeExpr::Shell { inner, .. } => world_aabb(inner, f),
        ShapeExpr::Difference { base, .. } => world_aabb(base, f),
        ShapeExpr::Union { shapes, .. } => shapes.iter()
            .map(|s| world_aabb(s, f))
            .reduce(Aabb::union)
            .unwrap_or_else(|| Aabb::around(pos, Vec3::ZERO)),
        ShapeExpr::Intersect { shapes } => shapes.iter()
            .map(|s| world_aabb(s, f))
            .try_fold(None::<Aabb>, |acc, b| match acc {
                None    => Some(Some(b)),
                Some(a) => a.intersect(b).map(Some),
            })
            .flatten()
            .unwrap_or_else(|| corners_aabb(&analytic_extents(shape), f)),
        ShapeExpr::At { inner, args } => {
            let t = Vec3::new(arg_f64(args, "x", 0.0), arg_f64(args, "y", 0.0), arg_f64(args, "z", 0.0));
            world_aabb(inner, &f.compose(&Frame::from_pos(t)))
        }
        ShapeExpr::Spin { inner, args } => world_aabb(inner, &f.compose(&Frame::from_rot(spin_rot(args)))),
        ShapeExpr::Mirror { inner, args } =>
            world_aabb(inner, &f.compose(&Frame::from_rot(reflect_mat(mirror_normal(args))))),
        ShapeExpr::Extrude { .. } | ShapeExpr::Scale { .. } => corners_aabb(&analytic_extents(shape), f),
    }
}

/// The 8 corners of `e`, mapped through `f`, boxed. A valid bound for any
/// shape, tight only when `f` is axis-aligned.
fn corners_aabb(e: &Extents, f: &Frame) -> Aabb {
    let mut out: Option<Aabb> = None;
    for &x in &[e.min.x, e.max.x] {
        for &y in &[e.min.y, e.max.y] {
            for &z in &[e.min.z, e.max.z] {
                let p = f.apply_point(Vec3::new(x, y, z));
                let b = Aabb::of_points(p, p);
                out = Some(out.map_or(b, |a| a.union(b)));
            }
        }
    }
    out.expect("eight corners")
}

// ── Words ──────────────────────────────────────────────────────────────────

const COMPASS: [&str; 4] = ["north", "east", "south", "west"];

/// A direction in the words `aim=`/`facing=` take.
///
/// `up`/`down` within 10° of vertical. Otherwise the nearest compass word,
/// then whole-degree remainders: `north 20° up`, `north 35° east, 20° down`.
/// Zero terms are dropped. North is +Z, east is +X, up is +Y.
pub fn direction_words(d: [f64; 3]) -> String {
    let v = Vec3::new(d[0], d[1], d[2]);
    let Some(u) = v.normalize() else { return "none".to_string() };

    let elevation = u.y.clamp(-1.0, 1.0).asin().to_degrees();
    if 90.0 - elevation.abs() <= 10.0 + 1e-9 {
        return if elevation > 0.0 { "up" } else { "down" }.to_string();
    }

    // Heading measured from north toward east.
    let heading = u.x.atan2(u.z).to_degrees();
    let quarter = (heading / 90.0).round();
    let main    = COMPASS[(quarter as i64).rem_euclid(4) as usize];
    let rest    = (heading - quarter * 90.0).round() as i64;
    let side    = COMPASS[((quarter as i64) + rest.signum()).rem_euclid(4) as usize];
    let lift    = elevation.round() as i64;

    let mut out = main.to_string();
    if rest != 0 {
        out.push_str(&format!(" {}° {side}", rest.abs()));
    }
    if lift != 0 {
        out.push_str(if rest != 0 { ", " } else { " " });
        out.push_str(&format!("{}° {}", lift.abs(), if lift > 0 { "up" } else { "down" }));
    }
    out
}

/// A number to one decimal, or as an integer when it is one.
fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        return format!("{}", v.round() as i64);
    }
    let r = (v * 10.0).round() / 10.0;
    format!("{:.1}", if r == 0.0 { 0.0 } else { r })
}

fn fmt_fixed(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    if s.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') {
        s.trim_start_matches('-').to_string()
    } else {
        s
    }
}

fn fmt_vec(v: [f64; 3]) -> String {
    format!("({}, {}, {})", fmt_num(v[0]), fmt_num(v[1]), fmt_num(v[2]))
}

fn fmt_size(v: [f64; 3]) -> String {
    format!("{}×{}×{}", fmt_num(v[0]), fmt_num(v[1]), fmt_num(v[2]))
}

fn round6(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

fn vec6(v: Vec3) -> [f64; 3] { [round6(v.x), round6(v.y), round6(v.z)] }

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

// ── Text ───────────────────────────────────────────────────────────────────

/// `Name[12]` → (`Name`, 12).
fn parse_indexed(name: &str) -> Option<(&str, u64)> {
    let body = name.strip_suffix(']')?;
    let open = body.rfind('[')?;
    let idx  = body[open + 1..].parse::<u64>().ok()?;
    Some((&body[..open], idx))
}

/// One printed row: a single part, or a family of consecutive indexed parts
/// that share a parent and a size.
struct Row {
    /// What the table prints: `Fin[0..3] ×4`, or a part's own name.
    label:   String,
    /// What the contacts line prints: `Fin[0..3]`, or a part's own name.
    short:   String,
    members: Vec<usize>,
}

fn rows_of(t: &ThingReport, full: bool) -> Vec<Row> {
    let n = t.parts.len();
    let single = |i: usize| Row {
        label: t.parts[i].name.clone(), short: t.parts[i].name.clone(), members: vec![i],
    };
    if full {
        return (0..n).map(single).collect();
    }

    type Key<'a> = (&'a str, Option<&'a str>, String);
    let key = |i: usize| -> Option<(Key<'_>, u64)> {
        let p = &t.parts[i];
        let (base, idx) = parse_indexed(&p.name)?;
        let size = format!("{:.6}|{:.6}|{:.6}", p.size[0], p.size[1], p.size[2]);
        Some(((base, p.parent.as_deref(), size), idx))
    };

    // Group by (base, parent, size), then cut each group into runs of
    // consecutive indices; a run of two or more is a family.
    let mut by_key: Vec<(Key<'_>, Vec<(usize, u64)>)> = Vec::new();
    for i in 0..n {
        if let Some((k, idx)) = key(i) {
            match by_key.iter_mut().find(|(g, _)| *g == k) {
                Some((_, v)) => v.push((i, idx)),
                None         => by_key.push((k, vec![(i, idx)])),
            }
        }
    }
    let mut family_start: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut in_family = vec![false; n];
    for (_, members) in &by_key {
        let mut run: Vec<(usize, u64)> = Vec::new();
        let mut flush = |run: &mut Vec<(usize, u64)>| {
            if run.len() >= 2 {
                let ids: Vec<usize> = run.iter().map(|&(i, _)| i).collect();
                for &i in &ids { in_family[i] = true; }
                family_start.insert(ids[0], ids);
            }
            run.clear();
        };
        for &(i, idx) in members {
            if run.last().is_some_and(|&(_, prev)| prev + 1 != idx) { flush(&mut run); }
            run.push((i, idx));
        }
        flush(&mut run);
    }

    let mut rows = Vec::new();
    for i in 0..n {
        if let Some(ids) = family_start.remove(&i) {
            let (base, first) = parse_indexed(&t.parts[i].name).expect("family members are indexed");
            let (_, last)     = parse_indexed(&t.parts[*ids.last().unwrap()].name).expect("indexed");
            let short = format!("{base}[{first}..{last}]");
            rows.push(Row { label: format!("{short} ×{}", ids.len()), short, members: ids });
        } else if !in_family[i] {
            rows.push(single(i));
        }
    }
    rows
}

/// `Skull[0..3], Eye` — consecutive indexed names collapse, order is kept.
fn collapse_names(names: &[String], full: bool) -> String {
    if full { return names.join(", "); }
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < names.len() {
        let mut j = i + 1;
        if let Some((base, first)) = parse_indexed(&names[i]) {
            while j < names.len()
                && parse_indexed(&names[j]) == Some((base, first + (j - i) as u64))
            {
                j += 1;
            }
            if j - i >= 2 {
                let (_, last) = parse_indexed(&names[j - 1]).expect("indexed");
                out.push(format!("{base}[{first}..{last}]"));
                i = j;
                continue;
            }
        }
        out.push(names[i].clone());
        i += 1;
    }
    out.join(", ")
}

/// A look in a few words: `Hole #000000`, `Eyes #ff2414 glow 2`, …
fn look_words(l: &crate::scene::Look) -> String {
    let mut w = vec![match &l.material { Some(m) => format!("{m} {}", l.color), None => l.color.clone() }];
    if let Some(e) = l.emissive { w.push(format!("glow {}", fmt_look(e))); }
    if let Some(o) = l.opacity { w.push(format!("opacity {}", fmt_look(o))); }
    if let Some(r) = l.roughness { w.push(format!("roughness {}", fmt_look(r))); }
    if let Some(m) = l.metalness { w.push(format!("metalness {}", fmt_look(m))); }
    if l.blend.is_some() { w.push("additive".into()); }
    w.join(" ")
}

/// A look's number as written, up to two decimals (`0.95`, `2`).
fn fmt_look(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn thing_text(t: &ThingReport, full: bool) -> String {
    let n_pieces = t.pieces.len();
    let mut lines: Vec<String> = vec![format!(
        "{} · voxel {} · {} · {}",
        t.thing, fmt_num(t.voxel_size),
        plural(t.parts.len(), "part", "parts"),
        plural(n_pieces, "piece", "pieces"),
    )];

    let Some(b) = &t.bbox else { return lines.join("\n") };
    let dec = if t.voxel_size >= 1.0 { 0 } else { 1 };
    let axis = |i: usize, name: &str| format!(
        "{name} {}..{}", fmt_fixed(b.min[i], dec), fmt_fixed(b.max[i], dec));
    lines.push(format!(
        "bbox  {}  {}  {}  ({} × {} × {})",
        axis(0, "x"), axis(1, "y"), axis(2, "z"),
        fmt_fixed(b.max[0] - b.min[0], dec),
        fmt_fixed(b.max[1] - b.min[1], dec),
        fmt_fixed(b.max[2] - b.min[2], dec),
    ));

    // ── the table ─────────────────────────────────────────────────────────
    let rows = rows_of(t, full);
    let words = |v: [f64; 3]| direction_words(v);
    let mut table: Vec<[String; 6]> = vec![[
        "part".into(), "on".into(), "center".into(), "size".into(), "up".into(), "front".into(),
    ]];
    for r in &rows {
        let first = &t.parts[r.members[0]];
        let on    = first.parent.clone().unwrap_or_else(|| "—".to_string());
        if r.members.len() == 1 {
            table.push([
                r.label.clone(), on, fmt_vec(first.center), fmt_size(first.size),
                words(first.up), words(first.front),
            ]);
        } else {
            let same = |f: fn(&PartRow) -> [f64; 3]| {
                let w0 = words(f(first));
                if r.members.iter().all(|&i| words(f(&t.parts[i])) == w0) { w0 } else { "varies".to_string() }
            };
            table.push([
                r.label.clone(), on, "varies".into(), fmt_size(first.size),
                same(|p| p.up), same(|p| p.front),
            ]);
        }
    }
    let widths: Vec<usize> = (0..6)
        .map(|c| table.iter().map(|row| row[c].chars().count()).max().unwrap_or(0))
        .collect();
    for row in &table {
        let cells: Vec<String> = row.iter().enumerate()
            .map(|(c, s)| format!("{s:<w$}", w = widths[c]))
            .collect();
        lines.push(cells.join("   ").trim_end().to_string());
    }

    // ── contacts ──────────────────────────────────────────────────────────
    let mut row_of: HashMap<&str, usize> = HashMap::new();
    for (k, r) in rows.iter().enumerate() {
        for &i in &r.members { row_of.insert(t.parts[i].name.as_str(), k); }
    }
    // A family that only partly touches names the members that do
    // (`Fin[2]–Body`), so `Fin[0..3]–Body` always means all four.
    let describe = |pairs: &[[String; 2]]| -> String {
        type Group = ((usize, usize), usize, [Vec<String>; 2]);
        let mut groups: Vec<Group> = Vec::new();
        for [a, b] in pairs {
            let key = (row_of[a.as_str()], row_of[b.as_str()]);
            let at = match groups.iter().position(|(k, ..)| *k == key) {
                Some(at) => at,
                None => { groups.push((key, 0, [Vec::new(), Vec::new()])); groups.len() - 1 }
            };
            groups[at].1 += 1;
            for (side, name) in [a, b].into_iter().enumerate() {
                if !groups[at].2[side].contains(name) { groups[at].2[side].push(name.clone()); }
            }
        }
        let side_text = |row: usize, touched: &[String]| -> String {
            let members = &rows[row].members;
            if touched.len() >= members.len() { return rows[row].short.clone(); }
            let ordered: Vec<String> = members.iter()
                .map(|&i| t.parts[i].name.clone())
                .filter(|n| touched.contains(n))
                .collect();
            collapse_names(&ordered, false).replace(", ", "+")
        };
        let shown: Vec<String> = groups.iter().map(|((a, b), c, names)| {
            let mut s = format!("{}–{}", side_text(*a, &names[0]), side_text(*b, &names[1]));
            if *c > 1 { s.push_str(&format!(" ×{c}")); }
            s
        }).collect();
        if shown.is_empty() { "none".to_string() } else { shown.join(", ") }
    };
    lines.push(format!("touching: {}", describe(&t.touching)));

    // ── floating pieces ───────────────────────────────────────────────────
    for p in t.pieces.iter().filter(|p| p.gap.is_some()) {
        let mut s = format!(
            "piece {}: {} — {} from {}",
            p.piece, collapse_names(&p.parts, full),
            fmt_num(p.gap.unwrap_or(0.0)), p.from.as_deref().unwrap_or("?"),
        );
        if p.parts.len() > 1 {
            s.push_str(&format!(" (via {})", p.via.as_deref().unwrap_or("?")));
        }
        lines.push(s);
    }

    // ── the cut ───────────────────────────────────────────────────────────
    if let Some(torn) = &t.torn {
        for p in &t.pieces {
            let how = match &p.broken {
                Some(at) => format!("broken off at {at}"),
                None     => "stays with the root".to_string(),
            };
            lines.push(format!("piece {}: {} — {how}", p.piece, collapse_names(&p.parts, full)));
        }
        lines.push(format!("torn: {}", describe(torn)));
    }

    for n in &t.notes {
        lines.push(format!("note: {n}"));
    }
    for l in &t.looks {
        lines.push(format!("look: {} — {} ({})", l.part, look_words(&l.look), l.state));
    }
    for o in &t.overlaps {
        lines.push(format!("overlap: {o}"));
    }
    if !t.hidden.is_empty() {
        lines.push(format!("hidden: {}", t.hidden.iter()
            .map(|h| format!("{} (until {} breaks)", h.part, h.until)).collect::<Vec<_>>().join(", ")));
    }

    lines.join("\n")
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::Mat3;

    const LAMP: &str      = include_str!("../scripts/LAMP.md");
    const DESK_LAMP: &str = include_str!("../scripts/DESK_LAMP.md");

    fn near(a: [f64; 3], b: [f64; 3], eps: f64) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < eps)
    }

    fn row<'a>(t: &'a ThingReport, name: &str) -> &'a PartRow {
        t.parts.iter().find(|p| p.name == name).unwrap_or_else(|| panic!("no part {name}"))
    }

    // ── words ─────────────────────────────────────────────────────────────

    #[test]
    fn directions_read_as_the_words_aim_and_facing_take() {
        let up = |deg: f64| [0.0, deg.to_radians().sin(), deg.to_radians().cos()]; // north, tipped up
        let heading = |h: f64, lift: f64| {
            let (c, s) = (lift.to_radians().cos(), lift.to_radians().sin());
            [h.to_radians().sin() * c, s, h.to_radians().cos() * c]
        };
        for (v, want) in [
            ([0.0, 1.0, 0.0],  "up"),
            ([0.0, -1.0, 0.0], "down"),
            ([0.0, 0.0, 1.0],  "north"),
            ([1.0, 0.0, 0.0],  "east"),
            ([0.0, 0.0, -1.0], "south"),
            ([-1.0, 0.0, 0.0], "west"),
            (up(20.0),  "north 20° up"),
            (up(-30.0), "north 30° down"),
            (heading(35.0, -20.0),   "north 35° east, 20° down"),
            (heading(110.0, 0.0),    "east 20° south"),
            (heading(-100.0, 0.0),   "west 10° south"),
            (heading(-30.0, 10.0),   "north 30° west, 10° up"),
        ] {
            assert_eq!(direction_words(v), want, "for {v:?}");
        }
    }

    #[test]
    fn vertical_means_within_ten_degrees() {
        assert_eq!(direction_words(up_by(9.9)), "up");
        assert_eq!(direction_words(down_by(9.9)), "down");
        assert_eq!(direction_words(down_by(11.0)), "north 79° down");
        // 11° off vertical is a lean, not "up".
        assert_eq!(direction_words(up_by(11.0)), "north 79° up");
    }

    /// A unit vector `off` degrees from straight up, leaning north.
    fn up_by(off: f64) -> [f64; 3] {
        let a = (90.0 - off).to_radians();
        [0.0, a.sin(), a.cos()]
    }

    /// A unit vector `off` degrees from straight down, leaning north.
    fn down_by(off: f64) -> [f64; 3] {
        let a = off.to_radians();
        [0.0, -a.cos(), a.sin()]
    }

    #[test]
    fn numbers_print_short() {
        assert_eq!(fmt_num(6.0), "6");
        assert_eq!(fmt_num(304.76), "304.8");
        assert_eq!(fmt_num(52.0449), "52.0");
        assert_eq!(fmt_num(-0.04), "0.0");
        assert_eq!(fmt_num(-12.5), "-12.5");
        assert_eq!(fmt_fixed(-0.2, 0), "0");
        assert_eq!(fmt_fixed(339.52, 0), "340");
    }

    // ── analytic boxes ────────────────────────────────────────────────────

    fn shape(src: &str) -> ShapeExpr {
        // A one-part thing, so the real parser/resolver builds the expression.
        let s = format!("```moxi\nmaterial M {{ color = red }}\nthing T {{ part P {{ shape = {src}, material = M }} resolve voxel_size = 1 }}\nprint T detail=low\n```\n");
        let scene = crate::pipeline::compile_to_scene(&s).expect("compiles");
        scene.layers[0].parts[0].shape.to_expr()
    }

    #[test]
    fn a_rotated_sphere_boxes_to_center_plus_minus_radius() {
        let f = Frame::new(Mat3::rot_x(0.6).mul(&Mat3::rot_z(0.3)), Vec3::new(5.0, -2.0, 9.0));
        let b = world_aabb(&shape("sphere(radius=7)"), &f);
        assert!(near(vec6(b.lo), [-2.0, -9.0, 2.0], 1e-9) && near(vec6(b.hi), [12.0, 5.0, 16.0], 1e-9));
    }

    #[test]
    fn a_tilted_cylinder_boxes_by_its_discs() {
        // 90° about x: the axis (+Y) points to +Z, so it spans z 0..10.
        let f = Frame::new(Mat3::rot_x(std::f64::consts::FRAC_PI_2), Vec3::ZERO);
        let b = world_aabb(&shape("cylinder(height=10, radius=3)"), &f);
        assert!(near(vec6(b.lo), [-3.0, -3.0, 0.0], 1e-9) && near(vec6(b.hi), [3.0, 3.0, 10.0], 1e-9), "{b:?}");
        // 45° about z: the end discs tip, shrinking their x/y reach by cos/sin.
        let f = Frame::new(Mat3::rot_z(std::f64::consts::FRAC_PI_4), Vec3::ZERO);
        let b = world_aabb(&shape("cylinder(height=10, radius=3)"), &f);
        let (s, r) = (std::f64::consts::FRAC_1_SQRT_2, 3.0 * std::f64::consts::FRAC_1_SQRT_2);
        assert!((b.hi.x - r).abs() < 1e-9 && (b.lo.x + 10.0 * s + r).abs() < 1e-9, "{b:?}");
        assert!((b.hi.z - 3.0).abs() < 1e-9 && (b.lo.z + 3.0).abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn composed_shapes_follow_their_wrappers() {
        let f = Frame::IDENTITY;
        let b = world_aabb(&shape("at(sphere(radius=2), x=10)"), &f);
        assert!(near(vec6(b.lo), [8.0, -2.0, -2.0], 1e-9) && near(vec6(b.hi), [12.0, 2.0, 2.0], 1e-9));
        let b = world_aabb(&shape("union(sphere(radius=2), at(sphere(radius=2), z=9))"), &f);
        assert!(near(vec6(b.lo), [-2.0, -2.0, -2.0], 1e-9) && near(vec6(b.hi), [2.0, 2.0, 11.0], 1e-9));
        let b = world_aabb(&shape("spin(cylinder(height=10, radius=1), axis=z, degrees=90)"), &f);
        assert!(near(vec6(b.lo), [-10.0, -1.0, -1.0], 1e-9) && near(vec6(b.hi), [0.0, 1.0, 1.0], 1e-9), "{b:?}");
    }

    /// The box is a bound: every mesh vertex of every part lies inside it
    /// (one cell of slack for noise-displaced shapes), in rest and posed.
    #[test]
    fn world_boxes_contain_every_meshed_surface() {
        for (name, src, pose) in [
            ("LAMP", LAMP, None),
            ("DESK_LAMP", DESK_LAMP, None),
            ("DESK_LAMP", DESK_LAMP, Some("Left.Folded")),
            ("SKELETON_v3", include_str!("../scripts/SKELETON_v3.md"), None),
        ] {
            let scene = crate::pipeline::compile_to_scene_posed(src, pose).expect("compiles");
            for layer in &scene.layers {
                for p in &layer.parts {
                    let expr  = p.shape.to_expr();
                    let frame = p.frame.to_frame();
                    let b     = world_aabb(&expr, &frame).grown(layer.voxel_size);
                    let mesh  = surface_nets(&expr, auto_cell(&expr, layer.voxel_size), layer.voxel_size);
                    for v in &mesh.positions {
                        let w = frame.apply_point(Vec3::new(v[0] as f64, v[1] as f64, v[2] as f64));
                        assert!(b.contains(w), "{name}/{}: vertex {w:?} outside {b:?}", p.name);
                    }
                }
            }
        }
    }

    // ── the report ────────────────────────────────────────────────────────

    #[test]
    fn lamp_report_is_pinned() {
        let r = from_source(LAMP, None).expect("compiles");
        let t = &r.things[0];
        assert_eq!((t.thing.as_str(), t.voxel_size, t.parts.len()), ("Lamp", 3.0, 4));

        let want = [
            ("Base", None,          [0.0, 6.0, 0.0],        [90.0, 12.0, 90.0]),
            ("Post", Some("Base"),  [0.0, 162.0, 0.0],      [12.0, 300.0, 12.0]),
            ("Arm",  Some("Post"),  [0.0, 304.759, 52.045], [8.0, 98.0, 8.0]),
            ("Bulb", Some("Arm"),   [0.0, 321.516, 98.093], [36.0, 36.0, 36.0]),
        ];
        for (name, parent, center, size) in want {
            let p = row(t, name);
            assert_eq!(p.parent.as_deref(), parent, "{name} parent");
            assert!(near(p.center, center, 0.01), "{name} center {:?}", p.center);
            assert!(near(p.size, size, 1e-9), "{name} size {:?}", p.size);
            assert_eq!(p.piece, 1);
        }
        // The arm leans 20° up toward the front; its own front turned east
        // (a `side` socket's tangent is the meridian). The bulb inherits it.
        for name in ["Arm", "Bulb"] {
            assert_eq!(direction_words(row(t, name).up),    "north 20° up", "{name} up");
            assert_eq!(direction_words(row(t, name).front), "east",         "{name} front");
        }
        for name in ["Base", "Post"] {
            assert_eq!(direction_words(row(t, name).up),    "up");
            assert_eq!(direction_words(row(t, name).front), "north");
        }
        assert_eq!(t.pieces.len(), 1, "one piece");
        let pairs: Vec<String> = t.touching.iter().map(|[a, b]| format!("{a}-{b}")).collect();
        assert_eq!(pairs, ["Base-Post", "Post-Arm", "Arm-Bulb"]);
    }

    #[test]
    fn lamp_text_form_is_pinned() {
        let text = from_source(LAMP, None).expect("compiles").to_text(false);
        let want = "\
Lamp · voxel 3 · 4 parts · 1 piece
bbox  x -45..45  y 0..340  z -45..116  (90 × 340 × 161)
part   on     center             size        up             front
Base   —      (0, 6, 0)          90×12×90    up             north
Post   Base   (0, 162, 0)        12×300×12   up             north
Arm    Post   (0, 304.8, 52.0)   8×98×8      north 20° up   east
Bulb   Arm    (0, 321.5, 98.1)   36×36×36    north 20° up   east
touching: Base–Post, Post–Arm, Arm–Bulb
";
        assert_eq!(text, want);
    }

    /// L3 item 8: `Fin[0..2]` means all of those touch; one floating member
    /// is named out of the family instead of riding along with it.
    #[test]
    fn a_partly_touching_family_names_its_touching_members() {
        let src = r##"
material M { color = "#888888" }
thing T {
    part Body { shape = box(width=40, height=10, depth=10), material = M }
    for i in 0..4 { part Fin[i] { shape = box(width=4, height=10, depth=4), material = M } }
    relation {
        Fin[0].bottom on Body.top shift=(-15, 0)
        Fin[1].bottom on Body.top shift=(-5, 0)
        Fin[2].bottom on Body.top shift=(5, 0)
        Fin[3].bottom on Body.top shift=(15, 0) gap=6
    }
}
print T
"##;
        let text = from_source(src, None).expect("compiles").to_text(false);
        assert!(text.contains("\ntouching: Body–Fin[0..2] ×3\n"), "{text}");
        let all = src.replace(" gap=6", "");
        let text = from_source(&all, None).expect("compiles").to_text(false);
        assert!(text.contains("\ntouching: Body–Fin[0..3] ×4\n"), "the whole family: {text}");
        // two non-adjacent members: both are named
        let split = src.replace("Fin[1].bottom on Body.top shift=(-5, 0)", "Fin[1].bottom on Body.top shift=(-5, 0) gap=6");
        let text = from_source(&split, None).expect("compiles").to_text(false);
        assert!(text.contains("\ntouching: Body–Fin[0]+Fin[2] ×2\n"), "{text}");
    }

    #[test]
    fn a_floating_part_is_reported_with_its_gap() {
        let r = from_source(include_str!("../scripts/LAMP_REPORT.md"), None).expect("compiles");
        let t = &r.things[0];
        assert_eq!(t.parts.len(), 5);
        assert_eq!(t.pieces.len(), 2, "the hat floats free of the lamp");
        let hat = &t.pieces[1];
        assert_eq!(hat.parts, ["Hat"]);
        assert_eq!(hat.from.as_deref(), Some("Bulb"));
        let gap = hat.gap.expect("a floating piece has a gap");
        assert!((gap - 9.0).abs() < 0.75, "hat placed 9 above the bulb, reported {gap}");
        assert_eq!(row(t, "Hat").piece, 2);
        assert_eq!(row(t, "Bulb").piece, 1);
        assert_eq!(t.touching.len(), 3, "the hat touches nothing");

        let text = r.to_text(false);
        let line = text.lines().find(|l| l.starts_with("piece 2: Hat — ")).expect("piece line");
        assert!(line.ends_with(" from Bulb"), "{line}");
        assert!(text.contains(" · 5 parts · 2 pieces\n"), "{text}");
    }

    const ROCKET: &str = "```moxi
material Hull { color = white }
material Trim { color = red }

thing Rocket(fins=4, length=200, radius=20) {
    part Body { shape = capsule(height=length, radius=radius), material = Hull }
    part Nose { shape = cone(height=60, radius=radius), material = Trim }
    relation { Nose.bottom on Body.top gap=-radius }

    for i in 0..fins {
        part Fin[i] { shape = box(width=4, height=50, depth=35), material = Trim }
        relation { Fin[i].south on Body.side(t=0.12, angle=360 / fins * i) }
    }

    resolve voxel_size = 2
}

print Rocket detail=low
```
";

    #[test]
    fn families_collapse_and_full_expands() {
        let r = from_source(ROCKET, None).expect("compiles");

        let short = r.to_text(false);
        assert!(short.contains("Fin[0..3] ×4"), "{short}");
        assert!(!short.contains("Fin[1]"), "collapsed rows hide the members:\n{short}");
        let fin = short.lines().find(|l| l.starts_with("Fin[0..3]")).expect("family row");
        let cells: Vec<&str> = fin.split_whitespace().collect();
        assert_eq!(&cells[..5], ["Fin[0..3]", "×4", "Body", "varies", "4×50×35"],
                   "one row: on Body, center varies, shared size: {fin}");
        assert!(short.contains("Body–Fin[0..3] ×4"), "contacts collapse too:\n{short}");

        let full = r.to_text(true);
        for k in 0..4 {
            assert!(full.lines().any(|l| l.starts_with(&format!("Fin[{k}] "))), "Fin[{k}] row:\n{full}");
        }
        assert!(!full.contains("] ×4"), "{full}");
        assert!(full.contains("Body–Fin[0]"), "{full}");
        // Four fin rows instead of one: the collapse is the only difference.
        assert_eq!(full.lines().count(), short.lines().count() + 3);

        // JSON is never collapsed.
        let json: serde_json::Value = serde_json::from_str(&r.to_json_pretty()).unwrap();
        assert_eq!(json["things"][0]["parts"].as_array().unwrap().len(), 6);
    }

    #[test]
    fn a_pose_changes_only_the_posed_rows() {
        let rest   = from_source(DESK_LAMP, None).expect("compiles");
        let folded = from_source(DESK_LAMP, Some("Left.Folded")).expect("compiles");
        assert_eq!(folded.pose.as_deref(), Some("Left.Folded"));
        let (a, b) = (&rest.things[0], &folded.things[0]);
        assert_eq!(a.parts.len(), b.parts.len());
        for p in &a.parts {
            let q = row(b, &p.name);
            let same = near(p.center, q.center, 1e-9) && near(p.up, q.up, 1e-9) && near(p.front, q.front, 1e-9);
            // Left.Folded bends the left lamp's arm; the right lamp is its
            // mirror image and follows. The tabletop and both bases stay.
            let posed = ["Upper", "Fore", "Shade", "Bulb"].iter().any(|s| p.name.ends_with(s));
            assert_eq!(!same, posed, "{}: moves in Left.Folded = {posed}", p.name);
            assert_eq!(p.size, q.size, "{} keeps its size", p.name);
            assert_eq!(p.parent, q.parent, "{} keeps its parent", p.name);
        }
        assert_eq!(a.pieces.len(), 1);
        assert_eq!(b.pieces.len(), 1, "a folded lamp is still one piece");
    }

    #[test]
    fn output_is_deterministic() {
        for src in [LAMP, DESK_LAMP, ROCKET] {
            let a = from_source(src, None).unwrap();
            let b = from_source(src, None).unwrap();
            assert_eq!(a.to_text(false), b.to_text(false));
            assert_eq!(a.to_text(true),  b.to_text(true));
            assert_eq!(a.to_json_pretty(), b.to_json_pretty());
        }
    }

    /// The report is a function of the `Scene` alone: send the scene through
    /// its own JSON and the report does not change. There is no source, no
    /// resolver and no solver in `build`'s signature to re-solve with.
    #[test]
    fn the_report_reads_only_the_scene() {
        for src in [LAMP, DESK_LAMP] {
            let scene = crate::pipeline::compile_to_scene(src).expect("compiles");
            let again = Scene::from_json(&scene.to_json_pretty()).expect("round trips");
            assert_eq!(build(&scene, None).to_json_pretty(), build(&again, None).to_json_pretty());
        }
    }

    #[test]
    fn json_carries_the_schema_and_vectors() {
        let r = from_source(LAMP, None).expect("compiles");
        let json: serde_json::Value = serde_json::from_str(&r.to_json_pretty()).unwrap();
        assert_eq!(json["schema"], SCHEMA);
        assert!(json["pose"].is_null());
        let arm = &json["things"][0]["parts"][2];
        assert_eq!(arm["name"], "Arm");
        assert_eq!(arm["parent"], "Post");
        assert_eq!(arm["size"].as_array().unwrap().len(), 3);
        let up = arm["up"].as_array().unwrap();
        assert!((up[1].as_f64().unwrap() - 20f64.to_radians().sin()).abs() < 1e-6);
        assert_eq!(json["things"][0]["touching"][0][0], "Base");
        assert_eq!(json["things"][0]["pieces"][0]["piece"], 1);
    }

    #[test]
    fn an_unknown_pose_is_the_compilers_error() {
        let errs = from_source(DESK_LAMP, Some("Nope")).expect_err("no such pose");
        assert!(errs[0].message.contains("Nope"), "{:?}", errs[0].message);
    }

    // ── --break ───────────────────────────────────────────────────────────

    const BREAK: &str = include_str!("../scripts/BREAK.md");

    /// A collar mated to the body that overlaps the head: breaking the head
    /// tears through it.
    const COLLAR: &str = r##"
material M { color = "#888888" }
thing Guy {
    part Body   { shape = capsule(height=50, radius=16), material = M }
    part Head   { shape = sphere(radius=14), material = M }
    part Collar { shape = cylinder(height=6, radius=18), material = M }
    part Hat    { shape = cone(height=10, radius=8), material = M }
    relation {
        Head.bottom   on Body.top gap=-4
        Collar.bottom on Body.top gap=-2
        Hat.bottom    on Head.top
    }
    breakable Head { hits=2 }
    resolve voxel_size = 1
}
print Guy
"##;

    fn names(v: &[String]) -> Vec<&str> { v.iter().map(String::as_str).collect() }

    #[test]
    fn break_splits_the_lump_off_as_its_own_piece() {
        let r = from_source_broken(BREAK, None, &["D.Head".into(), "D.EyeR".into()]).expect("compiles");
        assert_eq!(r.broken.as_deref(), Some(&["D.Head".to_string(), "D.EyeR".to_string()][..]));
        let t = &r.things[0];
        assert_eq!(t.pieces.len(), 3);
        assert_eq!(t.pieces[1].broken.as_deref(), Some("D.Head"));
        assert_eq!(names(&t.pieces[1].parts), ["D.Head", "D.HornR", "D.HornL", "D.EyeL"]);
        // A lump broken inside a lump is its own piece, not part of the head's.
        assert_eq!(names(&t.pieces[2].parts), ["D.EyeR"]);
        assert!(t.pieces[0].parts.contains(&"D.Body".to_string()));
        assert_eq!(row(t, "D.EyeL").piece, 2);
        assert_eq!(row(t, "D.EyeR").piece, 3);
        // The mates that broke are not tears.
        assert_eq!(t.torn.as_deref(), Some(&[][..]));
    }

    #[test]
    fn break_reports_the_contact_a_cut_tears_through() {
        let r = from_source_broken(COLLAR, None, &["Head".into()]).expect("compiles");
        let t = &r.things[0];
        let torn = t.torn.as_ref().expect("break mode");
        assert!(torn.iter().any(|p| p.contains(&"Collar".to_string()) && p.contains(&"Head".to_string())),
            "collar overlaps the head, got {torn:?}");
        assert!(!torn.iter().any(|p| p.contains(&"Body".to_string()) && p.contains(&"Head".to_string())),
            "the broken mate itself is not a tear");
        assert_eq!(names(&t.pieces[1].parts), ["Head", "Hat"]);
        assert!(r.to_text(false).contains("torn: Head–Collar"));
    }

    #[test]
    fn break_leaves_the_plain_report_alone() {
        let plain = from_source(BREAK, None).unwrap().to_json_pretty();
        assert!(!plain.contains("\"torn\"") && !plain.contains("\"broken\""));
        assert_eq!(from_source_broken(BREAK, None, &[]).unwrap().to_json_pretty(), plain);
        // Geometry is untouched: only pieces and tears differ.
        let cut = from_source_broken(BREAK, None, &["D.Head".into()]).unwrap();
        let (a, b) = (&from_source(BREAK, None).unwrap().things[0], &cut.things[0]);
        assert_eq!(a.touching, b.touching);
        assert_eq!(a.bbox.as_ref().map(|x| x.min), b.bbox.as_ref().map(|x| x.min));
    }

    #[test]
    fn break_names_are_checked_with_the_breakables_listed() {
        let errs = from_source_broken(BREAK, None, &["D.Body".into(), "Nope".into()]).expect_err("rejected");
        assert_eq!(errs.len(), 2);
        assert!(errs[0].message.contains("not declared `breakable`"));
        assert!(errs[1].message.contains("no part named `Nope`"));
        for e in &errs {
            assert!(e.message.contains("Breakable parts: D.Head, D.EyeR, D.EyeL, D.ArmR, D.ArmL, R.Stage1, R.Stage2"), "{}", e.message);
        }
        let none = from_source_broken(LAMP, None, &["Base".into()]).expect_err("lamp has none");
        assert!(none[0].message.contains("declares no `breakable`"));
    }

    #[test]
    fn break_report_of_the_demo_is_pinned() {
        let text = from_source_broken(BREAK, None, &["D.Head".into()]).unwrap().to_text(false);
        let tail: Vec<&str> = text.lines().skip_while(|l| !l.starts_with("piece")).collect();
        assert_eq!(tail, [
            "piece 1: Floor, D.Body, D.ArmR, D.ArmL, R.Pad, R.Stage1, R.Stage2, R.Capsule — stays with the root",
            "piece 2: D.Head, D.HornR, D.HornL, D.EyeR, D.EyeL — broken off at D.Head",
            "torn: none",
        ]);
    }

    #[test]
    fn states_on_together_show_each_look_and_the_overlap() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/STATES.md")).unwrap();
        let on = |names: &[&str]| from_source_full(&src, None, &[], &names.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        let r = on(&["Broken", "On"]).expect("both on");
        assert_eq!(r.states, Some(vec!["On".to_string(), "Broken".to_string()]), "in the order they combine");
        let t = &r.things[0];
        let look = |p: &str| t.looks.iter().find(|l| l.part == p).unwrap();
        assert_eq!((look("Shade").state.as_str(), look("Shade").look.material.as_deref()), ("On", Some("Lit")));
        assert_eq!((look("Bulb").state.as_str(), look("Bulb").look.material.as_deref()), ("Broken", Some("Ash")));
        assert_eq!(t.overlaps, vec!["`On` and `Broken` both change `Bulb`; `Broken` wins (its whole look of `Bulb`)".to_string()]);
        let text = r.to_text(false);
        assert!(text.starts_with("states On, Broken") && text.contains("look: Bulb — Ash #2a2622 (Broken)"), "{text}");
        // errors: an unknown state lists the states; two of one family
        let m = on(&["Calm", "Angry", "Nope"]).unwrap_err().iter().map(|e| e.message.clone()).collect::<Vec<_>>().join("\n");
        assert!(m.contains("--state: no state named `Nope` in any printed thing. States: On, Broken, Calm, Angry"), "{m}");
        assert!(m.contains("`Calm` and `Angry` are both in family `Eyes` — at most one state of a family is on"), "{m}");
        // a plain report says nothing about states
        let plain = from_source(&src, None).unwrap();
        assert!(plain.states.is_none() && plain.things[0].looks.is_empty());
        assert!(!plain.to_json_pretty().contains("\"looks\""));
    }
}