#!/usr/bin/env bash
# Create one tracking issue per phase, auto-linking the children by number.
# Run from the repo root AFTER create_issues.sh --apply.
#
#   bash create_tracking_issues.sh            # dry run
#   bash create_tracking_issues.sh --apply

set -euo pipefail
APPLY=0; [[ "${1:-}" == "--apply" ]] && APPLY=1

run() { if [[ $APPLY -eq 1 ]]; then "$@"; else printf '  would run: %s\n\n' "$*"; fi; }

# Build a markdown checklist of issues carrying a given label.
checklist() {
  gh issue list --label "$1" --state open --limit 50 \
      --json number,title --jq 'sort_by(.number)[] | "- [ ] #\(.number) \(.title)"' \
    | grep -v 'Tracking:' || true
}

mktracking() {
  local label="$1" title="$2" milestone="$3" body="$4"
  local items; items="$(checklist "$label")"
  if [[ -z "$items" ]]; then
    echo "  no open issues labelled $label — skipping"; return
  fi
  run gh issue create \
    --title "$title" \
    --milestone "$milestone" \
    --label "$label" --label enhancement \
    --body "$body

## Children

$items

---

Ordering and rationale: [\`ROADMAP.md\`](../blob/main/ROADMAP.md).
Contributing rules: [\`CONTRIBUTING.md\`](../blob/main/CONTRIBUTING.md)."
}

mktracking "phase:M" \
  "Tracking: Phase M — Instrumentation" \
  "Phase M — Instrumentation" \
  "**Goal:** make language quality measurable before changing the language.

Moxi's primary consumer is a language model. Right now there is no way to tell
whether a change makes the language easier or harder to write — every decision
is taste. Phase M builds the instrument: the grammar surface emitted from the
compiler, documentation generated from it, a prompt corpus with property
assertions, and an eval loop reporting first-try compile rate and mean repair
iterations.

Those two numbers become the fitness function for every later phase.

**Exit criteria:** baseline numbers recorded and committed to \`bench/results/\`.
They are the control group for everything that follows."

mktracking "phase:S" \
  "Tracking: Phase S — Surface" \
  "Phase S — Surface" \
  "**Goal:** clean Markdown rendering; grammar decoupled from presentation.

Scripts are Markdown, but today the lexer forces every design note to be a
blockquote and indentation fights Markdown's own indented-code-block rule.
Moving to fenced \`\`\`moxi blocks fixes rendering, frees prose to be prose, and
gives editors a language tag — without touching anything inside the fences.

Also records two decisions so they stop being relitigated: braces stay, and
Markdown structure does not become the grammar.

**Exit criteria:** all \`scripts/*.md\` migrated and rendering cleanly on GitHub;
bench numbers no worse than the Phase M baseline."

mktracking "phase:P" \
  "Tracking: Phase P — Placement" \
  "Phase P — Placement" \
  "**Goal:** fix the cyclops problem.

The mate formula has exactly one positional degree of freedom — \`gap\`, along
the socket normal. A part can be pushed away from a socket but never slid
sideways along it, so every attachment lands dead-center on a face. That is why
you cannot put two eyes on a head.

Phase P adds a tangent-plane \`shift\`, gives every shape a \`surface(u, v)\`,
decouples socket position from normal, and thins the relation keywords that are
aliases pretending to be distinct concepts.

**Exit criteria:** the \`character\` bench category passes — a face with two
distinct, symmetric eyes compiles from a natural-language prompt."

echo
[[ $APPLY -eq 0 ]] && echo "Dry run complete. Re-run with --apply." \
                   || echo "Done. Now pin the three tracking issues in the GitHub UI."
