# Working notes

Context that isn't obvious from the source. Decisions and their reasons;
known gaps with their diagnoses. Read this before ROADMAP.md — it's stale
in more places than this file is.

## Form, stage 4 (0.8.0-dev) — hosted parts and `fuse`, what shipped

- **Words** (approved as written): `cuts = TARGET` and `fuses = TARGET` on a
  part, with an optional `blend = k`; `fuse A, B, C blend=k` in a thing's
  body. `TARGET` is `host` or a part of the same thing. None of the words is
  reserved; no existing script used them (the `blend =` lines in scripts are
  `union(…)` arguments). `skin(yaw, pitch)` is left for later.
- **`host`** (`resolver/host.rs`): for a part inside an instance, the object
  of the mate whose subject is inside the instance and whose object is not
  (`Win[0].seat on Wall.north` → `Wall`). A thing printed on its own, or an
  unmated instance, has none: a cut is then not drawn, a fuse is drawn on
  its own, and `moxi check` notes it. A family name in `fuse` takes every
  `Name[k]`; an instance name takes its parts that wear the group's
  material.
- **Assembly** (`pipeline::assemble`, end of `solve_checked`, after the
  constraints): fuse groups first (one union of the members, in order, in
  the first member's frame, with the group's blend), then hosted fuses,
  then hosted cuts, in script order. A hosted part that targets a fuse
  member goes into the group's first part. Each member is placed by its
  solved frame relative to the target: `placed` writes the rotation as
  `spin z(spin y(spin x(…)))` (R = Rz·Ry·Rx, with the gimbal case handled)
  inside an `at`. The solver is not touched: frames, anchors and
  constraints are each part's own.
- **`difference` blend.** `ShapeExpr::Difference` and `scene::Shape::Difference`
  gained `blend` (0 for everything a script writes; the scene JSON omits it
  at 0). A hosted cut's `blend` rounds the rim with the smooth maximum
  `−smin(−d, c, k)`, which is 1-Lipschitz like `smin`, so the sparse mesher
  stays exact. A plain difference keeps its old code path in `distance`,
  `contains` and the GLSL.
- **Outputs.** Absorbed parts stay in the scene with `absorbed: {into, cut}`.
  `Scene::at_rest` (mesh, raymarcher, viewer), `Scene::without_guides`
  (glTF) and `undrawn_parts` (voxels) leave them out; `without_guides` also
  hands an absorbed part's children to the part it went into, so a leaf on
  a fused bough hangs from the trunk's node. The report lists `Hole (cut
  into Wall)`, `Bough (fused into Trunk)` and leaves them out of rows,
  contacts and pieces.
- **Rule.** A pose may not move an absorbed part relative to the part it is
  in (checked by re-solving every pose; the error names the pose and both
  parts). A pose that moves them together is fine.
- **Errors**, each naming the part and the fix: unknown target, a part that
  targets itself, `cuts` and `fuses` on one part, `blend` with neither, a
  target that itself cuts or fuses, `cuts`/`fuses` on an instance part, a
  `fuse` of fewer than two parts, with mixed materials, a part named twice,
  a part in two groups, a hosted part in a group, an unknown name in
  `fuse`, a mirror image whose thing cuts or fuses into its host.
- **Math, checked by test** (`src/host_tests.rs`): `placed` reproduces 200
  random frames and both gimbal poses to 1e-9; a window's cut removes its
  center and changes the wall's distance nowhere away from it (1e-12); the
  same `Window` cuts a thin and a thick wall; every frame is bit-identical
  with and without the cut, and with and without a `fuse`; a fused collar
  changes the host's distance only near the join; a fuse group is one glTF
  node and a child of a member hangs from it; a rounded cut meshes
  bit-identically sparse and dense; families and instances expand by
  material.
- **Demos.** `scripts/FACADE.md`: one `Window` thing on a wall three times
  and on a thicker tower once, plus an arched door cut with `blend`; no
  opening is written into either wall. `scripts/MUG_FUSED.md`: a span
  handle fused into a hollow cup. `scripts/MUG.md` is unchanged.
- **Limit.** A hosted part's `blend` is folded under its thing's defaults,
  so an instance argument that would change it does not.
- **Parity.** `json`, `scene`, `report`, `gltf` and OBJ output are
  byte-identical to the baseline for every existing script, MOXIBOI and
  MUG included. Both demos' web shaders pass `glslangValidator`.

## Form, stage 3 (0.8.0-dev) — spans, what shipped

- **The line.** `Arch from A.top to B.top [reach=…] [sag=… | rise=… | via=…]`
  in a relation block (`Placement::Span`, boxed). `from`, `to`, `via`,
  `reach`, `sag`, `rise` are not reserved. The ends and `via` are `aim`
  targets, so the resolver's `resolve_aim` handles parts, anchors, paths
  into instances, points and `let`-bound points, and `Aim::prefixed` carries
  spans through instancing. `from` must be a part or an anchor (its frame
  is the span's); a direction is never an end.
- **The solve** (`frame_resolver::solve_span`). A span depends on every part
  it touches and is solved once, after them, in the existing toposort. It
  computes, in the `from` part's own space: `Pa`; `Qa = Pa + na·h` if the
  `from` anchor is oriented; the middle (`via`'s point, or the midpoint of
  its neighbors ± `rise`/`sag` along the thing's up); `Qb = Pb + nb·h`;
  `Pb`, with `h = reach·|Pb − Pa|`. The stage 2 tube through those points
  leaves `Pa` along `na` and arrives at `Pb` against `nb` (its end tangents
  are its end chords). The span's frame is the `from` anchor's (part frame ∘
  anchor frame, as every mate), so its path starts at the origin, and its
  joint parent is the `from` part. Working in the `from` part's space means
  a span whose ends all sit on one part has a bit-identical path in every
  pose. The solver returns the paths (`PathMap`, beside `ReflectMap`);
  `fill_paths` puts them into the shapes in `solve_checked` (so constraints,
  joints, the scene, every backend and every pose see the real tube), in
  `solve_notes`, and in the resolver's instance extents. `lookup_anchor`
  reads a span's anchors from its filled tube, so parts mate onto it. The
  `Align` and `Mirror` arms are unchanged; the diff threads `paths` into
  `lookup_anchor`/`target_direction`/`solve_one` and adds one arm in each
  dependency match.
- **Checks.** Every path-less tube is a span's part and every span's part is
  a path-less tube (a part's whole shape); anchors on a span are validated
  on a stand-in path before the solve. Errors name the part and the fix:
  not a tube, a tube with a `path`, a path-less tube with no span, a
  path-less tube inside a union, two middles, `reach` outside 0..1, an
  unknown qualifier, ends at one point, straight-out runs that pass each
  other (with the largest `reach` that works), a point as `from`, a
  direction as an end, a span as a `symmetric_across` source, a span line
  in a pose, an instance as a span's subject.
- **Poses.** A pose re-solves spans. `moxi check` notes a span whose path
  changes (by more than half a grain) in a pose, since glTF animations move
  rigid nodes: "span 'Rope' changes shape in pose 'Wide'; an animation
  through it keeps the rest shape, `moxi gltf --pose Wide` is exact".
- **Report.** A span's `on` column reads `A→B` (`JointOut.to`, written only
  for spans, so scene JSON for existing scripts is unchanged).
- **Math, checked by test** (`src/span_tests.rs`): pinned path coordinates
  for two boxes with `rise`; on a leaned post and a sphere, the world start
  and end equal the anchor points and the end tangents equal `na` and `−nb`
  to 1e-9; `via` and points are on the path; `reach=0` is two points; every
  other part's frame is bit-identical with the span removed; a part mated on
  `side(t, angle)` lands on the tube whatever the line order; a handle on a
  tipping cup keeps a bit-identical local path while a rope follows a post
  moved by a pose (with the note). `scripts/LANTERN.md`: two posts, a
  sagging rope, a lantern hung from the rope's lowest point, its handle a
  span with both ends on the body; no end coordinate in the script.
- **Parity.** `json`, `scene`, `report`, `gltf` and OBJ output are
  byte-identical to the baseline for every existing script, MOXIBOI
  included; all existing solver tests pass untouched. The lantern's web
  shader passes `glslangValidator`.

## Form, stage 2 (0.8.0-dev) — `tube` and `lathe`, what shipped

- **Words.** `tube(path=[[x, y, z], …], radius=r | [r0, …])` and
  `lathe(profile=[[r, y], …])`, approved as written. They are recognized
  only where a shape stands (`Ident` + `(` in `parse_shape_expr`), not
  reserved: `scripts/MUG.md` keeps `let tube = wall * 0.6`. A `shape`
  declaration may not take either name.
- **One module, `src/tube.rs`.** Both curves are centripetal Catmull-Rom
  splines through the given points (mirrored end points, so two points are
  a straight line), sampled per span at the fewest chords (1, 2, 4 … 64)
  whose midpoints are within a tolerance of the curve. The tolerance comes
  from the shape's own numbers (tube: 5% of the thinnest radius, at most
  0.5% of its size; lathe: 0.2% of its size), never from the grain, so the
  mesher, voxels, raymarcher, extents, report and anchors all use the same
  polyline. The tube is the union of exact round cones (Quilez) between
  samples, with a box per segment to skip far ones; the lathe is the
  revolved polygon of the samples, distance to every edge but the one on
  the axis, signed by the even-odd rule. Both are exact distances, so the
  sparse mesher treats them like spheres and boxes. A small per-thread
  cache keyed by the arguments' bits keeps the mesher from resampling.
- **Anchors** (`anchors.rs` dispatches; the math is `Tube::at` and
  `Lathe::at`). Tube: `start`/`end` at the cap tips pointing out along the
  curve, `side(t, angle)` by arc length, angle 0 = a cylinder's +Z turned
  by the smallest rotation onto the start tangent, then carried without
  roll (double-reflection rotation-minimizing frames); the normal tilts by
  the taper, as on the round cone it lies on. Lathe: a cylinder's anchors
  (`top`/`bottom` on the axis, `side(t, angle)` with t along the profile,
  `rim_top`/`rim_bottom(angle)`).
- **Errors.** Malformed arguments are reported at resolve time, per part
  and per instance override: "part 'A' (tube): `path` needs at least two
  points, got 1", a zero radius, a lathe listed top to bottom or with a
  negative radius.
- **Tests.** The round cone against a brute-force swept-sphere distance; a
  two-point lathe equals `cylinder` to 1e-9 everywhere; every `side`
  anchor lies on the surface and points out; `side(t=1, angle=0)` on a
  quarter-circle path is pinned at (0, 10, r); sparse and dense meshes of
  both shapes are bit-identical. The generated GLSL (`sdRoundCone`,
  `latheEdge`) passes `glslangValidator`. `scripts/PITCHER.md` is the
  demo: a lathed body, a tube handle in the body's frame via `origin`.
- **Math, to look at if anything reads wrong.** At a bend, the inside of
  the curve is covered a little by the neighboring round cone, so a `side`
  anchor there can sit up to the sampling tolerance inside the surface.
  `shell` of a tube insets every radius; `shell` of a lathe is left as it
  is for cones (the inset copy is the same shape, so it draws nothing).
- **Parity.** `json`, `scene`, `report`, `gltf` and OBJ output are
  byte-identical to the baseline for every existing script, MOXIBOI and
  MUG included. `moxi mesh` now writes `.mtl` materials in order (mat_0,
  mat_1, …), the same on every run; the OBJ geometry is unchanged.
- **Not built here:** the viewer feature (`cargo build --features viewer`).
  Its shape matches have fallbacks, so tubes and lathes go to the mesher.

## Form, stage 0 (0.8.0-dev) — sparse meshing, what shipped

`src/mesh.rs` only; `geometry::distance` and `contains` are unchanged.

- **Skipping empty space.** `sample_grid` splits the padded grid into an
  octree of corner boxes. A box whose center sample `d(c)` exceeds, in size,
  the radius covering every cell touching its corners (plus one cell) is
  filled with `d(c)` and not sampled. This is exact when the field is
  1-Lipschitz (`is_lipschitz`): no point within that radius can have the other
  sign, so no cell there straddles the surface and surface nets reads only
  signs there. Sphere, box, cylinder, cone, capsule and torus are exact SDFs;
  min, max, the polynomial smooth minimum (its partial derivatives are h and
  1 − h), at/spin/mirror and `scale` (smallest factor over largest) keep the
  property. Ellipsoid, blob and heightfield do not, and any shape containing
  one is sampled densely, as before.
- **Skipping far union operands.** `Field` wraps a root `union` of 8+
  operands. An operand whose distance is exact outside its bounding box
  (`exact_outside`) is not evaluated where the box is farther than the running
  value + 2·blend: there `min` and `smooth_min` both return the running value
  unchanged. The fold keeps the operands' order. A grid of operand boxes gives
  the near operands once the running value is low. Normals use the same field
  through `Field::gradient`, the same arithmetic as `geometry::gradient`.
- **Result.** Tests compare sparse against dense bit for bit on ten shapes
  (plain and blended capsule networks, difference, intersect, scale, shell,
  cone, extrude, blob, ellipsoid) and the culled field against `distance` at
  60,000 points. 500 capsules over a 200-unit box at cell 0.5 take 1.8% of the
  dense samples. Parity: `json`, `scene`, `report`, `gltf` and `mesh` (OBJ)
  outputs are byte-identical to the baseline for every script and bench
  solution, MOXIBOI included; meshing all scripts went from 46 s to 13 s.
- **Not done: item (3).** `MAX_MESH_SAMPLES` coarsening still uses the dense
  count. Raising it would change output for any part that hits the cap today,
  so it needs its own decision.
- **Found, not fixed:** `moxi mesh` writes the `.mtl` materials in hash-map
  order, so the file differs from run to run (the geometry does not). Sorting
  them is a one-line fix that changes every `.mtl` once.

## Form, stage 1 (0.8.0-dev) — what shipped

Named shapes, shape loops, `rand`, `origin` and `guide` (proposals/FORM.md,
stage 1). Where each lives:

- `shape NAME(params) = shape` and calls `Name(args)`: `ast::ShapeDecl`,
  `ShapeExpr::Call`; `for k in a..b { shape }` among `union`, `intersect` and
  `difference` operands: `ShapeExpr::Repeat`. Both are removed by
  `src/shapes.rs::expand`, which runs first in `Resolver::resolve`; nothing
  after it sees them (the defensive arms in `geometry`, `anchors`, `scene`,
  `spec`, `report` and the resolver walkers are there only because the enums
  are matched exhaustively).
- Errors are `MoxiError::ShapeError` (spec count 25): an unknown shape lists
  the declared ones, a recursive one names the cycle (`A -> B -> A`), arity
  names the parameters, loop bounds say what they must be.
- `import shape S`: shapes travel with their file like fns (all of a giving
  file's shapes come along; a name clash is an error). `import.rs` only
  checks that S exists.
- `rand` in `value.rs`: SplitMix64 over the arguments' bit patterns; the
  value of `rand(7, 3)` is pinned in a test because the hash is now part of
  the language.
- `origin` anchor: `anchors::resolve_anchor`, orientation-free, (0,0,0) of the
  part's own frame, so `at()` does not move it. It is in `UNIVERSAL`, so it
  shows in every shape's anchor list, in errors and in SKILL.md.
- `guide`: `PartDecl.guide` -> `ResolvedPart.guide` -> `scene::Part.guide`
  (omitted from JSON when false). Solved like any part, then dropped by
  `Scene::at_rest()` (mesh, raymarcher, viewer), `Scene::without_guides()`
  (glTF), `pipeline::undrawn_parts` (voxels and the notes' bounding box) and
  the report's rows/contacts/pieces; the report lists it as `(guide)`.

**Limits, on purpose.** Loop bounds fold before any thing is resolved, so they
may be numbers or the enclosing `shape`'s parameters but not a thing's own
`param`s or `let`s (the error says so). Supporting them needs the expansion
inside the per-instance substitution, which is a bigger change to the
resolver and was left out of stage 1. A `for` is only an operand: not a
shape of its own, not the base of a `difference`.

**Math, checked.** `origin` is `Frame::from_pos(0)` with `AnchorKind::Free`:
the frame solver then copies the object anchor's world frame, rotation
included (a test mates B to a tipped A and compares both rotations). Mirror,
scale and spin all fix the local origin, so no wrapper changes it; `at()`
deliberately does not move it. On an instance, `Inst.origin` resolves to its
root part's origin, which is the instance's own frame. The defensive arms in
`geometry` return `false` / `f64::MAX` (not ∞, which `smooth_min` would turn
into NaN); they are unreachable because `expand` runs first. Found and fixed
in review: a shape body could see a calling thing's names (now an error that
names the parameter to add), and a parameter or loop variable called
`x`/`y`/`z` rewrote `axis=` (axis words are no longer substituted). An
instance can be a guide too (`part L { thing = Leg, guide }`). The viewer
feature was not compiled here (`cargo build --features viewer`); it matches
no `ShapeExpr` itself.
Parity: every script's `json`, `scene`, `report` and `gltf` output is
byte-identical to the 0.7.0-line baseline, MOXIBOI included. `SKILL.md`'s
version line moved from 0.7.0 to 0.8.0-dev with the regeneration.

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