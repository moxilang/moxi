// src/bench.rs
//
// M3 — the bench corpus runner. `bench/cases.yaml` is a set of
// natural-language prompts with machine-checkable PROPERTY assertions on
// the compiled output — never golden voxel hashes (Phase B1 legitimately
// shifted voxel output by up to one voxel; a golden file would have
// flagged that as a regression). This module loads the corpus, compiles
// each case's hand-written reference solution (`bench/solutions/<id>.md`)
// via `pipeline::compile_source`, and checks its assertions.
//
// A case whose `gated_on` phase(s) haven't shipped yet is SKIPPED, not
// failed — it isn't broken, the language just doesn't support it yet
// (see LANDED_PHASES below). `moxi bench --check` only fails on a case
// that's expected to work today and doesn't.
//
// Color-tag convention: `clusters`/`coplanar` assertions filter voxels by
// a pseudo-color tag (e.g. `eye`) that isn't a real Moxi palette name.
// Rather than lean on the compiler's silent unrecognized-name-to-white
// fallback (`colors::resolve_color`), reference solutions paint a
// reserved literal hex from COLOR_ALIASES below, so the convention
// doesn't depend on incidental compiler behavior that could change.

use serde::Deserialize;
use std::collections::{HashMap, HashSet};

use crate::pipeline::{self, CompileError, WorldOutput};

// ── Corpus schema ────────────────────────────────────────────────────────

pub type Corpus = Vec<Case>;

#[derive(Debug, Deserialize)]
pub struct Case {
    pub id:     String,
    #[allow(dead_code)] // documentation for the human reader, not checked
    pub prompt: String,
    #[serde(default)]
    pub gated_on: Option<GatedOn>,
    #[serde(default)]
    #[allow(dead_code)]
    pub note: Option<String>,
    pub assert: Assertions,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum GatedOn {
    One(String),
    Many(Vec<String>),
}

impl GatedOn {
    fn phases(&self) -> &[String] {
        match self {
            GatedOn::One(s) => std::slice::from_ref(s),
            GatedOn::Many(v) => v,
        }
    }
}

/// Phases that have actually shipped, checked against every `gated_on`
/// value used in `bench/cases.yaml` (currently: P1, P3, E, I). Only P1
/// (the `shift=` qualifier) has landed as of this module's introduction.
const LANDED_PHASES: &[&str] = &["P1"];

fn is_landed(case: &Case) -> bool {
    case.gated_on
        .as_ref()
        .map(|g| g.phases().iter().all(|p| LANDED_PHASES.contains(&p.as_str())))
        .unwrap_or(true)
}

#[derive(Debug, Default, Deserialize)]
pub struct Assertions {
    pub compiles:           Option<bool>,
    pub layers:              Option<IntAssertion>,
    pub total_voxels:        Option<IntAssertion>,
    pub bbox_x:              Option<[i32; 2]>,
    pub bbox_y:              Option<[i32; 2]>,
    pub bbox_z:              Option<[i32; 2]>,
    pub distinct_colors:     Option<IntAssertion>,
    pub parts_min:           Option<i64>,
    pub bilateral_symmetry:  Option<f64>,
    pub error_stage:         Option<String>,
    pub clusters:            Option<ClusterAssertion>,
    pub coplanar:            Option<CoplanarAssertion>,
    pub centroid_spread:     Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct ClusterAssertion {
    pub color: String,
    pub n:     IntAssertion,
}

#[derive(Debug, Deserialize)]
pub struct CoplanarAssertion {
    pub color: String,
    pub axis:  Axis,
    pub tol:   f64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis { X, Y, Z }

impl Axis {
    fn pick(self, x: i32, y: i32, z: i32) -> i32 {
        match self { Axis::X => x, Axis::Y => y, Axis::Z => z }
    }
}

/// Either a bare integer (`5`, meaning `== 5`) or a comparator string
/// (`">=5"`, `"<=5"`, `"==5"`).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum IntAssertion {
    Exact(i64),
    Cmp(String),
}

#[derive(Debug, Clone, Copy)]
enum Cmp { Ge, Le, Eq }

fn parse_cmp(s: &str) -> Result<(Cmp, i64), String> {
    let (cmp, rest) = if let Some(r) = s.strip_prefix(">=") {
        (Cmp::Ge, r)
    } else if let Some(r) = s.strip_prefix("<=") {
        (Cmp::Le, r)
    } else if let Some(r) = s.strip_prefix("==") {
        (Cmp::Eq, r)
    } else {
        (Cmp::Eq, s)
    };
    rest.trim().parse::<i64>().map(|n| (cmp, n)).map_err(|_| format!("bad comparator value: {s:?}"))
}

impl IntAssertion {
    fn check(&self, actual: i64) -> bool {
        let (cmp, n) = match self {
            IntAssertion::Exact(n) => (Cmp::Eq, *n),
            IntAssertion::Cmp(s) => match parse_cmp(s) {
                Ok(pair) => pair,
                Err(_) => return false,
            },
        };
        match cmp {
            Cmp::Ge => actual >= n,
            Cmp::Le => actual <= n,
            Cmp::Eq => actual == n,
        }
    }

    fn describe(&self) -> String {
        match self {
            IntAssertion::Exact(n) => format!("== {n}"),
            IntAssertion::Cmp(s) => s.clone(),
        }
    }
}

// ── Color-tag convention ────────────────────────────────────────────────

/// Reserved hex values for bench-only pseudo-colors used by `clusters`/
/// `coplanar` assertions (e.g. `color: eye`). These are NOT part of the
/// language's palette (`src/colors.rs`) — a reference solution that needs
/// one paints the literal hex directly (`color = "#ff00ff"`), so the
/// convention doesn't depend on the compiler's unrecognized-name fallback.
pub const COLOR_ALIASES: &[(&str, &str)] = &[
    ("eye", "#ff00ff"),
    ("leg", "#00ffff"),
    ("hut", "#ffff00"),
];

fn resolve_tag(tag: &str) -> String {
    COLOR_ALIASES
        .iter()
        .find(|(name, _)| *name == tag)
        .map(|(_, hex)| hex.to_string())
        .unwrap_or_else(|| tag.to_string())
}

// ── Evaluation ───────────────────────────────────────────────────────────

pub struct AssertionResult {
    pub key:      &'static str,
    pub expected: String,
    pub actual:   String,
    pub pass:     bool,
}

fn r(key: &'static str, expected: impl Into<String>, actual: impl Into<String>, pass: bool) -> AssertionResult {
    AssertionResult { key, expected: expected.into(), actual: actual.into(), pass }
}

pub enum CaseStatus {
    Pass,
    Fail,
    Skipped(String),
}

pub struct CaseReport {
    pub id:      String,
    pub status:  CaseStatus,
    pub results: Vec<AssertionResult>,
}

/// Evaluate one case's assertions against its compile outcome. Always
/// returns one `AssertionResult` per populated assertion key, even when
/// the compile failed and every other assertion is therefore moot — the
/// report is never silently truncated.
pub fn evaluate_case(a: &Assertions, outcome: &Result<WorldOutput, Vec<CompileError>>) -> Vec<AssertionResult> {
    let mut out = Vec::new();

    if let Some(expected) = a.compiles {
        out.push(r("compiles", expected.to_string(), outcome.is_ok().to_string(), outcome.is_ok() == expected));
    }

    let world = match outcome {
        Ok(w) => w,
        Err(errors) => {
            let na = |key: &'static str| r(key, "n/a", "N/A (compile failed)", false);
            if a.layers.is_some() { out.push(na("layers")); }
            if a.total_voxels.is_some() { out.push(na("total_voxels")); }
            if a.bbox_x.is_some() { out.push(na("bbox_x")); }
            if a.bbox_y.is_some() { out.push(na("bbox_y")); }
            if a.bbox_z.is_some() { out.push(na("bbox_z")); }
            if a.distinct_colors.is_some() { out.push(na("distinct_colors")); }
            if a.parts_min.is_some() { out.push(na("parts_min")); }
            if a.bilateral_symmetry.is_some() { out.push(na("bilateral_symmetry")); }
            if a.clusters.is_some() { out.push(na("clusters")); }
            if a.coplanar.is_some() { out.push(na("coplanar")); }
            if a.centroid_spread.is_some() { out.push(na("centroid_spread")); }
            if let Some(expected) = &a.error_stage {
                let hit = errors.iter().any(|e| &e.stage == expected);
                let actual = errors.iter().map(|e| e.stage.as_str()).collect::<Vec<_>>().join(",");
                out.push(r("error_stage", expected.clone(), actual, hit));
            }
            return out;
        }
    };

    if let Some(ia) = &a.layers {
        let actual = world.layers.len() as i64;
        out.push(r("layers", ia.describe(), format!("{actual}"), ia.check(actual)));
    }
    if let Some(ia) = &a.total_voxels {
        let actual = world.total as i64;
        out.push(r("total_voxels", ia.describe(), format!("{actual}"), ia.check(actual)));
    }
    if let Some([lo, hi]) = a.bbox_x { out.push(bbox_result("bbox_x", world, 0, lo, hi)); }
    if let Some([lo, hi]) = a.bbox_y { out.push(bbox_result("bbox_y", world, 1, lo, hi)); }
    if let Some([lo, hi]) = a.bbox_z { out.push(bbox_result("bbox_z", world, 2, lo, hi)); }
    if let Some(ia) = &a.distinct_colors {
        let actual = world.voxels.iter().map(|v| &v.color).collect::<HashSet<_>>().len() as i64;
        out.push(r("distinct_colors", ia.describe(), format!("{actual}"), ia.check(actual)));
    }
    if let Some(min) = a.parts_min {
        let actual: i64 = world.layers.iter().map(|l| l.parts as i64).sum();
        out.push(r("parts_min", format!(">= {min}"), format!("{actual}"), actual >= min));
    }
    if let Some(tol) = a.bilateral_symmetry {
        out.push(bilateral_symmetry_result(world, tol));
    }
    if let Some(c) = &a.clusters {
        out.push(clusters_result(world, c));
    }
    if let Some(c) = &a.coplanar {
        out.push(coplanar_result(world, c));
    }
    if let Some(min) = a.centroid_spread {
        out.push(centroid_spread_result(world, min));
    }

    out
}

fn bbox_result(key: &'static str, world: &WorldOutput, axis: usize, lo: i32, hi: i32) -> AssertionResult {
    let extent = world.bounds[1][axis] - world.bounds[0][axis];
    r(key, format!("extent in [{lo}, {hi}]"), format!("extent == {extent}"), extent >= lo && extent <= hi)
}

/// Reflected-position matching about the bbox x-midline: for each voxel,
/// its mirror image (2*mid_x - x, y, z) should also be occupied. A naive
/// left/right COUNT split is biased whenever the mirror axis passes
/// through an odd number of x-columns (the single straddling column has
/// to land wholly on one side or the other, even for a perfectly
/// symmetric shape) — reflection avoids that artifact entirely, since a
/// voxel sitting exactly on the midline reflects to itself.
fn bilateral_symmetry_result(world: &WorldOutput, tolerance: f64) -> AssertionResult {
    let mid_x = (world.bounds[0][0] + world.bounds[1][0]) as f64 / 2.0;
    let occupied: HashSet<(i32, i32, i32)> = world.voxels.iter().map(|v| (v.x, v.y, v.z)).collect();

    let total = world.voxels.len().max(1) as f64;
    let mismatched = world
        .voxels
        .iter()
        .filter(|v| {
            let mirrored_x = (2.0 * mid_x - v.x as f64).round() as i32;
            !occupied.contains(&(mirrored_x, v.y, v.z))
        })
        .count();
    let diff = mismatched as f64 / total;
    r(
        "bilateral_symmetry",
        format!("<= {tolerance}"),
        format!("{diff:.4} ({mismatched}/{} voxels unmatched)", world.voxels.len()),
        diff <= tolerance,
    )
}

fn voxel_coords_by_color<'a>(world: &'a WorldOutput, hex: &str) -> Vec<&'a crate::types::Voxel> {
    world.voxels.iter().filter(|v| v.color == hex).collect()
}

fn clusters_result(world: &WorldOutput, c: &ClusterAssertion) -> AssertionResult {
    let hex = resolve_tag(&c.color);
    let matched = voxel_coords_by_color(world, &hex);
    let coords: HashSet<(i32, i32, i32)> = matched.iter().map(|v| (v.x, v.y, v.z)).collect();
    let n = count_components(&coords) as i64;
    r(
        "clusters",
        format!("clusters(color={}) n {}", c.color, c.n.describe()),
        format!("clusters(color={}) n == {n}", c.color),
        c.n.check(n),
    )
}

/// 6-connected component count over a set of voxel coordinates (flood
/// fill via BFS over face-neighbors).
fn count_components(coords: &HashSet<(i32, i32, i32)>) -> usize {
    let mut visited: HashSet<(i32, i32, i32)> = HashSet::new();
    let mut count = 0;
    for &start in coords {
        if visited.contains(&start) { continue; }
        count += 1;
        let mut stack = vec![start];
        visited.insert(start);
        while let Some((x, y, z)) = stack.pop() {
            for (dx, dy, dz) in [(1,0,0),(-1,0,0),(0,1,0),(0,-1,0),(0,0,1),(0,0,-1)] {
                let n = (x + dx, y + dy, z + dz);
                if coords.contains(&n) && !visited.contains(&n) {
                    visited.insert(n);
                    stack.push(n);
                }
            }
        }
    }
    count
}

fn coplanar_result(world: &WorldOutput, c: &CoplanarAssertion) -> AssertionResult {
    let hex = resolve_tag(&c.color);
    let matched = voxel_coords_by_color(world, &hex);
    if matched.is_empty() {
        return r("coplanar", format!("extent(axis={:?}) <= {}", c.axis, c.tol), "no voxels matched color", false);
    }
    let vals: Vec<i32> = matched.iter().map(|v| c.axis.pick(v.x, v.y, v.z)).collect();
    let extent = vals.iter().max().unwrap() - vals.iter().min().unwrap();
    r(
        "coplanar",
        format!("extent(axis={:?}) <= {}", c.axis, c.tol),
        format!("extent == {extent}"),
        (extent as f64) <= c.tol,
    )
}

fn centroid_spread_result(world: &WorldOutput, min: f64) -> AssertionResult {
    let mut groups: HashMap<&str, (f64, f64, f64, usize)> = HashMap::new();
    for v in &world.voxels {
        let e = groups.entry(v.color.as_str()).or_insert((0.0, 0.0, 0.0, 0));
        e.0 += v.x as f64; e.1 += v.y as f64; e.2 += v.z as f64; e.3 += 1;
    }
    let centroids: Vec<(f64, f64, f64)> = groups
        .values()
        .map(|(sx, sy, sz, n)| (sx / *n as f64, sy / *n as f64, sz / *n as f64))
        .collect();

    if centroids.len() < 2 {
        return r("centroid_spread", format!("> {min}"), format!("only {} distinct color(s)", centroids.len()), false);
    }

    let mut min_dist = f64::MAX;
    for i in 0..centroids.len() {
        for j in (i + 1)..centroids.len() {
            let (ax, ay, az) = centroids[i];
            let (bx, by, bz) = centroids[j];
            let d = ((ax - bx).powi(2) + (ay - by).powi(2) + (az - bz).powi(2)).sqrt();
            min_dist = min_dist.min(d);
        }
    }
    r("centroid_spread", format!("> {min}"), format!("{min_dist:.2}"), min_dist > min)
}

// ── Corpus runner ────────────────────────────────────────────────────────

pub fn run(dir: &str) -> Vec<CaseReport> {
    let cases_path = format!("{dir}/cases.yaml");
    let text = std::fs::read_to_string(&cases_path)
        .unwrap_or_else(|e| { eprintln!("error: cannot read {cases_path}: {e}"); std::process::exit(1); });
    let corpus: Corpus = serde_yaml::from_str(&text)
        .unwrap_or_else(|e| { eprintln!("error: cannot parse {cases_path}: {e}"); std::process::exit(1); });

    corpus.iter().map(|case| evaluate(dir, case)).collect()
}

fn evaluate(dir: &str, case: &Case) -> CaseReport {
    if !is_landed(case) {
        let phases = case.gated_on.as_ref().map(|g| g.phases().join(",")).unwrap_or_default();
        return CaseReport { id: case.id.clone(), status: CaseStatus::Skipped(format!("gated on {phases}")), results: Vec::new() };
    }

    let solution_path = format!("{dir}/solutions/{}.md", case.id);
    let source = match std::fs::read_to_string(&solution_path) {
        Ok(s) => s,
        Err(_) => {
            return CaseReport {
                id: case.id.clone(),
                status: CaseStatus::Fail,
                results: vec![r("solution", "a solution file", format!("missing: {solution_path}"), false)],
            };
        }
    };

    let outcome = pipeline::compile_source(&source);
    let results = evaluate_case(&case.assert, &outcome);
    let status = if results.iter().all(|res| res.pass) { CaseStatus::Pass } else { CaseStatus::Fail };
    CaseReport { id: case.id.clone(), status, results }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Voxel;

    fn voxel(x: i32, y: i32, z: i32, color: &str) -> Voxel {
        Voxel { x, y, z, color: color.to_string() }
    }

    fn world(voxels: Vec<Voxel>, bounds: [[i32; 3]; 2]) -> WorldOutput {
        WorldOutput {
            total: voxels.len(),
            layers: vec![pipeline::LayerInfo { name: "T".into(), dims: [1, 1, 1], voxels: voxels.len(), parts: 1 }],
            voxels,
            bounds,
        }
    }

    #[test]
    fn int_assertion_parses_bare_and_comparator_forms() {
        assert!(IntAssertion::Exact(5).check(5));
        assert!(!IntAssertion::Exact(5).check(4));
        assert!(IntAssertion::Cmp(">=5".into()).check(7));
        assert!(!IntAssertion::Cmp(">=5".into()).check(3));
        assert!(IntAssertion::Cmp("<=5".into()).check(5));
        assert!(IntAssertion::Cmp("==5".into()).check(5));
    }

    #[test]
    fn gated_on_deserializes_string_and_list() {
        let one: Case = serde_yaml::from_str(
            "id: a\nprompt: p\ngated_on: P1\nassert: {compiles: true}\n",
        ).unwrap();
        assert_eq!(one.gated_on.unwrap().phases(), &["P1".to_string()]);

        let many: Case = serde_yaml::from_str(
            "id: b\nprompt: p\ngated_on: [P1, P3]\nassert: {compiles: true}\n",
        ).unwrap();
        assert_eq!(many.gated_on.unwrap().phases(), &["P1".to_string(), "P3".to_string()]);

        let none: Case = serde_yaml::from_str(
            "id: c\nprompt: p\nassert: {compiles: true}\n",
        ).unwrap();
        assert!(none.gated_on.is_none());
    }

    #[test]
    fn landed_phases_requires_every_gate_to_be_landed() {
        let landed: Case = serde_yaml::from_str("id: a\nprompt: p\ngated_on: P1\nassert: {}\n").unwrap();
        assert!(is_landed(&landed));

        let not_landed: Case = serde_yaml::from_str("id: b\nprompt: p\ngated_on: [P1, P3]\nassert: {}\n").unwrap();
        assert!(!is_landed(&not_landed));

        let ungated: Case = serde_yaml::from_str("id: c\nprompt: p\nassert: {}\n").unwrap();
        assert!(is_landed(&ungated));
    }

    #[test]
    fn bilateral_symmetry_survives_a_one_voxel_shift_but_catches_lopsided() {
        // Symmetric: 5 voxels each side of the midline (bounds -5..5 -> mid 0).
        let mut voxels = Vec::new();
        for x in -5..0 { voxels.push(voxel(x, 0, 0, "#fff")); }
        for x in 0..5 { voxels.push(voxel(x, 0, 0, "#fff")); }
        let w = world(voxels, [[-5, 0, 0], [4, 0, 0]]);
        let result = bilateral_symmetry_result(&w, 0.02);
        assert!(result.pass);

        // Lopsided: all voxels on one side.
        let voxels = vec![voxel(-4, 0, 0, "#fff"); 10];
        let w = world(voxels, [[-5, 0, 0], [4, 0, 0]]);
        let result = bilateral_symmetry_result(&w, 0.02);
        assert!(!result.pass);
    }

    #[test]
    fn clusters_counts_6_connected_components() {
        let mut voxels = Vec::new();
        for x in 0..3 { voxels.push(voxel(x, 0, 0, "#ff00ff")); }
        for x in 10..13 { voxels.push(voxel(x, 0, 0, "#ff00ff")); }
        let w = world(voxels, [[0, 0, 0], [12, 0, 0]]);

        let two = ClusterAssertion { color: "eye".into(), n: IntAssertion::Exact(2) };
        assert!(clusters_result(&w, &two).pass);

        let one = ClusterAssertion { color: "eye".into(), n: IntAssertion::Exact(1) };
        assert!(!clusters_result(&w, &one).pass);

        // No voxels of that color at all: 0 components, no panic.
        let empty = world(vec![voxel(0, 0, 0, "#000000")], [[0, 0, 0], [0, 0, 0]]);
        let none = ClusterAssertion { color: "eye".into(), n: IntAssertion::Exact(0) };
        assert!(clusters_result(&empty, &none).pass);
    }

    #[test]
    fn coplanar_checks_extent_along_one_axis() {
        let voxels = vec![voxel(0, 0, 4, "#ff00ff"), voxel(0, 0, 5, "#ff00ff")];
        let w = world(voxels, [[0, 0, 4], [0, 0, 5]]);
        let thin = CoplanarAssertion { color: "eye".into(), axis: Axis::Z, tol: 2.0 };
        assert!(coplanar_result(&w, &thin).pass);

        let voxels = vec![voxel(0, 0, 0, "#ff00ff"), voxel(0, 0, 10, "#ff00ff")];
        let w = world(voxels, [[0, 0, 0], [0, 0, 10]]);
        let spread = CoplanarAssertion { color: "eye".into(), axis: Axis::Z, tol: 2.0 };
        assert!(!coplanar_result(&w, &spread).pass);
    }

    #[test]
    fn centroid_spread_needs_at_least_two_colors() {
        let voxels = vec![voxel(0, 0, 0, "#000000"), voxel(0, 0, 0, "#000000")];
        let w = world(voxels, [[0, 0, 0], [0, 0, 0]]);
        assert!(!centroid_spread_result(&w, 1.0).pass);

        let voxels = vec![voxel(0, 0, 0, "#000000"), voxel(10, 0, 0, "#ffffff")];
        let w = world(voxels, [[0, 0, 0], [10, 0, 0]]);
        assert!(centroid_spread_result(&w, 5.0).pass);
        assert!(!centroid_spread_result(&w, 20.0).pass);
    }

    #[test]
    fn bbox_extent_not_absolute_position() {
        let w = world(vec![voxel(0, 0, 0, "#fff")], [[0, 0, 0], [0, 20, 0]]);
        let pass = bbox_result("bbox_y", &w, 1, 8, 30);
        assert!(pass.pass);
        let fail = bbox_result("bbox_y", &w, 1, 8, 15);
        assert!(!fail.pass);
    }

    #[test]
    fn end_to_end_against_real_compile_source() {
        let source = r#"
material Body { color = ivory }
material Trim { color = black }
thing T {
    part A { shape = sphere(radius=3), material = Body }
    part B { shape = sphere(radius=1), material = Trim }
    relation { B.bottom on A.top }
    resolve voxel_size = 1.0
}
print T detail=low
"#;
        let outcome = pipeline::compile_source(source);
        let a = Assertions {
            compiles: Some(true),
            layers: Some(IntAssertion::Exact(1)),
            distinct_colors: Some(IntAssertion::Cmp(">=2".into())),
            ..Default::default()
        };
        let results = evaluate_case(&a, &outcome);
        assert!(results.iter().all(|r| r.pass), "expected all to pass: {:?}", results.iter().map(|r| (r.key, &r.expected, &r.actual, r.pass)).collect::<Vec<_>>());
    }
}
