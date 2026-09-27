# Moxi — Roadmap

**Status, September 2026.** The language core is done: composition, CSG,
parameters, values, loops, functions. So is a render stack the original
roadmap never planned: a canonical scene IR, a signed-distance geometry
kernel, a mesher, a GLSL raymarcher, and two viewers.

What remains to **v1** is four items. Everything after v1 is growth. This
document separates the two on purpose. An ambition like "state-of-the-art 3D
from language" has no finish line; v1 does.

Moxi's primary consumer is a language model. That drives every rule below.

---

## The invariants

Every phase must preserve these. If a feature breaks one, the feature is
wrong, not the invariant.

1. **Total.** Compilation always terminates. No `while`, no unbounded
   recursion, no general fixpoint. Loop bounds and recursion depths are
   compile-time constants.
2. **Analyzable.** Every error is static, spanned, and names the valid
   vocabulary. `UndefinedAnchor { valid: [...] }` is the model to copy.
3. **One language.** `let`, `if`, `for` and `fn` live inside the language
   and evaluate at resolve time. No second scripting language stapled on.
4. **Errors are API.** Error text is the model's only feedback channel at
   inference time. Rewording one is a reviewable change.

And one convention, fixed so it stops causing bugs:

5. **Orientation is glTF's.** +Y up, **+Z is the front** of every thing
   (`north` is the face), +X is right as seen from the front. Every relation
   keyword, both viewers and every export agree on it.

---

## Where things stand

### Language

| Phase | | Status |
|---|---|---|
| **M** Instrumentation | M1 `moxi spec --json` | ✅ |
| | M2 generated SKILL.md, CI-checked | ✅ |
| | M3 `bench/` corpus + `moxi bench` runner | ✅ runner and all assertion types; 12 of 17 cases have reference solutions, the other 5 are gated on unshipped phases |
| | M4 `moxi eval` | ❌ **v1** |
| **S** Surface | S1 fenced ` ```moxi ` blocks | ✅ |
| | S2 braces stay (decision) | ✅ |
| | S3 `:` as a synonym for `=` | ⏸ merges only if M4 shows a gain |
| **P** Placement | P1 `shift=(a, b)` | ✅ |
| | P2 universal `surface(u, v)` | ❌ **v1** — only sphere-likes, heightfield and torus have it |
| | P3 decouple position from normal | ❌ — superseded by aim-at-target, see Growth |
| | P4 thin the relation vocabulary | ❌ |
| **D** Values | `let`, `if` as an expression, one evaluator | ✅ |
| | math builtins (degrees), qualifier expressions | ✅ |
| | D.2 lists | ❌ **v1** |
| | vectors, strings | ❌ |
| **E** Repetition | `for i in a..b { … }` over parts, relations, lets, nested loops | ✅ |
| | indexed parts `Rib[i]`, index arithmetic `V[k-1]`, grids `C[i][j]` | ✅ |
| | `fn name(args) = expr` — pure, total, no recursion | ✅ |
| | computed instance arguments; structural parameters re-resolve per override | ✅ |
| **F** Higher-order | parameter pass-through into nested instances | ✅ |
| | entity-typed parameters | ❌ |
| | bounded self-recursion | ❌ |
| **G** Modules + stdlib | | ❌ |
| **H** Traits | | ❌ |
| **I** Worlds as values | "a scene is a thing" — placement by relation, no `world` block | ✅ |
| | retire global `print`; delete `generator` | ❌ |

The plan was a comprehension syntax (`part Rib[i in 0..12]`) plus array
combinators. What shipped is `for` blocks with indexed names. That is more
general, and it made the combinators unnecessary.

### Render stack (not in the original plan)

| Component | Status |
|---|---|
| Scene IR (`scene.rs`) — one canonical artifact every backend reads | ✅ |
| Signed-distance kernel (`geometry::distance`), smooth blend, gradients | ✅ |
| Surface-nets mesher, `moxi mesh` → OBJ | ✅ |
| GLSL raymarcher, `moxi web` → self-contained HTML | ✅ |
| Bevy viewer drawing the scene IR, primitives + meshed fallback | ✅ |
| Sculptor primitives: `capsule`, `torus`, `box(round=)` | ✅ |
| Local transforms: `at`, `spin`, `mirror`, `scale` | ✅ |
| Geometric mirroring — `symmetric_across` reflects shape, not only pose | ✅ |
| `lathe`, `sweep` | ❌ **v1** — waits on lists |

**Scope freeze.** Until v1, no new backends, viewers or export formats. The
render stack is complete enough to show what the language can do. Its job
now is to stay correct, not to grow.

---

## v1 — the finish line

Four items, in order. When all four are done, Moxi is v1.

### 1. D.2 — lists

A list value type folded at resolve time, like every other value. It is
needed first by sweep's path and lathe's profile. Open questions, to settle
before writing code:

- element types: numbers only, or points (which needs vectors)?
- whether `[for t in a..b: expr]` comprehensions belong here or in E.2
- how lists pass through `let` substitution and instance arguments

This is the one item on the v1 list that is a genuine design problem rather
than implementation. Route it to the strongest model available.

### 2. `sweep` and `lathe`

- `lathe(profile=[…])` revolves a 2D profile. Vases, columns, bottles.
- `sweep(radius=, path=[…])` runs a circle along a 3D path. Ribs, horns,
  tails, pipes, clavicles, a spine's S-curve as one shape.

With `for` and `fn`, a path can be computed: each rib's curve is a formula
in `i`. That is the difference between the ribcage we have (flat hoops) and
an anatomical one (curves that descend as they run forward).

Design points: the path representation (polyline vs. spline), frames along
the curve (rotation-minimizing, so a cross-section does not roll), and a
distance function for a tube around a curve.

### 3. P2 — universal `surface(u, v)`

Every shape gets a surface parameterization, not only the sphere-likes.
Boxes get face + (u, v). CSG and the local transforms inherit it through
their base, as the other anchors already do. This is what attachment needs:
features placed continuously on any host, not at face centres.

### 4. M4 — `moxi eval`, and a recorded baseline

Run a model against the current SKILL.md. Report **first-try compile rate**
and **mean repair iterations**, feeding `compile_to_json` errors back.
Commit the numbers.

This is the scoreboard. From here on, a syntax proposal (S3, P4, anything)
is judged against these two numbers instead of taste.

---

## v1 hygiene — small debts to clear along the way

None of these is a phase. Each is a morning.

- **Bench, E-gated cases.** Write reference solutions for `mech-003`
  (gear), `arch-003` (colonnade) and `abstract-001` (spiral stair), then add
  `"E"` to `LANDED_PHASES`. Audit the rest of `LANDED_PHASES` against what
  shipped.
- **Orientation audit.** Every script and bench case using `in_front_of` or
  `behind` was written against the old, inverted table. Rewrite the ones that
  relied on it.
- **Constraints inside instances.** `constraint RightLeg.Foot below Torso`
  does not parse. Copy `parse_partial_anchor_ref`.
- **Part naming vs. orientation.** A character's own right hand is at −X
  (`west`). Scripts that put `RightArm` on `Torso.east` have it backwards.
- **Geometry oracles.** Every serious geometry bug so far compiled clean and
  was caught by a human looking at a screenshot. Add property tests on the
  solved scene for the classes that recur — "the face is on +Z", "mirrored
  limbs are mirror images", "nothing floats" — alongside the two that exist
  (`relation_keywords_agree_on_one_orientation`,
  `symmetric_across_mirrors_geometry_not_just_placement`).

---

## Growth — after v1

Unordered until M4 exists to order them.

- **Aim-at-target placement.** `Arm.socket on Torso.side(…) aim_at=Hand`
  replaces two coupled rotations with one target point. Posing is the most
  expensive recurring failure in practice: three separate scripts got the
  `pitch`/`twist` convention on `side` anchors backwards, and a rib cannot be
  tilted without rolling it. This supersedes P3.
- **G — modules and a standard library.** `use PalmTree from "flora.md"`,
  then `std/anatomy`, `std/arch`, `std/mech`. This is what makes detail
  cheap: a model composes a `Hand` instead of reinventing one.
- **F — entity-typed parameters and bounded recursion.** An L-system tree
  in nine lines, still total.
- **The visual feedback loop.** The model sees the raymarched render of what
  it wrote and iterates. This is where "compiles" becomes "looks right".
- **H — traits.** `thing Oak is Tree`; scatter anything that is a Tree.
- **I, finished.** Retire global `print`; `generator` becomes a library
  `scatter` over a list of frames.
- **Conditional items inside loops.** `if i > 0 { relation … }` inside `for`.
  Today's workaround — first element outside, loop from 1 — is documented.
- **Chained mirrors.** Mirroring a mirror image is a clear error today. The
  composition is a rotation, not a reflection, and needs its own care.

---

## Decisions already made

Recorded so they stop consuming cycles.

- **Braces stay.** No whitespace-significant syntax: renderers and model
  reflow silently change leading spaces, and brace recovery powers the repair
  loop.
- **Markdown structure is never the grammar.** A pretty-document form, if
  wanted, is a view generated from the AST.
- **No Turing completeness.** General recursion buys the halting problem:
  no static diagnostics inside loops, unbounded compile times in the WASM
  sandbox, and model-generated infinite loops hanging a browser tab.
- **`thing` is the one declaration form.** `entity` lexes as a synonym for
  one release, then goes. No `struct`/`class` split. `fn` is the orthogonal
  second form: pure values, no geometry.
- **No abbreviated keywords.** Tokens are cheap; ambiguity is not.
- **The scene IR is the canonical artifact.** Voxels are one backend among
  several, not the model.
- **Lathe and sweep wait for lists.** Building them around the gap would
  smuggle a list type in through one feature instead of designing it.
- **A scene is a thing.** No `world` block; worlds are composed by the same
  placement as everything else.
- **Orientation is glTF's** (see invariant 5).

---

## Working notes

- `NOTES.md` tracks what actually shipped and what is currently wrong, at a
  finer grain than this file. Read it before this one.
- `SKILL.md` is generated: edit `docs/skill_preamble.md`, then
  `cargo run -- skill > SKILL.md`. CI fails on a stale file.
- Diffs to `frame.rs`, `frame_resolver.rs`, `geometry/mod.rs` or `anchors.rs`
  need human review of the math. Their failure mode is silently wrong
  geometry, not a compile error.
- A new shape touches about twelve files across the parser, resolver, anchors,
  geometry, scene IR, shader, spec, viewer and docs. `NOTES.md` lists them.
  That cost is the price of several backends that must agree; it is why the
  render stack is frozen until v1.
