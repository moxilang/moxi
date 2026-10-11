> **Generated file.** Everything below the first `---` is hand-written prose from `docs/skill_preamble.md`. Everything after the second `---` — the Generated Reference — is rendered by `moxi skill` from the compiler's own tables in `src/spec.rs`. Do not hand-edit the reference section; run `moxi skill > SKILL.md` after a language change, and `moxi skill --check` to verify it's current (CI does this).

# Moxi — SKILL.md

Moxi describes a 3D object as **parts** and **how they attach**; the compiler
works out where everything goes. Code lives inside ` ```moxi ` fences —
anything outside a fence is prose and is ignored.

## A complete file

```moxi
material Iron  { color = "#333333" }
material Glass { color = yellow }

thing Lamp(height=300, reach=90) {
    part Base { shape = cylinder(height=12, radius=45), material = Iron }
    part Post { shape = cylinder(height=height, radius=6), material = Iron }
    part Arm  { shape = capsule(height=reach, radius=4), material = Iron }
    part Bulb { shape = sphere(radius=18), material = Glass }

    relation {
        Post.bottom on Base.top
        Arm.bottom  on Post.side(t=0.92, angle=0) lean=(20, 0)
        Bulb.center on Arm.top
    }

    resolve voxel_size = 3
}

print Lamp detail=low
```

- Declare in this order: `material`, then `fn`, then `thing` (a thing before
  any thing that uses it), then one `print`.
- Every `thing` ends with `resolve voxel_size = N`. Pick N near 1/100 of the
  object's largest dimension; smaller is finer and slower.
- Numbers are in whatever unit the task uses. Do not convert.
- Code is ASCII. `#` at the start of a line is a comment.

## Materials

`material Name { color = red }` — named colors: `red orange yellow green blue
purple white black gray brown ivory maroon peach`, or a hex string `"#c96f4a"`.
`emissive = 1.5` makes it glow in its own colour (0 = no glow).
`opacity = 0.4` makes it see-through (0 invisible, 1 solid); `blend = additive`
adds its colour to whatever is behind it, so overlapping parts get brighter —
fire, sparks, glow, ghosts (`normal` is the default). Both are for the mesh and
glTF output; voxels stay solid.
`roughness = 0.2` sets the surface, 0 mirror-smooth to 1 chalk-dry (default
0.9): wet blood, polished brass, glass low; bone, ash, cloth high.
`metalness = 1` makes it bare metal (default 0; anything not metal stays 0).
They go to glTF, OBJ and the raymarcher; voxels have no surface.

## Parts and shapes

A part is a shape (`shape = …, material = …`) or an instance of another thing
(`thing = Wing(span=54)`). The shape table is in the reference below.
`sphere`, `box`, `ellipsoid` and `torus` are centered; `cylinder`, `cone` and
`capsule` stand on their base with their axis up.

Combine shapes into one solid with `union(a, b, blend=k)` (blend rounds the
seam), `difference(base, cut, …)` and `intersect(a, b)`. Wrap any shape with
`at(s, x=, y=, z=)`, `spin(s, axis=y, degrees=)`, `mirror(s, axis=x)` or
`scale(s, x=, y=, z=)`. A cut must reach past the faces it opens: a bore
through a 10-tall disc is `at(cylinder(height=12, radius=5), y=-1)`. A
combined shape's anchors come from its **first** operand (for `difference`,
the base), so put the shape you will attach things to first — and `at()`
moves those anchors along with the geometry.

## Which way is which

**+Y is up, +Z is the front, +X is right** as seen from the front. On every
shape, `top`/`bottom` face up and down, `north` is the front face, `south` the
back, `east` the right, `west` the left. A character's **own** right side is
`west`: name parts for the character.

## Placing parts

`Part.anchor on Other.anchor` makes the two anchors meet with their normals
opposed: the part points straight out of the anchor it is placed on.
On a sphere or ellipsoid the compass anchors are single points (the
bottom-most point, the front-most…); to attach around a curved body, use
`surface(yaw, pitch)` or sink the part in with a negative `gap`.
Anchors are the compass faces above plus shape-specific ones —
`side(t, angle)` on cylinders, cones and capsules (t from 0 at the base to 1
at the top, angle 0 = front), `surface(yaw, pitch)` on spheres and
ellipsoids — listed per shape in the reference. `center` has no direction:
the part keeps its own orientation.

Qualifiers, all optional:

- `gap=g` — move along the normal; negative sinks the part in.
- `shift=(a, b)` — slide across the face:

  | Face | `a` | `b` |
  |---|---|---|
  | `north`, `south` | up | right |
  | `east`, `west` | up | front |
  | `top`, `bottom` | right | front |
  | `side`, `surface` | up | around, toward increasing angle / yaw |

- `lean=(a, b)` — tip the part toward those same directions, in degrees.
  On a post's `side`, `lean=(20, 0)` is 20° upward. On `surface(yaw=-35, …)`,
  `lean=(0, 35)` swings the part back to face straight ahead (yaw 0). Use
  `lean` to angle arms, branches and struts.
- `twist=d` — spin the part about the normal. To set which way a part
  faces, use `facing` (below); keep `twist` for joints and motion.
- `aim=target` — point the part's axis (the way it leaves the anchor) at a
  part (`aim=Hammer`, its center), an anchor (`aim=Hammer.top`, or
  `aim=Arm.Hand` inside an instance), a point `aim=[0, 120, 40]` in the
  thing's space, or a direction `north`, `south`, `east`, `west`, `up`,
  `down`. It turns the axis the shortest way and never rolls the part, so
  `Spike.bottom on Dome.surface(yaw=…, pitch=…) aim=up` stands a spike
  straight up wherever it sits. `lean`, `twist` and `pitch` still apply,
  starting from the aimed direction. When you know where a part should
  point, use `aim` instead of working out a `lean`. A part cannot aim at
  anything that hangs off it, and a `center` mate has no axis to aim.
- `facing=target` — after any tip, swivel the part about its own axis until
  its front points as close as possible to the target. It takes the same
  targets as `aim`, for example `Head.bottom on Neck.top facing=north` or
  `facing=Angel`. A part's front is its own `north`; a part mated by its
  `north` or `south` uses its `top` as its front instead. `facing` reads the
  same on every face, so it is how to set which way a part faces. `twist`
  then turns from the faced direction. `facing` only swivels: to look down
  at something, tip the axis with `aim` too. A target along the part's own
  axis is an error.
- Three things to know about `aim` and `facing`:
  - An `aim` that turns the axis almost a half turn (target nearly behind
    the socket's normal) has no settled roll: the smallest move of the
    target swings it. `moxi check` notes it; add `facing=` to fix the roll.
  - A part's front switches from `north` to `top` when its axis comes
    within 45° of its own `north` or `south`, so `facing` can jump there.
  - `lean`, `twist` and `pitch` after `aim` are offsets from the aimed
    direction: with any of them the axis no longer points at the target.

Shorthand for separate objects: `A above B`, `below`, `left_of`, `right_of`,
`in_front_of`, `behind`. For features **on** a surface (eyes, buttons,
handles), use an explicit mate with `shift`, not `left_of`.

`L symmetric_across Body from=R` makes L the mirror image of R across Body.

**Each part is placed at most once, and every part but one is placed.** The
one part with no placement is the root; everything else hangs off it through
relations. A second unplaced part is a compile error that names it.

## A reusable thing, mirrored

```moxi
material Shell { color = "#2a6f4a" }
material Film  { color = "#cfe8f0" }
material Black { color = black }

thing Wing(span=60, chord=14) {
    part Blade { shape = ellipsoid(rx=span / 2, ry=1, rz=chord / 2), material = Film }
    anchor root = Blade.west
    resolve voxel_size = 1
}

thing Dragonfly {
    part Thorax  { shape = ellipsoid(rx=8, ry=8, rz=14), material = Shell }
    part Tail    { shape = capsule(height=70, radius=3), material = Shell }
    part Head    { shape = sphere(radius=7), material = Shell }
    part EyeL    { shape = sphere(radius=3), material = Black }
    part EyeR    { shape = sphere(radius=3), material = Black }
    part WingFR  { thing = Wing }
    part WingFL  { thing = Wing }
    part WingBR  { thing = Wing(span=54) }
    part WingBL  { thing = Wing(span=54) }

    relation {
        Head.south on Thorax.north gap=-2
        Tail.top   on Thorax.south
        EyeR.center on Head.surface(yaw=-40, pitch=20)
        EyeL symmetric_across Head from=EyeR
        WingFR.root on Thorax.west shift=(4, 5)  lean=(8, 0)
        WingBR.root on Thorax.west shift=(4, -5) lean=(4, 0)
        WingFL symmetric_across Thorax from=WingFR
        WingBL symmetric_across Thorax from=WingBR
    }

    resolve voxel_size = 1
}

print Dragonfly detail=low
```

`anchor root = Blade.west` exports a socket so the instance can be placed by
it; an instance also answers `top`, `north` etc. for its whole assembly.
Instance parts are named `WingFR.Blade`.

## Using another file's things

`from "lib/flame.md" import Flame` at the top of a file makes the thing
`Flame` from that file usable here, with its poses, states, joints and
breakables; instance it as usual (`part F { thing = Flame(height=30) }`).
The path is relative to the importing file (`..` climbs). Several names:
`import Flame, Ember as Coal, material Blood` — `material` imports a
material, `as` renames. Whatever an imported thing uses from its own file
comes along privately: you cannot name it and it never clashes with your
names (your `Bone` and the flame's `Bone` stay apart). A library file may
`print` itself; that print is ignored when it is imported. Every name traces
to its file: there is no `import *`. Imports cannot go round in a circle.

## Values, lists and functions

```moxi
material Wood  { color = brown }
material Metal { color = gray }

fn bar_length(i) = 120 - 11 * i

thing Xylophone(n=8, spacing=14) {
    let lengths = [for i in 0..n { bar_length(i) }]
    let longest = if n > 0 { lengths[0] } else { 0 }

    part Frame { shape = box(width=n * spacing, height=6, depth=longest), material = Wood }
    for i in 0..n {
        part Bar[i] { shape = box(width=10, height=4, depth=lengths[i]), material = Metal }
        relation { Bar[i].bottom on Frame.top shift=((i - (n - 1) / 2) * spacing, 0) }
    }
    resolve voxel_size = 1
}

print Xylophone detail=low
```

A thing's parameters need defaults; an instance overrides them with any
expression of the caller's values: `thing = Xylophone(n=count * 2)`. `let`
names a value; `if` is an expression with a mandatory `else`; `fn` is a pure
one-expression function. Lists are `[a, b]` or `[for i in a..b { expr }]`,
indexed from 0 with `xs[i]`, sized with `len(xs)`; a point is `[x, y, z]`.
Math: `sin cos tan sqrt abs floor round pow min max clamp lerp` (angles in
degrees). `-x` works on any expression.

## Repetition

```moxi
material Hull { color = white }
material Trim { color = red }

thing Rocket(fins=4, length=200, radius=20) {
    part Body { shape = capsule(height=length, radius=radius), material = Hull }
    part Nose { shape = cone(height=60, radius=radius), material = Trim }
    relation { Nose.bottom on Body.top gap=-radius }

    for i in 0..fins {
        part Fin[i] { shape = box(width=4, height=50, depth=35), material = Trim }
        relation { Fin[i].south on Body.side(t=0.12, angle=360 / fins * i) }
    }

    resolve voxel_size = 2
}

print Rocket detail=low
```

`for i in a..b { … }` repeats the parts, `relation { … }` blocks, `let`s and
`joint`s inside it for i = a … b−1. `Fin[i]` names each copy; refer to one anywhere as
`Fin[0]`. A ring of copies is `side(angle=360 / n * i)`; a row is
`shift=(i * spacing, 0)`. To chain copies, place the first outside the loop
and loop from 1: `Seg[k].bottom on Seg[k-1].top`. Nested loops make grids.

## Scenes and terrain

A scene is just a thing whose parts are other things, placed by relation
like anything else. `heightfield(seed=, radius=, noise=, max_height=)` is
terrain; a `generator` scatters a thing over the printed scene's surface:

```text
generator Forest {
    scatter Tree
    count = 40   min_spacing = 5   seed = 7
    where = elevation > 3 and slope < 0.5
}
```

## Pieces that come off

`breakable Head { hits=5 }` says the mate that places `Head` can break. When
it breaks, `Head` and everything mated under it come off as one piece, which
keeps its own breakables: `breakable EyeR { hits=1 }` still pops an eye off
the fallen head. The trigger is `hits=N` (N hits on the piece or anything
inside it) or `event=Name` (the game fires `Name`, e.g. a rocket's
`event=Separate`), exactly one. `push=out` (the default: away from what it
hung from) or a direction word says which way the piece is kicked. The root
cannot break. To make several sibling parts one piece, make them a `thing`
and break the instance. A mirror image breaks like its source. Instances
inherit their thing's breakables.

`after { … }` says what is left once the piece has broken:
`breakable Head { hits=5  after { Neck material = Flesh  reveal Stump } }`.
`Part material = M` swaps a part's look; `reveal Part` names a part that is
not there until the break (a stump, an exposed ring). An after line touches
only the cut: the piece itself, the part it hung from, or another part
hanging from that one. A revealed part is placed like any part, revealed by
one piece only; a mirror image's after names the images of those parts, so
mirror a stump along with its arm.

## Checks

`constraint A above B` (also `below`, `inside`, `surrounds`) fails the
compile with the measured numbers if the solved geometry disagrees.

After a build, read `moxi report <script>` (add `--pose P` for a pose, `--full`
to list every member of a `Fin[0..3]` family, `--json` for the machine form).
It prints each part's parent, center, size, and where its top and front point
(`up`, `north 20° up`, `east`), then what touches what, then any piece that
floats free with its gap and nearest part. It reads the solved scene only; a
part you meant to attach that shows up as its own piece is the bug.

For a part declared `breakable`, `moxi report <script> --break Head,EyeR`
reports the scene after those lumps come off: the root's remainder as piece 1,
each lump as its own piece (`broken off at Head`), and a `torn:` line listing
contacts the cut tears through, meaning a part in the lump touching a part
outside it other than across the broken mate (a collar that hangs off the body
but overlaps the head). Some overlap is intended; read the line and decide.
Names are the printed ones (`D.Head` inside an instance). A part that is not
declared `breakable` is an error that lists the ones that are. Nothing moves:
the lumps stay where they were solved, `--json` adds `broken` and `torn`.
Parts still waiting for their break are left out and listed on a `hidden:`
line; `--break` shows the parts it reveals and its after swaps as `look:` lines.

## Poses

A pose is the same thing with some mate qualifiers set to other values. Inside
the lamp above, after `relation`:

```text
pose Up     { Arm lean=(70, 0) }
pose Tucked { Arm lean=(-60, 0)  Bulb gap=-4 }
```

A pose line names a part placed by one of this thing's own mates and the
`twist`, `pitch`, `gap`, `shift`, `lean`, `aim` or `facing` it takes in that
pose — absolute values, like the mate line's, and expressions are fine. Each
pose is solved from scratch, so a part that aims at or faces another follows
it when a pose moves the target. `for` works as
elsewhere. An instance takes its thing's pose by name: `Left pose=Folded`.
A pose cannot move the root, a `symmetric_across` image (pose its source;
the image follows) or a part placed inside an instance (use that thing's
poses). Every pose is solved and every `constraint` checked in it on each
compile; `moxi scene --pose Up` and `moxi gltf --pose Up` print one.

`animate Wave loop { 0 rest  0.5 Up  1 rest }` strings poses together:
times in seconds, starting at 0 and increasing, each a pose or `rest`;
`loop` is optional. `moxi gltf` exports one animation per `animate`. Only the
keyframes are solved, so no joint may turn more than 179° from one keyframe
to the next — add a keyframe in between.

`revolve Turn { GearA twist=90  GearB twist=-45 }` turns mates forever, in
degrees per second: `twist` about the mate's normal (a gear on its plate, a
halo), `pitch` about the socket's x axis (a wheel on its edge). It loops by
itself: the clip lasts until every part has made whole turns, so pick rates
with a simple ratio (two gears with 12 and 24 teeth: 90 and -45). Same parts
as a pose may name; a `joint` range on that qualifier is an error (a full
turn cannot fit). Not the shape modifier `spin(s, …)`, which rotates once.

`state Angry { Eyes.color = red  Eyes.emissive = 2  EyeR material = Hole }`
changes materials only: a material's `color`, `emissive`, `opacity`,
`roughness` or `metalness` (every
part wearing it), or which material a part — or every part of an instance — wears.
Any set of states may be on at once; where two on states change the same
part, the one declared later wins that part's whole look (a thing's own
states come after the ones its instances bring). `state Calm in Eyes { … }`
and `state Angry in Eyes { … }` form a family: at most one of them is on.
`moxi report --state On,Broken` shows the looks and every part two states
fight over. `moxi gltf` exports one material variant per state, plus what a
game needs to combine them.

`joint Arm { lean=(-90..120, -20..20)  twist=-30..30 }` bounds what a mate's
qualifiers may be, at rest and in every pose (instances inherit it); a
violation quotes the value and the range. In a loop, one per copy:
`joint Seg[k] { lean=(-10 * k..10 * k, 0..0) }`.

An instance inherits all of it, named after the instance and evaluated under
its arguments: inside a desk, `part Left { thing = Lamp }` brings poses
`Left.Up`, animations `Left.Wave`, revolves `Left.Turn` and states `Left.Dim`
(a state's material changes reach only that instance's parts). A
`symmetric_across` image inherits states but no motion — it follows its
source.

## When the compiler says no

Errors name the stage, the location, and the valid choices — an unknown
anchor lists every anchor that shape has. The fix is almost always in the
message.

---

# Generated Reference

Emitted by `moxi skill` from `src/spec.rs` — version `0.4.0`.

## Shapes

| Shape | Arguments | Local origin |
|---|---|---|
| `sphere` | `radius` (f64, default 1) | centered |
| `cylinder` | `height` (f64, default 1), `radius` (f64, default 0.5) | base at origin, axis +Y |
| `box` | `width` (f64, default 2), `height` (f64, default 2), `depth` (f64, default 2), `round` (f64, default 0) | centered; round > 0 fillets the corners |
| `cone` | `height` (f64, default 1), `radius` (f64, default 0.5) | base at origin, apex +Y |
| `ellipsoid` | `rx` (f64, default 1), `ry` (f64, default 1), `rz` (f64, default 1) | centered |
| `blob` | `radius` (f64, default 1), `roughness` (f64, default 0.2) | centered (nominal sphere; noise never perturbs anchors) |
| `heightfield` | `seed` (i64, default 42), `radius` (f64, default 50), `noise` (f64, default 0.3), `max_height` (f64, default 20) | base at origin |
| `shell` | `inner_offset` (f64, default 1) | same as its inner shape |
| `extrude` | `height` (f64, default 1) | base of profile at origin, extruded +Y |
| `capsule` | `height` (f64, default 1), `radius` (f64, default 0.5) | base at origin, axis +Y; rounded caps extend radius beyond each end |
| `torus` | `major_radius` (f64, default 2), `minor_radius` (f64, default 0.5) | centered, ring in the XZ plane, axis +Y |
| `union` | `blend` (f64, default 0) | compass anchors (center/top/bottom/north/south/east/west) use the union's own combined extents; surface/side and other shape-specific anchors delegate to the first operand; blend > 0 fillets the joins |
| `intersect` | — | delegates to the first operand |
| `difference` | — | delegates to the base |
| `at` | `x` (f64, default 0), `y` (f64, default 0), `z` (f64, default 0) | delegates to the child, translated |
| `spin` | `axis` (ident, required), `degrees` (f64, default 0) | delegates to the child, rotated |
| `mirror` | `nx` (f64, default 1), `ny` (f64, default 0), `nz` (f64, default 0) | delegates to the child, reflected across the plane through the local origin with normal (nx, ny, nz); `axis=x|y|z` is shorthand |
| `scale` | `x` (f64, default 1), `y` (f64, default 1), `z` (f64, default 1) | delegates to the child, stretched per axis about the local origin |

### Anchor vocabulary per shape

- **sphere**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), surface(yaw, pitch)
- **cylinder**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), side(t, angle), rim_top(angle), rim_bottom(angle)
- **box**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz)
- **cone**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), apex, base, side(t, angle)
- **ellipsoid**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), surface(yaw, pitch)
- **blob**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), surface(yaw, pitch)
- **heightfield**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), surface(x, z)
- **shell**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), surface(yaw, pitch)
- **extrude**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz)
- **capsule**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), side(t, angle)
- **torus**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), surface(angle, phi), outer(angle), inner(angle)
- **union**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz)
- **intersect**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz)
- **difference**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), surface(yaw, pitch)
- **at**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), surface(yaw, pitch)
- **spin**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), surface(yaw, pitch)
- **mirror**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), surface(yaw, pitch)
- **scale**: center, top, bottom, north, south, east, west, point(x, y, z, nx, ny, nz), surface(yaw, pitch)

## Relation keyword sugar

| Keyword | Subject anchor | Object anchor |
|---|---|---|
| `above` | `bottom` | `top` |
| `below` | `top` | `bottom` |
| `left_of` | `east` | `west` |
| `right_of` | `west` | `east` |
| `in_front_of` | `south` | `north` |
| `behind` | `north` | `south` |
| `outside` | `west` | `east` |
| `inside` | `center` | `center` |
| `surrounds` | `center` | `center` |
| `touch` | `bottom` | `top` |
| `adjacent_to` | `bottom` | `top` |
| `attached_to` | `bottom` | `top` |

**`symmetric_across`**:

```
SUBJECT symmetric_across PLANE from=SOURCE [axis=x|y|z]
```

Not a subject/object anchor pair — reflects SOURCE's solved frame across PLANE's anchor point.

## Qualifiers

- **`aim`** — applies to: explicit `on` and every relation-keyword sugar form except symmetric_across — where the part's axis points: a part (`aim=Hammer`, its center), an anchor (`aim=Hammer.top`, partial refs into instances allowed), a point `[x, y, z]` in the thing's space, or a direction north|south|east|west|up|down; tips the axis the shortest way, never rolling it; lean/twist/pitch then act from the aimed rest; needs a directional anchor on both sides
- **`axis`** — applies to: symmetric_across — default: `x`
- **`facing`** — applies to: explicit `on` and every relation-keyword sugar form except symmetric_across — after the tip, swivels the part about its axis until its front points as close as possible to the target — same targets as aim; a part's front is its own north, or its own top when it is mated by its north or south; twist is then an offset from the faced rest; a target along the part's axis is an error
- **`from`** — applies to: symmetric_across — required
- **`gap`** — unit: world units — applies to: explicit `on` and every relation-keyword sugar form except symmetric_across
- **`lean`** — unit: degrees — applies to: explicit `on` and every relation-keyword sugar form except symmetric_across — a pair `(a, b)` tipping the part toward the same a/b directions as shift, a first; never sweeps around a curved host or rolls the part; needs a directional anchor on both sides
- **`pitch`** — unit: degrees — applies to: explicit `on` and every relation-keyword sugar form except symmetric_across
- **`shift`** — unit: world units — applies to: explicit `on` and every relation-keyword sugar form except symmetric_across — a pair `(a, b)` sliding the mate across the face; on flat faces a/b are up/right (front, back), up/front (sides), right/front (top, bottom); on curved anchors up/around. `gap` is the same translation along the normal
- **`twist`** — unit: degrees — applies to: explicit `on` and every relation-keyword sugar form except symmetric_across

## Math functions

Usable inside any expression — a shape argument, a `let`, a qualifier. Positional: `sin(90)`, `clamp(x, 0, 1)`. Angles are degrees.

sin, cos, tan, sqrt, abs, floor, round, pow, min, max, clamp, lerp, len

## Error catalogue

Every error names its stage, and — for anchor and instance errors — the full valid vocabulary. Read the message; the fix is almost always in it.

- **`UnexpectedChar`**: [1:1] unexpected character '!'
- **`UnterminatedString`**: [1:1] unterminated string literal
- **`UnexpectedToken`**: [1:1] expected shape primitive, got '']''
- **`UnexpectedEof`**: unexpected end of file, expected closing '}'
- **`UndefinedName`**: [1:1] 'Foo' is not defined
- **`DuplicateName`**: [1:1] 'Skull' is already defined in this scope
- **`UndefinedMaterial`**: [1:1] material 'Bone' is not defined
- **`UndefinedAtom`**: [1:1] atom 'BONE' is not defined
- **`ConstraintViolation`**: constraint violated: 'Skull' above 'Ribcage': expected Skull.bottom.y ≥ Ribcage.top.y, got 3.00 < 5.00
- **`UndefinedAnchor`**: [1:1] part 'Trunk' has no anchor 'sidee' — valid anchors: center, top, bottom, north, south, east, west, point(...), side(t, angle), rim_top(angle), rim_bottom(angle)
- **`BadAnchor`**: [1:1] anchor 'side' on part 'Trunk': t must be in [0, 1], got 1.4
- **`UnplacedParts`**: [1:1] thing 'Ribcage': 'RibL[0]', 'RibL[1]' have no placement; its root is 'Spine' — only the root may go unplaced: place each one on another part (`Part.anchor on Other.anchor`) or mirror it with `symmetric_across`
- **`InstanceError`**: [1:1] instance 'RightArm': thing 'Arm' must be declared before it is instanced
- **`ExprError`**: [1:1] 'lenth' is not defined — in scope: girth, length
- **`FnError`**: [1:1] 'taper' takes 2 arguments (i, n), got 1
- **`PoseError`**: [1:1] pose 'Smash': 'ArmL' is the mirror image of 'ArmR'; pose 'ArmR', or give 'ArmL' its own mate
- **`AnimationError`**: [1:1] animation 'Swing': 'ArmR' lean turns 200° between 0.08 s and 0.17 s — insert a keyframe between them
- **`MaterialError`**: [1:1] material 'Eyes': no property 'emisive' — a material has: color, voxel_atom, emissive, opacity, blend, roughness, metalness
- **`StateError`**: [1:1] state 'Angry': no material 'Eyez' — materials: Eyes, Skin, Gold
- **`JointError`**: [1:1] joint 'ArmR': pose 'Smash' sets lean (first value) = 140, outside -90..120
- **`RevolveError`**: [1:1] revolve 'Turn': 'Halo' cannot revolve `lean` — a revolve turns `twist` (about the mate's normal: a gear on its plate, a halo) or `pitch` (about the socket's x axis: a wheel on its edge)
- **`AimError`**: [1:1] aim of 'ArmR': 'Hamer' is neither a direction (north, south, east, west, up, down) nor a part of 'Smith' — parts: ArmR, Body, Hammer; or a point `aim=[x, y, z]`
- **`BreakError`**: [1:1] breakable 'Body': is not placed by a mate (it is the root of 'Demon'), so there is nothing to break it from — parts that can break in 'Demon': Head, EyeR, EyeL, ArmR
- **`ImportError`**: [1:1] import: "lib/flame.md" has no thing 'Flme' — things: Flame, Ember; materials: Ash, Coal (import a material with `material Ash`)

## Reserved keywords

atom, legend, voxel, translate, merge, print, thing, entity, part, relation, constraint, shape, material, generator, world, refine, detail, biome, terrain, water, resolve, scatter, over, where, avoid, parts, on, box, sphere, cylinder, cone, ellipsoid, blob, heightfield, shell, extrude, capsule, torus, inside, outside, adjacent_to, above, below, left_of, right_of, in_front_of, behind, symmetric_across, attached_to, touch, surrounds, and, or, not, let, if, else, fn, for, in


