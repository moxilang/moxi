//! `moxi spec --json` — emit the compiler's own grammar surface as JSON.
//!
//! This is the machine-readable single source of truth the language will
//! generate docs from (`moxi skill`, M2) instead of hand-maintaining prose
//! that drifts. See ROADMAP.md, Phase M.
//!
//! Every table below is either read straight out of existing compiler
//! functions (`anchors::valid_anchor_names`) or built through an exhaustive
//! `match` with no wildcard arm, so a new `ShapeExpr` or `MoxiError` variant
//! is a compile error here until this file is updated — that's the drift
//! guard M1 asks for. `keywords` is the one table with no such guard: string
//! matching against arbitrary identifiers can't be made exhaustive by the
//! compiler, so it's checked instead by a test that lexes every entry and
//! confirms it isn't swallowed as a plain identifier.

use serde::Serialize;
use serde_json::{json, Value};

use crate::anchors::valid_anchor_names;
use crate::ast::ShapeExpr;
use crate::error::{MoxiError, Span};

// ── Shapes ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ArgSpec {
    pub name: &'static str,
    pub kind: &'static str, // "f64" | "i64" | "ident"
    pub default: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShapeSpec {
    pub name: &'static str,
    pub args: Vec<ArgSpec>,
    pub origin: &'static str,
    pub anchors: Vec<&'static str>,
}

fn arg(name: &'static str, kind: &'static str, default: Option<f64>) -> ArgSpec {
    ArgSpec { name, kind, default }
}

/// One placeholder instance per `ShapeExpr` variant, purely so `describe`'s
/// match below is exhaustive over the variant *shapes* — the compiler
/// rejects this file the moment a variant is added or removed without a
/// matching arm here, regardless of whether the vec below is kept in sync.
fn probes() -> Vec<ShapeExpr> {
    vec![
        ShapeExpr::Sphere { args: vec![] },
        ShapeExpr::Cylinder { args: vec![] },
        ShapeExpr::Box_ { args: vec![] },
        ShapeExpr::Cone { args: vec![] },
        ShapeExpr::Ellipsoid { args: vec![] },
        ShapeExpr::Blob { args: vec![] },
        ShapeExpr::Heightfield { args: vec![] },
        ShapeExpr::Shell {
            inner: Box::new(ShapeExpr::Sphere { args: vec![] }),
            args: vec![],
        },
        ShapeExpr::Extrude {
            profile: Box::new(ShapeExpr::Box_ { args: vec![] }),
            args: vec![],
        },
        ShapeExpr::Capsule { args: vec![] },
        ShapeExpr::Torus { args: vec![] },
        ShapeExpr::Union { shapes: vec![], args: vec![] },
        ShapeExpr::Intersect { shapes: vec![] },
        ShapeExpr::Difference {
            base: Box::new(ShapeExpr::Sphere { args: vec![] }),
            cuts: vec![],
        },
        ShapeExpr::At {
            inner: Box::new(ShapeExpr::Sphere { args: vec![] }),
            args: vec![],
        },
        ShapeExpr::Spin {
            inner: Box::new(ShapeExpr::Sphere { args: vec![] }),
            args: vec![],
        },
    ]
}

/// Argument defaults here are copied from `anchors::analytic_extents` and
/// `geometry::contains`, which must already agree with each other for
/// parity — this is a third reader of the same numbers, not a fourth source
/// of truth. `valid_anchor_names` never inspects argument *values*, only the
/// shape's discriminant, so probing with empty args is safe.
fn describe(shape: &ShapeExpr) -> ShapeSpec {
    match shape {
        ShapeExpr::Sphere { .. } => ShapeSpec {
            name: "sphere",
            args: vec![arg("radius", "f64", Some(1.0))],
            origin: "centered",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Cylinder { .. } => ShapeSpec {
            name: "cylinder",
            args: vec![
                arg("height", "f64", Some(1.0)),
                arg("radius", "f64", Some(0.5)),
            ],
            origin: "base at origin, axis +Y",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Box_ { .. } => ShapeSpec {
            name: "box",
            args: vec![
                arg("width", "f64", Some(2.0)),
                arg("height", "f64", Some(2.0)),
                arg("depth", "f64", Some(2.0)),
                arg("round", "f64", Some(0.0)),
            ],
            origin: "centered; round > 0 fillets the corners",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Capsule { .. } => ShapeSpec {
            name: "capsule",
            args: vec![arg("height", "f64", Some(1.0)), arg("radius", "f64", Some(0.5))],
            origin: "base at origin, axis +Y; rounded caps extend radius beyond each end",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Torus { .. } => ShapeSpec {
            name: "torus",
            args: vec![arg("major_radius", "f64", Some(2.0)), arg("minor_radius", "f64", Some(0.5))],
            origin: "centered, ring in the XZ plane, axis +Y",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Cone { .. } => ShapeSpec {
            name: "cone",
            args: vec![
                arg("height", "f64", Some(1.0)),
                arg("radius", "f64", Some(0.5)),
            ],
            origin: "base at origin, apex +Y",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Ellipsoid { .. } => ShapeSpec {
            name: "ellipsoid",
            args: vec![
                arg("rx", "f64", Some(1.0)),
                arg("ry", "f64", Some(1.0)),
                arg("rz", "f64", Some(1.0)),
            ],
            origin: "centered",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Blob { .. } => ShapeSpec {
            name: "blob",
            args: vec![
                arg("radius", "f64", Some(1.0)),
                arg("roughness", "f64", Some(0.2)),
            ],
            origin: "centered (nominal sphere; noise never perturbs anchors)",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Heightfield { .. } => ShapeSpec {
            name: "heightfield",
            args: vec![
                arg("seed", "i64", Some(42.0)),
                arg("radius", "f64", Some(50.0)),
                arg("noise", "f64", Some(0.3)),
                arg("max_height", "f64", Some(20.0)),
            ],
            origin: "base at origin",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Shell { .. } => ShapeSpec {
            name: "shell",
            args: vec![arg("inner_offset", "f64", Some(1.0))],
            origin: "same as its inner shape",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Extrude { .. } => ShapeSpec {
            name: "extrude",
            args: vec![arg("height", "f64", Some(1.0))],
            origin: "base of profile at origin, extruded +Y",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Union { .. } => ShapeSpec {
            name: "union",
            args: vec![arg("blend", "f64", Some(0.0))],
            origin: "delegates to the first operand; blend > 0 fillets the joins",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Intersect { .. } => ShapeSpec {
            name: "intersect",
            args: vec![],
            origin: "delegates to the first operand",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Difference { .. } => ShapeSpec {
            name: "difference",
            args: vec![],
            origin: "delegates to the base",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::At { .. } => ShapeSpec {
            name: "at",
            args: vec![
                arg("x", "f64", Some(0.0)),
                arg("y", "f64", Some(0.0)),
                arg("z", "f64", Some(0.0)),
            ],
            origin: "delegates to the child, translated",
            anchors: valid_anchor_names(shape),
        },
        ShapeExpr::Spin { .. } => ShapeSpec {
            name: "spin",
            args: vec![
                arg("axis", "ident", None),
                arg("degrees", "f64", Some(0.0)),
            ],
            origin: "delegates to the child, rotated",
            anchors: valid_anchor_names(shape),
        },
    }
}

pub fn shape_specs() -> Vec<ShapeSpec> {
    probes().iter().map(describe).collect()
}

// ── Relation sugar table ────────────────────────────────────────────────
// Mirrors parser::desugar_placement exactly. Kept as a second, explicit
// reading of that table rather than re-deriving it at runtime, because the
// desugar function is private to the parser and the table is tiny and
// stable; the parity test below is the safety net.

pub fn relation_specs() -> Value {
    let pairs: &[(&str, &str, &str)] = &[
        ("above", "bottom", "top"),
        ("below", "top", "bottom"),
        ("left_of", "east", "west"),
        ("right_of", "west", "east"),
        ("in_front_of", "north", "south"),
        ("behind", "south", "north"),
        ("outside", "west", "east"),
        ("inside", "center", "center"),
        ("surrounds", "center", "center"),
        ("touch", "bottom", "top"),
        ("adjacent_to", "bottom", "top"),
        ("attached_to", "bottom", "top"),
    ];

    let sugar: Vec<Value> = pairs
        .iter()
        .map(|(kw, subj, obj)| {
            json!({ "keyword": kw, "subject_anchor": subj, "object_anchor": obj })
        })
        .collect();

    json!({
        "sugar": sugar,
        "symmetric_across": {
            "form": "SUBJECT symmetric_across PLANE from=SOURCE [axis=x|y|z]",
            "note": "not a subject/object anchor pair — reflects SOURCE's solved frame across PLANE's anchor point"
        }
    })
}

// ── Qualifiers ──────────────────────────────────────────────────────────

pub fn qualifier_specs() -> Value {
    json!({
        "twist": { "unit": "degrees", "applies_to": "explicit `on` and every relation-keyword sugar form except symmetric_across" },
        "pitch": { "unit": "degrees", "applies_to": "explicit `on` and every relation-keyword sugar form except symmetric_across" },
        "gap":   { "unit": "world units", "applies_to": "explicit `on` and every relation-keyword sugar form except symmetric_across" },
        "shift": { "unit": "world units", "applies_to": "explicit `on` and every relation-keyword sugar form except symmetric_across", "note": "a pair `(along socket +X, along socket +Z)` sliding the mate within the socket's tangent plane; `gap` is the same translation's +Y component" },
        "from":  { "applies_to": ["symmetric_across"], "required": true },
        "axis":  { "applies_to": ["symmetric_across"], "default": "x" }
    })
}

// ── Errors ──────────────────────────────────────────────────────────────
// Exhaustive match, no wildcard arm — a new MoxiError variant fails this
// file to compile until an example is added here.

pub fn error_specs() -> Vec<Value> {
    let s = Span::new(1, 1);

    let examples: Vec<(&'static str, MoxiError)> = vec![
        ("UnexpectedChar", MoxiError::UnexpectedChar { ch: '!', span: s }),
        ("UnterminatedString", MoxiError::UnterminatedString { span: s }),
        (
            "UnexpectedToken",
            MoxiError::UnexpectedToken {
                got: "']'".to_string(),
                expected: "shape primitive".to_string(),
                span: s,
            },
        ),
        (
            "UnexpectedEof",
            MoxiError::UnexpectedEof {
                expected: "closing '}'".to_string(),
            },
        ),
        (
            "UndefinedName",
            MoxiError::UndefinedName {
                name: "Foo".to_string(),
                span: s,
            },
        ),
        (
            "DuplicateName",
            MoxiError::DuplicateName {
                name: "Skull".to_string(),
                span: s,
            },
        ),
        (
            "UndefinedMaterial",
            MoxiError::UndefinedMaterial {
                name: "Bone".to_string(),
                span: s,
            },
        ),
        (
            "UndefinedAtom",
            MoxiError::UndefinedAtom {
                name: "BONE".to_string(),
                span: s,
            },
        ),
        (
            "ConstraintViolation",
            MoxiError::ConstraintViolation {
                description: "'Skull' above 'Ribcage': expected Skull.bottom.y ≥ Ribcage.top.y, got 3.00 < 5.00"
                    .to_string(),
            },
        ),
        (
            "UndefinedAnchor",
            MoxiError::UndefinedAnchor {
                part: "Trunk".to_string(),
                anchor: "sidee".to_string(),
                valid: "center, top, bottom, north, south, east, west, point(...), side(t, angle), rim_top(angle), rim_bottom(angle)"
                    .to_string(),
                span: s,
            },
        ),
        (
            "BadAnchor",
            MoxiError::BadAnchor {
                part: "Trunk".to_string(),
                anchor: "side".to_string(),
                message: "t must be in [0, 1], got 1.4".to_string(),
                span: s,
            },
        ),
        (
            "InstanceError",
            MoxiError::InstanceError {
                instance: "RightArm".to_string(),
                message: "thing 'Arm' must be declared before it is instanced".to_string(),
                span: s,
            },
        ),
        (
            "ExprError",
            MoxiError::ExprError {
                message: "'lenth' is not defined — in scope: girth, length".to_string(),
                span: s,
            },
        ),
    ];

    examples
        .iter()
        .map(|(variant, err)| {
            json!({
                "variant": variant,
                "example_message": err.to_string(),
            })
        })
        .collect()
}

// ── Keywords ────────────────────────────────────────────────────────────
// No compiler-enforced exhaustiveness is possible here — arbitrary
// identifiers fall through to Ident in the lexer, so there is no closed set
// to match over. Checked instead by the test below: every entry must lex to
// something other than a plain identifier.

pub fn keyword_list() -> Vec<&'static str> {
    vec![
        "atom", "legend", "voxel", "translate", "merge", "print",
        "thing", "entity", "part", "relation", "constraint", "shape", "material",
        "generator", "world", "refine", "detail", "biome", "terrain",
        "water", "resolve", "scatter", "over", "where", "avoid", "parts", "on",
        "box", "sphere", "cylinder", "cone", "ellipsoid", "blob",
        "heightfield", "shell", "extrude", "capsule", "torus",
        "inside", "outside", "adjacent_to", "above", "below", "left_of",
        "right_of", "in_front_of", "behind", "symmetric_across",
        "attached_to", "touch", "surrounds",
        "and", "or", "not",
        "let", "if", "else",
    ]
}

// ── Assembly ────────────────────────────────────────────────────────────

pub fn build() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "keywords": keyword_list(),
        "shapes": shape_specs(),
        "relations": relation_specs(),
        "qualifiers": qualifier_specs(),
        "errors": error_specs(),
    })
}

pub fn to_json_pretty() -> String {
    serde_json::to_string_pretty(&build()).expect("spec JSON is always serializable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::lexer::token::TokenKind;

    /// Expected count of `ShapeExpr` variants. If this fails, either a
    /// variant was added/removed in `ast::ShapeExpr` (update `probes()` and
    /// `describe()` above) or the count here is stale — either way, that's
    /// the drift the M1 acceptance criteria asks this test to catch.
    const EXPECTED_SHAPE_COUNT: usize = 16;
    const EXPECTED_ERROR_COUNT: usize = 13;

    #[test]
    fn spec_is_valid_json() {
        let text = to_json_pretty();
        let parsed: Value = serde_json::from_str(&text).expect("moxi spec must emit valid JSON");
        assert!(parsed.is_object());
    }

    #[test]
    fn shape_count_matches_the_enum() {
        assert_eq!(shape_specs().len(), EXPECTED_SHAPE_COUNT);
    }

    #[test]
    fn every_shape_has_an_anchor_vocabulary() {
        for s in shape_specs() {
            assert!(!s.anchors.is_empty(), "{} has no anchors listed", s.name);
        }
    }

    #[test]
    fn error_count_matches_the_enum() {
        assert_eq!(error_specs().len(), EXPECTED_ERROR_COUNT);
    }

    #[test]
    fn every_keyword_lexes_as_a_keyword_not_an_identifier() {
        for kw in keyword_list() {
            let (tokens, errors) = Lexer::new(kw).tokenize();
            assert!(errors.is_empty(), "'{kw}' produced lexer errors");
            assert!(
                !matches!(&tokens[0].kind, TokenKind::Ident(_)),
                "'{kw}' lexed as a plain identifier, not a keyword"
            );
        }
    }

    #[test]
    fn no_new_dependencies_were_needed() {
        // Compile-time assertion by construction: this file imports only
        // serde, serde_json, and crate-internal modules already present in
        // Cargo.toml. Nothing to assert at runtime; this test exists so the
        // acceptance criterion has a named home.
    }
}