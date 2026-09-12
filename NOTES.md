# Working notes

Context that isn't obvious from the source. Decisions and their reasons;
known gaps with their diagnoses.

## Where the project is

Shipped beyond ROADMAP.md's plan: the scene IR (`src/scene.rs`) is the
canonical artifact every backend reads; an SDF backend (`geometry::distance`)
feeds a surface-nets mesher and a browser raymarcher. ROADMAP.md predates
all of that and needs a rewrite.

Remaining to a shippable v1: list values (D.2) → `for` blocks (E) →
lathe/sweep → the bench runner (M3/M4). The bench corpus exists at
`bench/cases.yaml`; the runner does not.

## Known gaps, with diagnoses

**Constraints can't see inside instances.** `parse_constraint_stmt` calls
`expect_ident()`, so `constraint RightLeg.Foot below Torso` won't parse,
and the resolver only knows bare names. A composed thing can only
constrain its top level. The placement path already solved this with
`parse_partial_anchor_ref` — copy that.

**Posing is unpredictable from source.** Three coupled rotations per limb;
`pitch` tilts off the socket normal so its sign depends on the socket's
orientation; and on an axis-aligned normal the tangent basis degenerates,
so `twist=0` means an arbitrary quarter turn. Posing MOXIBOI took five
rounds of guess-and-check. An aim-at-target placement form would replace
all three numbers with one point. P3 (`aim=`) is a partial answer.

**`0-1` lexes as `0` then `Int(-1)`.** A leading `-` before a digit folds
into the literal, so binary subtraction needs spaces: `0 - 1`. Changing
this risks `at(box, x=-1.5)` everywhere.

**P2 `surface(u,v)` is blocking character work.** The pec/ab definition in
the Moxi-Boi reference art needs surface-placed cuts.

## Decisions, with reasons

- **A scene is a thing.** No `world` block: a scene is a thing whose parts
  are other things, placed by relation. Reached with zero new syntax.
- **`atom` kept, demoted.** Materials are self-contained; `voxel_atom`
  remains for the genuine case of two materials sharing one atom
  (ISLAND.md's SOIL and TRUNK).
- **`entity` still lexes as a synonym for `thing`** for one release.
- **Lathe and sweep deferred**, not hacked: both need a profile as an
  ordered list, and lists aren't a value type yet. Doing them early would
  smuggle a list type in through one feature.
- **Totality holds.** No general recursion. Phases D–F give a total
  language, which is the point, not a limitation.
- **Union compass anchors use the union's own extents.** `center`/`top`/
  `bottom`/`north`/`south`/`east`/`west` on a `union(...)` now come from
  `analytic_extents` folded over every operand, not the first one — a
  blended body's `top` is the body's top, not its first shape's.
  `surface`/`side`/other shape-specific anchors still delegate to the
  first operand, unchanged. `Intersect`/`Difference` are untouched
  (still delegate everything to their base) — same gap may exist there,
  filed as a follow-up, not bundled into this fix.

## Blast radius reminders

A new `ShapeExpr` variant touches: `ast`, `lexer/token`, `lexer`, `parser`,
`anchors` (extents + resolve + valid names + shape_name), `geometry`
(contains + distance + inset_shape), `scene` (both directions), `spec`
(probes + describe + count), `resolver` (subst + collect idents),
`shader` (GLSL + emit), `bevy_viewer` (primitive_mesh), and
`docs/skill_preamble.md`. The viewer is feature-gated — `cargo test` does
NOT compile it. Use `cargo build --features viewer`.

`SKILL.md` is generated: edit `docs/skill_preamble.md`, then
`cargo run -- skill > SKILL.md`. CI fails on a stale file.