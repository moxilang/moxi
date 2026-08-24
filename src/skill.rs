//! `moxi skill` — render SKILL.md from `docs/skill_preamble.md` (hand-written)
//! plus a Generated Reference appendix built from `src/spec.rs`.
//!
//! Once this lands, `SKILL.md` is generated output — edit the preamble or
//! `spec.rs`, never `SKILL.md` directly. `moxi skill --check` is what CI
//! runs to fail the build on a stale committed copy. See CONTRIBUTING.md
//! rule 3 and ROADMAP.md, Phase M.

use crate::spec::{self, ArgSpec};
use std::fmt::Write as _;

const TOP_NOTE: &str = "\
> **Generated file.** Everything below the first `---` is hand-written prose \
from `docs/skill_preamble.md`. Everything after the second `---` — the \
Generated Reference — is rendered by `moxi skill` from the compiler's own \
tables in `src/spec.rs`. Do not hand-edit the reference section; run \
`moxi skill > SKILL.md` after a language change, and `moxi skill --check` \
to verify it's current (CI does this).\n";

/// Deterministic: same compiler state, same bytes, every time. All ordering
/// comes from `spec.rs`'s own tables (BTreeMap-backed via serde_json), never
/// from HashMap iteration.
pub fn render(preamble: &str) -> String {
    let mut out = String::new();

    out.push_str(TOP_NOTE);
    out.push('\n');
    out.push_str(preamble.trim_end());
    out.push_str("\n\n---\n\n");

    let _ = writeln!(out, "# Generated Reference\n");
    let _ = writeln!(
        out,
        "Emitted by `moxi skill` from `src/spec.rs` — version `{}`.\n",
        env!("CARGO_PKG_VERSION")
    );

    render_shapes(&mut out);
    render_relations(&mut out);
    render_qualifiers(&mut out);
    render_errors(&mut out);
    render_keywords(&mut out);

    out
}

fn render_shapes(out: &mut String) {
    let _ = writeln!(out, "## Shapes\n");
    let _ = writeln!(out, "| Shape | Arguments | Local origin |");
    let _ = writeln!(out, "|---|---|---|");
    for s in spec::shape_specs() {
        let args = if s.args.is_empty() {
            "—".to_string()
        } else {
            s.args.iter().map(fmt_arg).collect::<Vec<_>>().join(", ")
        };
        let _ = writeln!(out, "| `{}` | {} | {} |", s.name, args, s.origin);
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "### Anchor vocabulary per shape\n");
    for s in spec::shape_specs() {
        let _ = writeln!(out, "- **{}**: {}", s.name, s.anchors.join(", "));
    }
    let _ = writeln!(out);
}

fn fmt_arg(a: &ArgSpec) -> String {
    match a.default {
        Some(d) => format!("`{}` ({}, default {})", a.name, a.kind, fmt_default(d)),
        None => format!("`{}` ({}, required)", a.name, a.kind),
    }
}

fn fmt_default(d: f64) -> String {
    if d.fract() == 0.0 {
        format!("{}", d as i64)
    } else {
        format!("{d}")
    }
}

fn render_relations(out: &mut String) {
    let rel = spec::relation_specs();
    let _ = writeln!(out, "## Relation keyword sugar\n");
    let _ = writeln!(out, "| Keyword | Subject anchor | Object anchor |");
    let _ = writeln!(out, "|---|---|---|");
    for entry in rel["sugar"].as_array().expect("sugar is an array") {
        let _ = writeln!(
            out,
            "| `{}` | `{}` | `{}` |",
            entry["keyword"].as_str().unwrap_or("?"),
            entry["subject_anchor"].as_str().unwrap_or("?"),
            entry["object_anchor"].as_str().unwrap_or("?"),
        );
    }
    let _ = writeln!(out);

    let sym = &rel["symmetric_across"];
    let _ = writeln!(out, "**`symmetric_across`**:\n");
    let _ = writeln!(out, "```\n{}\n```\n", sym["form"].as_str().unwrap_or(""));
    let note = sym["note"].as_str().unwrap_or("");
    let capitalized = note
        .get(..1)
        .map(|c| c.to_uppercase())
        .unwrap_or_default()
        + &note[1..];
    let _ = writeln!(out, "{capitalized}.\n");
}

fn render_qualifiers(out: &mut String) {
    let q = spec::qualifier_specs();
    let _ = writeln!(out, "## Qualifiers\n");
    let mut keys: Vec<&String> = q.as_object().expect("qualifiers is an object").keys().collect();
    keys.sort();
    for key in keys {
        let entry = &q[key];
        let _ = write!(out, "- **`{key}`**");
        if let Some(unit) = entry["unit"].as_str() {
            let _ = write!(out, " — unit: {unit}");
        }
        if let Some(applies) = entry["applies_to"].as_str() {
            let _ = write!(out, " — applies to: {applies}");
        } else if let Some(arr) = entry["applies_to"].as_array() {
            let list: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
            let _ = write!(out, " — applies to: {}", list.join(", "));
        }
        if entry["required"].as_bool() == Some(true) {
            let _ = write!(out, " — required");
        }
        if let Some(default) = entry["default"].as_str() {
            let _ = write!(out, " — default: `{default}`");
        }
        if let Some(note) = entry["note"].as_str() {
            let _ = write!(out, " — {note}");
        }
        let _ = writeln!(out);
    }
    let _ = writeln!(out);
}

fn render_errors(out: &mut String) {
    let _ = writeln!(out, "## Error catalogue\n");
    let _ = writeln!(out, "Every error names its stage, and — for anchor and instance errors — the full valid vocabulary. Read the message; the fix is almost always in it.\n");
    for e in spec::error_specs() {
        let _ = writeln!(
            out,
            "- **`{}`**: {}",
            e["variant"].as_str().unwrap_or("?"),
            e["example_message"].as_str().unwrap_or("?"),
        );
    }
    let _ = writeln!(out);
}

fn render_keywords(out: &mut String) {
    let _ = writeln!(out, "## Reserved keywords\n");
    let _ = writeln!(out, "{}\n", spec::keyword_list().join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub_preamble() -> &'static str {
        "# Moxi — SKILL.md\n\nA prompt guide for LLMs generating Moxi scripts."
    }

    #[test]
    fn render_is_deterministic() {
        assert_eq!(render(stub_preamble()), render(stub_preamble()));
    }

    #[test]
    fn every_shape_appears() {
        let out = render(stub_preamble());
        for s in spec::shape_specs() {
            assert!(out.contains(s.name), "missing shape '{}'", s.name);
        }
    }

    #[test]
    fn every_error_variant_appears() {
        let out = render(stub_preamble());
        for e in spec::error_specs() {
            let variant = e["variant"].as_str().unwrap();
            assert!(out.contains(variant), "missing error variant '{variant}'");
        }
    }

    #[test]
    fn top_note_is_first() {
        assert!(render(stub_preamble()).starts_with("> **Generated file.**"));
    }

    #[test]
    fn preamble_survives_untouched() {
        let out = render(stub_preamble());
        assert!(out.contains(stub_preamble()));
    }
}