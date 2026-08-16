# Moxi — Roadmap (post-Phase C)

**Status of the tree:** Phases A, A.2, B1, B2, C are shipped. Composition,
CSG, arbitrary rotation, and entity parameters all work. What follows is the
plan to turn Moxi from a sophisticated declarative DSL into a *programming
language for 3D worlds* — without losing the properties that make it
AI-native.

**Naming:** phases M / S / P are mnemonics (Measure, Surface, Placement), not
alphabetical successors. The language-feature ladder resumes the letter
sequence at D.

---

## The one invariant

Every phase below must preserve four properties. If a proposed feature breaks
one, the feature is wrong, not the property.

1. **Total** — compilation always terminates. No unbounded recursion, no
   `while`, no general fixpoint.
2. **Analyzable** — every error is static, spanned, and names the valid
   vocabulary. `UndefinedAnchor { valid: [...] }` is the model to copy.
3. **One language** — `let`, `if`, comprehensions live *inside* entity bodies
   and evaluate at resolve time. No separate scripting block. The moment
   there are two languages stapled together, you have built Blender.
4. **Errors are API** — error message text is the model's only feedback
   channel at inference time. Snapshot-test the strings.

---

## Ordering rationale

The instinct is to start at Phase D (the value language) because it is the
substrate for everything. Resist it for three weeks. **M, S, and P come
first** because:

- **M** builds the instrument. Without it, every later decision is taste, and
  you cannot tell whether a syntax change helped or hurt the model.
- **S** changes the lexer entry point. Doing it before D means the expression
  language is written once, inside fences, rather than retrofitted.
- **P** is a ~20-line solver change that unblocks an entire class of models
  (faces, panels, studded surfaces) you currently cannot express. It is the
  highest capability-per-line item on the whole list, and it needs to land
  before the bench corpus is written, or the bench will encode the current
  limitation as normal.

None of M, S, or P touch `frame.rs`, `geometry/mod.rs`, or the solver's
topological sort. They are cheap and independent.

---

# Phase M — Instrumentation

**Goal:** make language quality measurable before changing the language.

### M1 — `moxi spec --json`

Emit the entire grammar surface *from the compiler*, not from prose:

- keyword list (from the lexer's `keyword_or_ident` table)
- every `ShapeExpr` variant with its args, defaults, and local origin
  convention
- `valid_anchor_names(shape)` for each shape — this function already exists
- the relation sugar table (from `desugar_placement`)
- the error catalogue with example messages
- version string

### M2 — Generated SKILL.md

`moxi skill > SKILL.md`, rendered from the M1 JSON plus a hand-written prose
preamble. Run it in CI; fail the build if the committed file is stale.

*Rationale:* hand-written docs for a moving DSL always drift, and drift is
invisible — the model keeps writing valid-two-versions-ago Moxi and you blame
the model. PHASE_C.md already lists this as a deferred item; it is promoted
here to a gate on every subsequent phase.

### M3 — `bench/`

Each case is a natural-language prompt plus **property assertions**, never
golden voxel hashes (B1 already proved output shifts legitimately):

```yaml
id: char-001
prompt: "a simple humanoid face with two eyes, a nose and a mouth"
assert:
  compiles: true
  layers: 1
  bbox_y: [8, 30]
  bilateral_symmetry: 0.02   # left/right voxel counts within 2%
  distinct_colors: ">=3"
  parts_min: 5
```

Categories, chosen to find holes rather than confirm strengths:
`organic` · `mechanical` · `architectural` · `terrain` · `character` ·
`abstract`. Fifteen cases is enough to start; the character category will
fail immediately, which is the point.

### M4 — `moxi eval`

Runs a model against the current SKILL.md and reports two numbers:

- **first-try compile rate**
- **mean repair iterations to success** (feeding `compile_to_json` errors back)

These two numbers are the fitness function. Every future syntax proposal gets
judged against them instead of against taste.

**Exit criteria:** baseline numbers recorded for the current language. Commit
them. They are the control group for everything that follows.

**On version control:** put `bench/` in the public repo. It is the artifact
that makes the AI-native claim falsifiable rather than marketing, and it is
the only way a change proposed by a model can be validated by someone who
isn't you. A private validation workflow means the language's quality is
unauditable with a bus factor of one.

---

# Phase S — Surface

**Goal:** clean Markdown rendering; grammar decoupled from presentation.

### S1 — Fenced code blocks

The lexer compiles what is inside ` ```moxi ` fences and ignores everything
else. Smaller than the current `#` / `>` comment handling in
`skip_whitespace_and_comments`.

What this buys:

- previews render properly — code in monospace boxes, prose as prose
- indentation stops fighting Markdown's own 4-space indented-code-block rule,
  which is what actually makes current previews messy
- **prose stops needing to be a blockquote** — every `>` design note in
  `ISLAND.md` becomes normal paragraphs, headings, tables, images
- editor and GitHub syntax highlighting via the language tag
- it is the literate-programming convention every model has seen millions of
  times (Rmd, Quarto, notebooks)

Keep a compatibility flag for one release so existing scripts still compile.

### S2 — Keep braces

Decision, recorded so it stops being reopened: **braces stay; no
whitespace-significant syntax.** Markdown already assigns meaning to leading
spaces, so any renderer, copy-paste, or model reflow can silently change
program structure. Braces are self-delimiting, which is why
`skip_to_close_brace` can recover from a parse error instead of cascading —
and that recovery directly improves repair-loop iterations. As D adds `let`,
`if`, and comprehensions, brace-delimited blocks nest without ambiguity.

Also rejected: making Markdown structure *be* the program (`##` headings as
declarations, bullets as properties). It previews beautifully, caps nesting at
six levels, makes the grammar hostage to a renderer, and will fight the
expression language. If the pretty-document form is wanted later, generate it
as a *view* of the AST.

### S3 — `:` as a synonym for `=` in property position

One match arm in the parser. Models write `shape: sphere(radius=4)` by reflex
from YAML/JSON/TS. **Do not merge on intuition — merge if M4 shows a
first-try compile-rate gain.** This is the first real use of the bench, and a
good rehearsal of the workflow.

**Exit criteria:** all `scripts/*.md` migrated to fences and rendering
cleanly on GitHub; bench numbers not worse than the M baseline.

---

# Phase P — Placement

**Goal:** fix the cyclops problem. Placement gains the degrees of freedom it
is missing.

### P1 — `shift` (the fix)

**Diagnosis:** the mate formula has exactly one positional degree of freedom,
`gap`, along the socket normal. A part can be pushed away from a socket but
never slid sideways. Every attachment lands dead-center on a face — hence one
eye.

```moxi
LeftEye.back on Head.front shift=(-2.5, 1.0)
RightEye symmetric_across Head from=LeftEye
```

`shift` is a 2-vector in the socket's **tangent plane** — its local X and Z,
which `frame_from_normal` already defines. Implementation: one more
`Frame::from_pos` in the `adjust` chain in `solve_one`, beside `gap`. ~20
lines. It turns every anchor in the language from a point into a patch: eyes,
buttons, windows, rivets, freckles.

### P2 — Universal `surface(u, v)`

Give every shape a surface parameterization, not just sphere / ellipsoid /
cylinder / cone / heightfield. Box gets face + uv; CSG inherits from its base
operand (the machinery already exists in `resolve_anchor`).

### P3 — Decouple position from normal

`surface(u, v, aim=out|up|axis)`. SKELETON_v3's shoulder bug was exactly this
coupling — `surface(pitch=-55)` moved the socket 55° toward the bottom pole
because position and normal are welded together. Make the aim explicit and
optional.

### P4 — Thin the relation vocabulary

`touch`, `adjacent_to`, and `attached_to` all desugar to `bottom`/`top`. That
is arguably a lie in each case, and a model reading the vocabulary will assume
they differ and pick wrong. Six honest keywords beat thirteen where seven are
aliases or fictions. Deprecate with a warning that names the survivor.

### The conceptual split worth recording

Moxi has two placement modes fused into one vocabulary:

- **Layout** — separated objects arranged relative to each other. `above`,
  `left_of`, grove spacing, terrain layers. Compass anchors on assembly
  boxes. A.2 nailed this; it works.
- **Attachment** — features on a host surface. Eyes on a head, handle on a
  mug, branch on a trunk. Wants continuous surface coordinates, not face
  centers.

P2 + P3 give attachment its own proper idiom. "A face" becomes one
`surface()` call per feature, plus `shift`, plus mirror.

**Exit criteria:** the `character` bench category passes. A face with two
distinct eyes compiles from a natural-language prompt.

---

# Phase D — Values and bindings

**Goal:** a real evaluated value language. Pure front-end; no geometry
changes.

- Promote `Expr` to a proper value domain: float, vector, bool, string, list.
- `let` bindings inside entity bodies.
- `if / else` as an *expression*.
- Constant-fold everything at resolve time (the Phase C `eval_const` /
  `subst_expr` machinery generalizes directly).
- Retire `generator.rs`'s private `eval_bool` / `eval_f64` interpreter — you
  already wrote a tiny expression evaluator there; this is the language
  admitting it.

**Why now and not first:** it is invisible to users and unlocks E through I.
Ship it quietly and the bench should barely move — which is the success
condition.

---

# Phase E — Repetition

**Goal:** the phase that makes people say "it's a real language."

- Comprehensions over parts and relations:
  `part Rib[i in 0..12] { shape = ..., material = Bone }`, with `i` usable in
  shape args and anchor args.
- Array combinators: linear, radial, grid. A radial array is columns, spokes,
  gear teeth, ribs, fence posts, clock faces.

`Ribcage` is currently `shell(ellipsoid)` not because that is the right model
but because you cannot write a loop. This phase fixes that.

Loop bounds must be compile-time constants — the totality invariant.

---

# Phase F — Higher-order entities

**Goal:** "functions of functions," properly.

- **Entity-typed parameters:** `entity Tree(canopy = Blob)` — pass an entity
  as an argument, not just a float. This is the actual difference between a
  macro and a function, and it is the thing currently missing.
- **Bounded self-recursion:** self-instancing where the depth argument is a
  compile-time constant that must strictly decrease.

```moxi
entity Branch(depth=4, length=8) {
    part Stem { shape = cylinder(height=length, radius=length*0.08) }
    when depth > 0 {
        part Left  { entity = Branch(depth=depth-1, length=length*0.7) }
        part Right { entity = Branch(depth=depth-1, length=length*0.7) }
    }
}
```

An L-system in nine lines. Always terminates, always analyzable — the
strongest evidence that bounded beats unbounded for this domain.

- **C.2 pass-through:** parameters flowing into nested instances
  (`entity = Inner(w=length)`), currently a clear error. F is where it lands.
- Parameters in `twist` / `pitch` / `gap` / `shift` qualifiers.

---

# Phase G — Modules and stdlib

- `use PalmTree from "flora.md"`, namespacing, cycle detection.
- **A standard library** — `std/flora`, `std/arch` (Arch, Column, Stair,
  Truss), `std/mech` (Gear, Bearing, Axle). Cheap relative to payoff, and it
  is what a model will actually lean on rather than reinventing a tree from
  primitives every time.

This is what makes "a full 3D program from a collection of scripts" true.

---

# Phase H — Traits / kinds

`entity Oak is Tree`. A scatter then distributes *anything that is a Tree*,
and the resolver resolves kinds rather than names. The genuinely AI-native
feature: the model writes intent, the compiler resolves specifics.

Depends on G — traits without modules is a vocabulary with nothing to range
over.

---

# Phase I — Worlds as values

Retire `print`-as-global-ordered-mutation. A world becomes a composable value
with explicit layering, so worlds can be imported, nested, and diffed.

`print` order is currently the least composable construct in the language and
it will fight modularity from the moment G lands. Also: `generator` collapses
into a library-level `scatter` returning a list of frames that an entity is
mapped over — a special-cased language feature deleted in exchange for a
general one. And `WorldDecl`, parsed since v0.2 and never compiled, finally
means something.

---

## Sequencing summary

| Phase | Name | Touches solver? | Gate |
|---|---|---|---|
| M | Instrumentation | no | baseline recorded |
| S | Surface | lexer only | previews clean, bench flat |
| P | Placement | `solve_one`, `anchors.rs` | character category passes |
| D | Values | resolver only | bench flat |
| E | Repetition | resolver only | mechanical category improves |
| F | Higher-order | resolver only | L-system tree compiles |
| G | Modules + stdlib | new front-end pass | all categories improve |
| H | Traits | resolver | intent-level prompts succeed |
| I | Worlds as values | pipeline | `generator` deleted |

**Immediate next four items, in order:** `moxi spec --json` → generated
SKILL.md → `bench/` with ~15 cases → `shift=`. None touch the solver's core;
afterward you have a measurement instrument pointed at the language *before*
you start reshaping it.

---

## Explicitly rejected

Recorded so they stop consuming cycles:

- **Turing completeness.** General recursion buys the halting problem, and
  with it: no static diagnostics inside loops, unbounded compile times in the
  WASM sandbox, and model-generated infinite loops hanging the browser tab.
  Blender's Python is Turing complete and nobody thinks that is the good part
  of Blender. Total beats general here.
- **Shortening `entity` to `ENT` / `ET`.** The primary consumer is an LLM and
  the primary asset is low ambiguity. Tokens are cheap; unambiguous keywords
  are not.
- **Splitting `entity` into `struct` vs `class` vs `entity`.** Two nouns for
  one concept taxes every future feature. If a second declaration form is
  wanted, make it orthogonal — `fn` for pure value functions (math, layout,
  position lists) vs `entity` for geometry-producing ones. That split earns
  its keep; class-vs-struct does not.
- **Whitespace-significant syntax.** See S2.
- **Markdown-structure-as-grammar.** See S2.