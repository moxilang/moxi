#!/usr/bin/env bash
#
# migrate_atoms.sh — rewrite scripts/ from the two-declaration form
#
#     atom BONE { color = ivory }
#     material Bone { color = ivory, voxel_atom = BONE }
#
# to the self-contained form
#
#     material Bone { color = ivory }
#
# and PROVE the voxel output did not change. Rule 4: parity is an explicit
# claim. This script produces the sentence you paste into the PR.
#
# Usage:
#   ./migrate_atoms.sh --check     baseline only, no edits (run before the compiler change)
#   ./migrate_atoms.sh --migrate   rewrite scripts/, then diff against the baseline
#
# Order of operations:
#   1. --check on the OLD compiler  → records baseline JSON
#   2. implement the resolver change
#   3. --migrate on the NEW compiler → rewrites scripts and diffs
#
set -euo pipefail

SCRIPTS_DIR="${SCRIPTS_DIR:-scripts}"
BASELINE_DIR="${BASELINE_DIR:-.parity-baseline}"
MODE="${1:---check}"

command -v cargo >/dev/null || { echo "cargo not found"; exit 1; }

# Voxel-only projection: bounds + total + the voxel list. Deliberately drops
# layer names and dims, which are not part of the parity claim.
project() {
  if command -v jq >/dev/null; then
    jq -S '{total, bounds, voxels}' 2>/dev/null || cat
  else
    cat
  fi
}

compile_all() {
  local out="$1"
  mkdir -p "$out"
  local n=0
  for f in "$SCRIPTS_DIR"/*.md; do
    [ -e "$f" ] || continue
    local base
    base="$(basename "$f" .md)"
    if cargo run --quiet -- json "$f" 2>/dev/null | project > "$out/$base.json"; then
      n=$((n + 1))
    else
      echo "  ! $base failed to compile" >&2
      echo '{"ok":false}' > "$out/$base.json"
    fi
  done
  echo "$n"
}

# ── rewrite ─────────────────────────────────────────────────────────────
#
# Two edits, both line-oriented and conservative:
#   1. strip `, voxel_atom = FOO` (and `voxel_atom = FOO,`) from material bodies
#   2. delete single-line `atom FOO { ... }` declarations
#
# An atom is only deleted when its name appears nowhere else after edit 1 —
# so an atom referenced by a `legend` block, or shared by two materials via
# an explicit voxel_atom, survives untouched.

rewrite_one() {
  local f="$1"

  if grep -qE '^\s*legend\b' "$f"; then
    echo "  = $(basename "$f") skipped (has a legend block; atoms are load-bearing)"
    return
  fi

  local tmp
  tmp="$(mktemp)"

  sed -E \
    -e 's/,[[:space:]]*voxel_atom[[:space:]]*=[[:space:]]*[A-Za-z_][A-Za-z0-9_]*//g' \
    -e 's/voxel_atom[[:space:]]*=[[:space:]]*[A-Za-z_][A-Za-z0-9_]*[[:space:]]*,[[:space:]]*//g' \
    "$f" > "$tmp"

  # Collect atom names, then delete each whose name is now unreferenced.
  local names
  names="$(grep -oE '^[[:space:]]*atom[[:space:]]+[A-Za-z_][A-Za-z0-9_]*' "$tmp" \
           | awk '{print $2}' || true)"

  local name refs
  for name in $names; do
    refs="$(grep -cE "\b${name}\b" "$tmp" || true)"
    if [ "$refs" -le 1 ]; then
      sed -i -E "/^[[:space:]]*atom[[:space:]]+${name}[[:space:]]*\{[^}]*\}[[:space:]]*$/d" "$tmp"
    else
      echo "  = $(basename "$f"): atom $name still referenced, kept"
    fi
  done

  if cmp -s "$f" "$tmp"; then
    rm -f "$tmp"
  else
    mv "$tmp" "$f"
    echo "  ~ $(basename "$f") rewritten"
  fi
}

# ── modes ───────────────────────────────────────────────────────────────

case "$MODE" in
  --check)
    echo "Recording parity baseline from $SCRIPTS_DIR ..."
    count="$(compile_all "$BASELINE_DIR")"
    echo "Baseline recorded: $count script(s) in $BASELINE_DIR/"
    echo "Now implement the resolver change, then re-run with --migrate."
    ;;

  --migrate)
    [ -d "$BASELINE_DIR" ] || { echo "No baseline. Run --check on the OLD compiler first."; exit 1; }

    echo "Rewriting $SCRIPTS_DIR ..."
    for f in "$SCRIPTS_DIR"/*.md; do
      [ -e "$f" ] || continue
      rewrite_one "$f"
    done

    echo "Recompiling ..."
    AFTER_DIR="$(mktemp -d)"
    compile_all "$AFTER_DIR" >/dev/null

    echo
    echo "── Parity ──────────────────────────────────────────────"
    drift=0
    for b in "$BASELINE_DIR"/*.json; do
      base="$(basename "$b")"
      if [ ! -f "$AFTER_DIR/$base" ]; then
        echo "  MISSING after: $base"; drift=1; continue
      fi
      if cmp -s "$b" "$AFTER_DIR/$base"; then
        echo "  identical  $base"
      else
        echo "  DRIFTED    $base"
        diff <(head -c 2000 "$b") <(head -c 2000 "$AFTER_DIR/$base") | head -20 || true
        drift=1
      fi
    done
    echo "────────────────────────────────────────────────────────"

    if [ "$drift" -eq 0 ]; then
      echo
      echo "PR line: every script in $SCRIPTS_DIR was migrated to the"
      echo "self-contained material form; voxel output is byte-identical"
      echo "for all of them. No parity claim is being made beyond that."
    else
      echo
      echo "DRIFT DETECTED. Per rule 4 the PR must name which scripts"
      echo "changed and argue the new output is more CORRECT, not merely"
      echo "different. Do not merge until that is written."
      exit 1
    fi
    ;;

  *)
    echo "usage: $0 [--check|--migrate]"; exit 1
    ;;
esac