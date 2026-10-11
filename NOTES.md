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
when rest is printed. Step 3 `animate` has shipped too (`src/resolver/animate.rs`; keyframe
joints read off the per-pose solves in pipeline `animations_of`; one glTF
animation per `animate`, LINEAR translation + rotation channels for nodes
that move). The 179° rule is checked twice: on the written qualifiers (a
matrix cannot tell lean 0→270 from 0→−90) and on each joint's turn
relative to its parent (qualifiers each under the limit can still combine
into a half turn). Step 4 shipped: material `emissive` (unknown material
properties are now an error — they used to be stored and ignored) and
`state` (`src/resolver/state.rs`; per-part looks computed in
`compile_to_scene`, only parts that differ from rest; exported as
KHR_materials_variants with `rest` mapped explicitly, glow above 1 as
KHR_materials_emissive_strength). three.js reads emissive strength natively
but needs a small plugin for variants. Step 5 shipped: `joint` ranges
(`src/resolver/joint.rs`) — a check on NUMBERS (each mate's qualifier at
rest and in every folded pose), so no solving; inherited by instances under
their arguments (templates keep raw and default-substituted bounds); limits
and rest values go to `JointOut` and glTF node extras `moxi.joint`. The
rotation AXES of lean/twist/pitch are not exported (they depend on the
anchor kind inside frame_resolver). Bench cases may name a `pose:` (the
runner asserts on that posed solve; `compile_source_posed`).

`revolve` (continuous rotation; `src/resolver/spin.rs`, pipeline
`revolves_of`): `revolve Turn { GearA twist=90  GearB twist=-45 }`, degrees
per second, `twist` or `pitch` only. The resolver picks the period (smallest
time after which every part has made whole turns; rates taken to 1/1000 °/s;
≤ 600 s, ≤ 720 keys) and a key at most every 90°; the pipeline solves each
key as a pose over rest (constraints checked, errors say "in revolve 'X' at
t s"), and gltf exports it as a looping clip. Named `revolve`, not `spin`:
the shape modifier `spin(s, axis=, degrees=)` already means a static
rotation. Instances inherit revolves, animations and states (below).
Demo: scripts/GEARS.md. Parser fix shipped alongside: words after a
relation `constraint` were all taken as (unused) qualifiers, swallowing a
following `pose`/`animate`/`state`/`joint`/`revolve`/`anchor`; they now stop
at those keywords.

Joints on indexed parts (ISSUE-20261004-joint-in-for): `joint` is a loop
body item (`ForBlock.joints`, unrolled like parts, the loop variable and the
body's lets usable in the name and the bounds), and joint names go through
`resolve_markers` like every other indexed name — before this a top-level
`joint Seg[2]` failed quoting the internal marker (`Seg[#4]`). Demo:
scripts/CHAIN.md. Parity: every other script byte-identical (json + scene).
The loop-body UnexpectedToken now lists `joint` (reviewable wording change).

Inheritance of motion (ISSUE-20261004-inherit-motion): an instance `Left`
brings its thing's poses as outer poses `Left.Up` (solved and checked on
every compile; they are the keyframes of inherited animations), animations
`Left.Wave` (times re-folded under the instance's arguments: templates keep
`RawAnim`), revolves `Left.Turn` (rates re-folded from `rate_expr`, period
recomputed) and states `Left.Dim` (`ResolvedState.scope` limits material
attributes to `Left.…` parts; emissive re-folded from `emissive_exprs`;
colours are names and are not re-evaluated). A `symmetric_across` image gets
states but no motion. Names compose through nesting (`A.L.Wave`).

See-through materials (ISSUE-20261004-see-through): material `opacity`
(0..1, stored only below 1) and `blend = normal | additive`; a `state` may
set `opacity` (not `blend`: give the part another material). Scene IR:
`Part.opacity`/`blend`, `Look.opacity`/`blend`, all omitted at default, so
every script's json, scene AND gltf output is byte-identical. glTF: alpha
in `baseColorFactor`, `alphaMode: BLEND`; additive goes in material
`extras.moxi.blend` (glTF has no additive mode — a three.js reader sets
`AdditiveBlending` from `material.userData.moxi.blend`). Voxel and
raymarch backends ignore both. Demo: scripts/CANDLE.md (layered additive
flame stretched by sliding stacked cones). Motivated by engelkraften's
burning man. Filed alongside, not built: ISSUE-20261004-pose-scale
(`scale=` as a posable mate qualifier; touches frame.rs).
The unknown-material-property and state-attribute messages now list the
new words (reviewable wording change).

Closing the circle, slice 1 (DOC-20261011-closing-the-circle, issues L2,
L1, K, I; 2026-10-11). L2: `#` comments after code, `%` (floored modulo,
`a % 0` an error), paths into instances on either side of a mate
(`B.Footing.west`), positional `point(x, y, z, nx, ny, nz)` /
`side(t, angle)` (positional first, then named; too many is an error),
`twist=(0..110)` read as the range (two ranges refused). L1: an `aim`
turning more than 150° with no `facing` gets a NOTE (`check`, `report`):
near a half turn the roll swings with the target —
`frame_resolver::resolve_frames_noted` returns the notes, no maths
changed (⚠ boundary file, plumbing only). K: material `roughness` (0..1,
default 0.9) and `metalness` (default 0), state-changeable, re-folded
under instance arguments (`surface_exprs`); glTF factors, OBJ/MTL `Ns`
and `Ks` (`export::highlight`: roughness 0.9 → exponent 24 / strength
0.3, the old fixed highlight), raymarcher `matSurf` emitted only when a
material sets one — defaults byte-identical everywhere. I: any set of
states may be on at once; per part the later state in
`scene::state_order` wins (inherited states first, then the thing's own);
`state X in Family` (at most one on); glTF adds node
`extras.moxi.states` {state: material} and scene
`extras.moxi.stateOrder` / `stateFamilies` — additive: with those two
blocks off every glb is byte-identical, and only glbs of scripts with
states change; `moxi report --state A,B` lists looks and overlaps.
Rule refinement against the DOC: overlap is decided per PART (its whole
look), not per material attribute — a node takes one material, and
attribute merging would need a material per combination. Demos:
WALLRUN.md, RELIC.md, STATES.md. Reworded errors (reviewable): the
material property list, the state attribute lists, '{' after a state
name.

Issue G, `after { … }` and `reveal` (2026-10-11): a breakable's after block
holds `Part material = M` swaps and `reveal Part`; `breakable.rs`
resolves them (names as in a state: a part, an instance part, a whole
instance), mirrors map them through `symmetric_across` images, and
`check_after` keeps every line at the cut (the lump, its parent, the
parent's other children) and each revealed part to one lump. IR:
`BreakOut.after` {part: Look}, `BreakOut.reveal`, `Part.hidden`. Hidden
parts are left out of the voxel loop (`compile_source_posed`) and of
every drawing of rest through `Scene::at_rest()` (mesh, glsl/web, the
bevy viewer — one call per entry point instead of a filter per loop);
glTF keeps them (`moxi.hidden`, `moxi.breakable.after` as material
indices). The report leaves still-hidden parts out (a `hidden:` line);
`--break` shows what it reveals, lists its swaps as `look:` lines and does
not count a revealed part against its own lump as torn. Demo STUMP.md.
Every existing output byte-identical (none declares `after`).
Found while testing: `print Demon` prints nothing of Demon when another
thing in the file instances it — the instancing thing is printed instead
(belongs with L3's `print A, B`).

Issue J, imports (2026-10-11, DOC-20261011-imports): `from "lib/x.md"
import A, B as C, material M` (contextual words; `ast::ImportDecl`).
`src/imports.rs` loads the main file and every file it imports (each once,
cycles named), computes what the main file's imports need (the named
declarations plus their things/materials from their own file,
transitively), and merges those into the main document BEFORE the one
resolver: names the main file gives keep them, the rest get private keys
(`Ember@lib_x_md`) no script can write. Imported declarations go first,
a file after the files it imports, each in its file's order (a thing must
be declared before it is instanced). Prints in imported files are ignored;
generators are not importable; imported `fn`s keep their names (a clash is
an error for now). Every public compile entry point takes
`impl Into<Program>` (`From<&str>`), so single-source calls are as before;
the CLI passes `Program::Files` with `FsSources` rooted at the script's
folder; wasm adds `compile_moxi_files(main, files_json)`. `Span` carries a
file id (col narrowed to u32 so a span stays 16 bytes — Placement's
large-enum gate); `CompileError.file` names the imported file (absent for
the main file, so wire JSON is unchanged); the CLI prints `(file:line:col)`.
Exports show private names as written (`shown_name`), and the root part of
an instance of an imported thing carries `Part.from` / glTF `moxi.from`
(file, thing). `ResolvedEntity.instances` lists every (prefix, thing),
nested too. Demo HEARTH.md + lib/FLAME.md. Every existing output
byte-identical. Reworded (reviewable): '{' after a state name earlier;
none here.

Issue L3, small resolver/report fixes (2026-10-11): (6) `aim=p` / `facing=p`
where `p` is a `let`-bound point `[x, y, z]` becomes that point, in mates
and poses; read from the thing's RAW lets so an instance's own arguments
move it (`Aim::bind_named_point`, `resolver::bind_aim_point[_raw]`).
`aim` from a `center` mate stays refused (the axis is the question), but the
error now names the fix: an oriented `X.point(x=0, y=0, z=0, nx=0, ny=1,
nz=0)` where the center is. (7) `instance_compass_anchor` no longer masks
the instance's own solve error as "has no anchor 'bottom'": it reports
`instance 'A': needed 'A' placed to find its 'bottom', but its own
placements do not solve: <the solver's text>`. (8) the report's `touching:`
line names the members that touch (`Fin[2]–Body`, `ToothA[3]`), and a
family only when every member does. (9) `print A, B detail=low` is one
statement per name; a thing named in a `print` is drawn even when another
thing instances it (it used to vanish into its instancer); SKILL.md says
every uninstanced thing is a top node. (10) notes (never errors) in
`moxi check`/`report`: an `animate` whose keys are all one pose; two drawn
things whose boxes share more than half of the smaller (all printed things
sit at the origin). A `constraint` in a thing now travels with each
instance, prefixed (`'A.H' above 'A.U'`), and is checked on the instancer's
solve — rest and every pose, under the instance's own arguments (it was
never checked when the thing was only instanced). Demo scripts/GAZE.md.
Parity: json/scene/gltf byte-identical on every script and engelkraften
asset; `report` differs only in `touching:` lines (FACING, GEARS and 19
engelkraften assets: pairs were over-claimed as whole-family) and two new
overlap notes (causeway SpanWhole/SpanBroken, ruin Corner/Wall — alternatives
the game swaps between, on purpose). Reworded (reviewable): the aim/facing
orientation-free error gains a suffix naming the `point(...)` fix; the old
text is its prefix. ⚠ touches solver: `frame_resolver.rs` only for that
message (a `free_fix` helper); no math.


Issue N, composable actions (2026-10-11, DOC-20261011-composable-actions):
`action Name { [trigger P hits=N]  effect… }` with effects `detach P
[push=] [strength=]`, `reveal P`, `hide P`, `P material = M`, `state S
on|off`, `play C`, `run A`, `delay=` on any line and `for` inside the body
(unrolled in `resolver/action.rs::flatten`, markers folded there; the
thing-level marker pass leaves loop bodies alone). Instances inherit
actions prefixed and under their own arguments (`RawAction`, like
`RawBreak`); `run` targets are checked once own and inherited actions are
known, and a `run` cycle is an error naming the chain (totality). The scene
IR gains `Layer.actions` (omitted when empty); `moxi gltf` writes them in
`scenes[0].extras.moxi.actions` (until N2, `breakable` was exported there
too, as `break.<Part>` or its event); a look names its
material by index and by name. A part an action reveals is hidden at rest;
`moxi report` says `(until action A runs)`. Parity: voxels, json, scene and
report byte-identical on every script and bench solution; gltf differs only
by the scene's `actions` on scripts with a breakable (BREAK, STUMP, and the
game's breakable assets: JSON and binary otherwise equal, checked). New
error variant `ActionError` (the catalogue count is 25). Demo
scripts/ACTIONS.md. Not built: other triggers, `report --run`, `every`,
KHR_interactivity export.


Issue N2, `breakable` leaves the language (2026-10-11, Andrew: actions are
the only way to say behaviour; the language stays clean). The parser answers
`breakable` with an error showing the action to write; `BreakDecl`,
`AfterLine`, resolver/breakable.rs, `Part.breakable`/`BreakOut`, the node's
`moxi.breakable` in glTF and `BreakError` are gone (`Push` lives in
resolver/action.rs; the error catalogue is 24). An action may sit in a
thing-level `for` with an indexed name (`action BreakLeg[k] { … }`). A
mirrored instance brings its source's actions renamed to its own parts, under
the source's arguments (`RawAction::imaged`, `mirror_of`), pushes reflected
by the image part's mirror wrapper; a part mirrored inside a thing gets its
own action written out. `moxi report --run A,B` (follows `run`; detaches as
pieces, reveals shown, hides dropped, looks listed) replaces `--break`.
imports.rs renames private materials named by an action's look (the same
walk `after` swaps had; missing since Issue N). BREAK, STUMP and ACTIONS
rewritten: voxels identical, scene identical but for `breakable` becoming
`actions`, glTF identical but for the node extras and the scene's actions
(binary equal), STUMP's report says `(until action D.BreakHead runs)`.

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