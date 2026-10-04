# Working notes

Context that isn't obvious from the source. Decisions and their reasons;
known gaps with their diagnoses. Read this before ROADMAP.md — it's stale
in more places than this file is.

## Where the project is

Shipped beyond ROADMAP.md's plan: the scene IR (`src/scene.rs`) is the
canonical artifact every backend reads; an SDF backend (`geometry::distance`)
feeds a surface-nets mesher (`src/mesh.rs`, `moxi mesh`) and a browser
raymarcher (`src/shader.rs`, `moxi web`). Sculptor primitives — capsule,
torus, rounded box — exist across every backend. "A scene is a thing":
global placement via instancing, no new syntax, no `world` block.

M3 (bench runner + corpus) has shipped: `src/bench.rs`, `moxi bench <dir>`,
all assertion types including clusters/coplanar/centroid_spread. 12 of 17
cases have reference solutions in `bench/solutions/`; the remaining 5 are
correctly gated (`gated_on:` in `cases.yaml`) on phases that haven't
shipped — writing a solution for those would defeat the case's own point.
`LANDED_PHASES` in `src/bench.rs` should be checked against what's actually
shipped (D and the Phase-I precursor, at minimum) before trusting which
cases are running vs. skipping.

Living models (DOC-20261004-living-models-design, in the docket book):
step 1 `moxi gltf` (`src/gltf.rs`, `src/joints.rs` — one node per part,
hierarchy = relation tree, pivots at mate frames) and step 2 `pose`
(`src/resolver/pose.rs`; `moxi scene|gltf --pose P`) have shipped. A pose is
resolved after loops and flattening into folded qualifier overrides and
applied by `apply_pose`, which only writes numbers into mate lines — the
frame solver does not know poses exist, and the pose = substitution test
pins that. Every pose is solved and its constraints checked on EVERY
compile (pipeline `solved_parts`), so a broken pose is a script error even
when rest is printed. Not yet: `animate`, `state`, `joint` ranges; bench
cases assert on rest voxels only, so pose cases need a `pose:` field in
`cases.yaml` first.

Remaining to a shippable v1: list values (D.2) → `for` blocks (E) →
lathe/sweep → M4 (moxi eval, the model-facing half of the bench).

The issue-filing kit (bodies, `create_issues.sh`, model routing) lives in
a separate private repo, not here — it contains routing/consulting notes
that don't belong in public source. Check there before re-deriving an
issue body from scratch.

## Known gaps, with diagnoses

**Posing is unpredictable without knowing the anchor convention.** `pitch`
tilts about the socket's local X, `twist` spins about socket Y (the
normal) — but which world direction those ARE depends on the anchor kind.
On a face anchor (`top`, `north`) socket-Y is straight off the face and
`pitch` tilts away from it, as expected. On `side(t, angle)` or a
`surface()` anchor, socket-Y is the RADIAL direction and socket-X is the
MERIDIAN (up the shape's axis) — so `pitch` there sweeps the part AROUND
the shape, parallel to the surface, not away from it. Getting this backwards
produced the same wrong-angle bug three times: MOXIBOI's arms, MOXIBOI's
legs (degenerate tangent basis on a straight-down normal), and SCARECROW's
arms (a horizontal `side` mate given a stray `pitch=12`, which swept both
arms sideways instead of drooping them — visible only from directly above).
To droop or angle a side/surface limb, rotate the tangent frame first:
`twist=-90 pitch=70`, the idiom `SKELETON.md`'s shoulders and hips use.

This is documented in `docs/skill_preamble.md` under Placement — but as of
the last check, only the worked example (`twist=-90 pitch=70 gap=1`) is
there; the explanatory prose above it did not make it into a commit. Worth
confirming it landed before assuming a model reading SKILL.md knows this.

The deeper fix is still open: an aim-at-target placement form
(`Arm.socket on Post.side(...) aim_at=Ground`) would replace the two
coupled rotations with one target point and make the mistake structurally
harder to make. P3 (`aim=`) is a partial answer — it only touches
`surface()`'s own coupling of position and normal, not this.

**Constraints can't see inside instances.** `parse_constraint_stmt` calls
`expect_ident()`, so `constraint RightLeg.Foot below Torso` won't parse,
and the resolver only knows bare names. A composed thing can only
constrain its top level. The placement path already solved this with
`parse_partial_anchor_ref` — copy that.

**P2 `surface(u,v)` is blocking character work.** The pec/ab definition in
the Moxi-Boi reference art needs surface-placed cuts. The torus's own
`surface(angle, phi)` (added with the sculptor primitives) is a working
precedent for what the universal version should generalize.

## Decisions, with reasons

- **A scene is a thing.** No `world` block: a scene is a thing whose parts
  are other things, placed by relation. Reached with zero new syntax.
- **`atom` kept, demoted.** Materials are self-contained; `voxel_atom`
  remains for the genuine case of two materials sharing one atom
  (ISLAND.md's SOIL and TRUNK).
- **`entity` still lexes as a synonym for `thing`** for one release.
- **A union's COMPASS anchors (`top`, `east`, …) come from the union's own
  analytic extents, not its first operand.** Fixed after `SKELETON.md`'s
  blended pelvis showed the inconsistency directly: `Pelvis.top` from the
  first operand landed inside the body (y=3.2, the iliac bowl's own top)
  instead of on the fused sacrum (y=6.2), while constraints — which always
  used full extents — disagreed with the anchor. Shape-specific anchors
  (`surface`, `side`) still delegate to the first operand; that part of
  the rule is unchanged and is what `SKELETON.md`'s `Pelvis.surface(...)`
  hip sockets rely on. This deleted six hand-written `point()` calls from
  an earlier MOXIBOI draft.
- **Lathe and sweep deferred**, not hacked: both need a profile as an
  ordered list of points, and lists aren't a value type yet. Doing them
  early would smuggle a list type in through one feature instead of
  designing it with D.2.
- **A `-` before a digit is a sign only in prefix position** — decided by
  whether the previous token could end an expression (`Int`, `Float`,
  `Ident`, `RParen`, `RBracket`). Fixed after `0-0.6` and `0-1` silently
  lexed as `0` followed by a negative literal instead of subtraction, in
  both MOXIBOI and SCARECROW independently. `x=-1.5` and `degrees=-90`
  are unaffected — a `-` right after `=` was already prefix position.
- **Totality holds.** No general recursion. Phases D–F give a total
  language, which is the point, not a limitation.

## Script versioning

Unsuffixed filename = current idiom. `_v1`, `_v2`, … = earlier idiom,
kept when it demonstrates a real capability gap that has since closed
(`SKELETON_v1/v2/v3.md` each show a different limitation lifting). Do NOT
keep a numbered predecessor whose lesson is fully subsumed by the new
version — fold the note into the surviving file's prose instead
(`SCARECROW.md` did this rather than keeping a `_v1`).

Caution: the historical `_v` files are NOT untouched history. Every
language-wide sweep (the `entity`→`thing` rename, the fence migration, the
atom/material unification) rewrote them in place, so they show OLD IDIOM
IN CURRENT SYNTAX, not what actually compiled at the time. `SKELETON_v1.md`
says `thing`, not `entity`, despite predating the rename.

**GROVE.md and GROVE_v2.md compile to the observably identical model** —
found by inspection, not yet acted on. GROVE_v2 (A.2, compass anchors
without exports) is the one that should survive as `GROVE.md`; GROVE
(hand-exported hip anchors, Phase A) should become `GROVE_v1.md` with its
note about what A.2 removed folded into the surviving file — same
treatment as SCARECROW got. Not yet done.

`AXLE.md`, `MUG.md`, `TREES.md` are still titled with their gating phase
("requires Phase B2" / "requires Phase C") — phases that shipped long
ago. Not yet modernized; SKELETON.md and SCARECROW.md are the reference
for what a modernization pass looks like (parameterized things, sculptor
primitives where they fit, prose explaining what changed and why, anchor
math checked by hand before compiling since these are usually written
without repo access).

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

## Working with a chat session instead of Claude Code

Chat sessions write scripts and anchor math without repo access, checked
by hand rather than compiled. Expect the first `cargo run -- check` to
surface something — most often the pitch/twist confusion above, or the
prefix-minus spacing gotcha, or a stale `point()` coordinate after the
shape it referenced changed. This is normal; report the exact compiler
error rather than describing the symptom, and paste a fresh `ygg` dump
when more than two files have changed since the working file was written
— reconstructing edits from memory of an earlier snapshot is the source
of most cross-session mistakes.