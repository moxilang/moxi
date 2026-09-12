> **Generated file.** Everything below the first `---` is hand-written prose from `docs/skill_preamble.md`. Everything after the second `---` — the Generated Reference — is rendered by `moxi skill` from the compiler's own tables in `src/spec.rs`. Do not hand-edit the reference section; run `moxi skill > SKILL.md` after a language change, and `moxi skill --check` to verify it's current (CI does this).

# Moxi — SKILL.md

A prompt guide for LLMs generating Moxi scripts. Read this before generating
any Moxi source.

## What Moxi is

Moxi is a semantic spatial description language that compiles to voxels.
Scripts are `.md` files. **Code lives inside ` ```moxi ` fences; everything
outside a fence is prose and is ignored.** Write explanation as normal
Markdown — headings, paragraphs, tables — and put every declaration in a
fence.

Work at the semantic layer. Describe what things *are*, how they *attach*, and
what must *hold*. **Never write absolute coordinates.** Every number in Moxi
is relative to a named frame — a shape's own origin, an anchor, a socket. If
you are computing a world-space `(x, y, z)`, you are using the language wrong.

## File shape

````md
# Palm tree

A trunk with a rough crown mated to its top.

```moxi
material Bark  { color = brown }
material Leafy { color = green }

thing PalmTree {
    part Trunk { shape = cylinder(height=6, radius=0.6), material = Bark }
    part Crown { shape = blob(radius=3, roughness=0.4), material = Leafy }
    relation { Crown.bottom on Trunk.top gap=-1 }
    resolve voxel_size = 1.0
}

print PalmTree detail=low
```
````

Multiple fences in one file concatenate in document order, so a long script
can be interleaved with prose that explains each stage. Fences tagged with any
other language (` ```rust `, ` ```text `) are prose and are not compiled.

## Declaration order

Forward references do not work. Declare in this order:

1. `material`
2. `thing` — and a thing must be declared **before** any thing that
   instances it
3. `generator`
4. `print`

## Materials

A material is self-contained. Give it a color and use it:

```moxi
material Bone { color = ivory }
```

Colors: `red` `orange` `yellow` `green` `blue` `purple` `white` `black`
`gray` `grey` `brown` `ivory` `maroon` `peach` `mochi-pink`, or a hex string
`"#c96f4a"`.

`atom` still exists, and a material may point at one with `voxel_atom = NAME`
when several materials must share a single atom. You almost never need this —
prefer the one-line form above, and reach for `atom` only when the sharing is
the point.

## Things and parts

A `thing` is the unit of the language: something that exists in space and is
made of parts. Parts are shapes, or other things.

```moxi
thing Skeleton {
    part Skull { shape = sphere(radius=4),                material = Bone }
    part Spine { shape = cylinder(height=24, radius=0.8), material = Bone }

    relation {
        Skull above Spine
    }

    constraint Skull above Spine
    resolve voxel_size = 1.0
}
```

A part is **either** a shape **or** an instance of another thing — never both.

## Shapes and CSG

The full shape table, argument list, and anchor vocabulary for each shape are
generated below in the Generated Reference. What matters here is composition:
every shape is a containment predicate, so `union`, `intersect`, `difference`,
`at`, and `spin` nest arbitrarily and combine with everything else. Use them
whenever a form is one *object* rather than an assembly — a mug body, a gear,
an arch:

```moxi
part Body {
    shape = difference(
        cylinder(height=10, radius=5),
        at(cylinder(height=10, radius=4), y=1)
    ),
    material = Ceramic
}
```

A CSG shape's anchors follow its **first** operand (the base, for
`difference`), transformed through any `at` / `spin`.

**Blend joins.** `union(a, b, blend=k)` fillets the seam with a curve
about `k` units wide instead of leaving a crease. It is how two spheres
become a shoulder, or a trunk flows into a branch. Use it for anything
organic; leave it off for mechanical parts that should meet at an edge.

```moxi
shape = union(sphere(radius=5), at(sphere(radius=3), y=6), blend=2.5)
```

## Sculptor primitives

Three more shapes for organic and curved forms, alongside the CAD set:

- `capsule(height=, radius=)` — a sphere-swept limb. Base at origin, axis
  +Y; the straight segment is `height` long, and rounded caps of `radius`
  extend beyond each end. A better bone or finger than `cylinder`.
- `torus(major_radius=, minor_radius=)` — a ring, centered, lying in the
  XZ plane. Anchors: `outer(angle)`, `inner(angle)`, `top(angle)`,
  `bottom(angle)`, and the general `surface(angle, phi)`.
- `box(..., round=)` — an existing box with its corners filleted by
  `round` world units. `round=0` (the default) is the sharp box exactly.

```moxi
part Arm { shape = capsule(height=8, radius=1.2), material = Skin }
part Ring { shape = torus(major_radius=3, minor_radius=0.6), material = Gold }
part Crate { shape = box(width=4, height=4, depth=4, round=0.4), material = Wood }
```

Not yet available: `lathe` (revolve a profile) and `sweep` (extrude a
profile along a path) — both need a profile as an ordered list of points,
and lists are not a value type yet.

## Anchors

Every anchor is a named frame on a shape: a position plus an outward normal.
Placement mates two anchors together. Universal anchors — `center`, `top`,
`bottom`, `north`, `south`, `east`, `west`, `point(...)` — exist on every
shape from its analytic bounding box. Shape-specific anchors refine these with
true surface geometry; the full per-shape list is in the Generated Reference.

If you name an anchor that does not exist, the compiler replies with the
complete list of valid anchors for that shape. Read it and pick from it.

## Placement

Two forms, both inside `relation { … }`:

**Explicit mate** — precise, with optional qualifiers:

```moxi
Subject.anchor on Object.anchor  twist=  pitch=  gap=  shift=(a, b)
```

The two anchors coincide and their normals oppose. Arbitrary angles are legal
and realize exactly.

**`shift` puts several features on one surface.** It slides the mate within
the socket's tangent plane, `(along socket +X, along socket +Z)`, in world
units. `gap` is the same translation along the normal. On a sphere's `north`
the first component runs up the shape and the second runs across it, so two
eyes are:

```moxi
LeftEye.south  on Head.north shift=(1, -2)
RightEye.south on Head.north shift=(1,  2)
```

Without `shift`, every mate lands dead-center on its socket.

**Relation keywords** are sugar for a default anchor pair — an explicit
anchor on either side overrides that side's default. The full sugar table is
in the Generated Reference. Keywords like `left_of` arrange *separate*
objects beside each other; for a feature *on* a surface, use an explicit mate
with `shift`.

`center` is orientation-free: the subject inherits the object's rotation
instead of flipping to face it.

**Mirroring**:

```moxi
LeftArm symmetric_across Spine from=RightArm
```

Reflects the **solved** frame of `from=` across a plane through the named
part's anchor. `axis=x` (default) is bilateral left/right symmetry.

**The one hard rule**: each part may be the subject of **at most one**
placement. Cycles and double-placements are compile errors.

## Composition — things instance things

```moxi
thing Arm {
    part Humerus { shape = cylinder(height=9, radius=0.8), material = Bone }
    part Forearm { shape = cylinder(height=8, radius=0.7), material = Bone }
    relation { Forearm.top on Humerus.bottom }
    anchor socket = Humerus.top
    resolve voxel_size = 1.0
}

thing Skeleton {
    part RightArm { thing = Arm }
    part LeftArm  { thing = Arm }
    relation {
        RightArm.socket on Ribcage.east twist=-90 pitch=70 gap=1
        LeftArm symmetric_across Spine from=RightArm
    }
    resolve voxel_size = 1.0
}
```

Instances flatten with prefixed names (`RightArm.Humerus`), so nesting works
to any depth. **Instances answer the compass for free** — `Middle.west on
Left.east` works with no exports declared, resolved against the instance's
whole solved assembly bounding box. Declare `anchor NAME = Part.anchor` only
when you need a socket the box cannot express.

Rules that produce errors: the template thing must be declared before it is
instanced; when an instance is the *subject* of a placement, the anchor
gripping it must be on the instance's root part; mirroring instance-to-instance
requires both sides to instance the same thing.

## Parameters

```moxi
thing PalmTree(height=6, crown=3) {
    part Trunk { shape = cylinder(height=height, radius=crown*0.2), material = Bark }
    part Crown { shape = blob(radius=crown, roughness=0.4), material = Leafy }
    relation { Crown.bottom on Trunk.top gap=-1 }
    resolve voxel_size = 1.0
}

part Tall  { thing = PalmTree(height=10, crown=4) }
part Mid   { thing = PalmTree }
```

Defaults are **required**. Arithmetic folds at compile time. Instance
arguments must be constants. Compass anchors reflect each instance's *actual*
size.

## Values — `let` and `if`

Name a computed value once and use it everywhere in the thing:

```moxi
thing Gear(teeth=12, radius=6) {
    let pitch = 360 / teeth
    let rim   = if teeth > 10 { radius * 2 } else { radius }
    part Disc { shape = cylinder(height=2, radius=rim), material = Steel }
    resolve voxel_size = 1.0
}
```

`let` bindings see the parameters and every earlier `let`, never a later
one. They are re-evaluated per instance, so `Gear(teeth=6)` gets its own
`pitch`. `if` is an *expression* — it produces a value, `else` is mandatory,
and only the taken branch is evaluated. Comparisons and `and` / `or` / `not`
produce booleans; arithmetic produces numbers.

A name that is not defined is an error listing what *is* in scope. There is
no default to fall back to.

Not yet: `let` in `twist` / `pitch` / `gap` / `shift`, lists, strings.

## Constraints

Checked against solved geometry, with half a voxel of tolerance. A violation
aborts compilation with expected vs actual numbers.

```moxi
constraint Skull above Ribcage
```

Enforced today: `above`, `below`, `inside`, `surrounds`.

## Generators

Scatter a thing over the primary terrain (the first thing containing a
`heightfield` part):

```moxi
generator ForestGen {
    scatter PalmTree
    count       = 60
    min_spacing = 5
    seed        = 7
    where       = elevation > 3 and elevation < 13
}
```

`where` variables: `elevation`, `slope`, `x`, `z`, `depth`. Combine with `and`,
`or`, `not`. `count`, `min_spacing`, and `seed` are all required for
deterministic output.

## Scenes — a scene is a thing

Build a scene the way you build anything else: one thing whose parts are
other things, placed by relation.

```moxi
thing World {
    part Sea  { thing = Ocean }
    part Land { thing = Terrain }
    part Hut  { thing = Cabin }
    relation {
        Land.bottom on Sea.top
        Hut.bottom  on Land.ground(x=12, z=4)
    }
    resolve voxel_size = 1.0
}

print World detail=low
```

Instances answer the compass (`Sea.top`, `Land.bottom`) from their solved
bounding box. To site something at a *point* on terrain, export the
surface anchor from the terrain thing — `anchor ground = Ground.surface` —
and pass coordinates through it: `Land.ground(x=12, z=4)`. The subject
takes the terrain's normal there, so it sits on the slope.

Generators scatter over the printed world's top surface. `elevation`, `x`
and `z` are world coordinates: the height and position of the top voxel
in that column, so a stacked ocean and beach are excluded simply by being
low. Scattered instances become parts of the world named
`<generator>.<index>.<part>` — they are in the scene, and the viewer draws
them.

## Print

Print the world. One `print` per scene:

```moxi
print World detail=low
```

Printing several things is still allowed, but they are not placed relative
to each other — each is centered at the origin and stacked by a viewer
convention. Prefer one world thing. Things used as instance templates or
generator targets are never printed as layers.

## Rules — never break these

1. **Declaration order is strict**, and a template thing must precede every
   thing that instances it.
2. **All code goes inside ` ```moxi ` fences.** Anything outside a fence is
   prose and is silently ignored — a declaration written outside one simply
   does not exist. Close every fence you open.
3. **ASCII only in code lines.** No em-dashes, no smart quotes.
4. **Never write absolute coordinates.** Use an anchor, and `shift` to move
   along it. `point()` exists as an escape hatch — reach for it last.
5. **`resolve voxel_size` on every thing.**
6. **Prefer `cylinder` or `box` to `heightfield` for flat uniform layers.**
   Heightfield noise gives ragged, run-varying edges. Ocean, sand, floors:
   never heightfield.
7. **Same seed for terrain layers that must align spatially.**
8. **A scene is a thing.** Place things relative to each other with
   relations inside one world thing, then print that. Do not stack
   separate prints and expect them to align.
9. **Repetition means a thing plus instances or a generator**, not fifty
   hand-written parts.
10. **One placement per part.** If a part needs two constraints, one of them
    is a `constraint`, not a `relation`.
11. **Features on a surface use `shift`, not `left_of`/`right_of`.** The
    relation keywords place separate objects side by side; they put two eyes
    on opposite temples.
12. **Read the error.** Unknown anchors and unknown parameters come back with
    the complete valid vocabulary. The fix is almost always in the message.

## Worked example — attaching with anchors

````md
# Mug

A hollow body is a difference, not a special shape. The handle mates to a
point on the body's side, found by anchor rather than by coordinate.

```moxi
material Ceramic { color = "#c96f4a" }

thing Mug {
    part Body {
        shape = difference(
            cylinder(height=10, radius=5),
            at(cylinder(height=10, radius=4), y=1)
        ),
        material = Ceramic
    }
    part Handle {
        shape = difference(
            box(width=4, height=8, depth=2),
            at(box(width=4, height=4, depth=3), x=-1.5)
        ),
        material = Ceramic
    }
    relation {
        Handle.west on Body.side(t=0.55, angle=90)
    }
    resolve voxel_size = 1.0
}

print Mug detail=low
```
````

## Where to look next

| Script | Shows |
|---|---|
| `scripts/ISLAND.md` | terrain layering, generators, determinism notes |
| `scripts/SKELETON_v2.md` | instancing, exported sockets, mirroring |
| `scripts/SKELETON_v3.md` | posed limbs from arbitrary angles |
| `scripts/MUG.md`, `scripts/AXLE.md` | CSG: difference, union, at, spin |
| `scripts/TREES.md` | thing parameters |
| `scripts/GROVE_v2.md` | instance compass anchors, zero exports |

---

# Generated Reference

Emitted by `moxi skill` from `src/spec.rs` — version `0.3.0`.

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

## Relation keyword sugar

| Keyword | Subject anchor | Object anchor |
|---|---|---|
| `above` | `bottom` | `top` |
| `below` | `top` | `bottom` |
| `left_of` | `east` | `west` |
| `right_of` | `west` | `east` |
| `in_front_of` | `north` | `south` |
| `behind` | `south` | `north` |
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

- **`axis`** — applies to: symmetric_across — default: `x`
- **`from`** — applies to: symmetric_across — required
- **`gap`** — unit: world units — applies to: explicit `on` and every relation-keyword sugar form except symmetric_across
- **`pitch`** — unit: degrees — applies to: explicit `on` and every relation-keyword sugar form except symmetric_across
- **`shift`** — unit: world units — applies to: explicit `on` and every relation-keyword sugar form except symmetric_across — a pair `(along socket +X, along socket +Z)` sliding the mate within the socket's tangent plane; `gap` is the same translation's +Y component
- **`twist`** — unit: degrees — applies to: explicit `on` and every relation-keyword sugar form except symmetric_across

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
- **`InstanceError`**: [1:1] instance 'RightArm': thing 'Arm' must be declared before it is instanced
- **`ExprError`**: [1:1] 'lenth' is not defined — in scope: girth, length

## Reserved keywords

atom, legend, voxel, translate, merge, print, thing, entity, part, relation, constraint, shape, material, generator, world, refine, detail, biome, terrain, water, resolve, scatter, over, where, avoid, parts, on, box, sphere, cylinder, cone, ellipsoid, blob, heightfield, shell, extrude, capsule, torus, inside, outside, adjacent_to, above, below, left_of, right_of, in_front_of, behind, symmetric_across, attached_to, touch, surrounds, and, or, not, let, if, else


