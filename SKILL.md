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
what must *hold*. **Never write coordinates.** If you are computing an `(x, y,
z)` position, you are using the language wrong — there is an anchor for it.

## File shape

````md
# Palm tree

A trunk with a rough crown mated to its top.

```moxi
material Bark  { color = brown }
material Leafy { color = green }

entity PalmTree {
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
2. `entity` — and an entity must be declared **before** any entity that
   instances it
3. `generator`
4. `print`

## Materials

A material is self-contained. Give it a color and use it:

````moxi
material Bone { color = ivory }
````

Colors: `red` `orange` `yellow` `green` `blue` `purple` `white` `black`
`gray` `grey` `brown` `ivory` `maroon` `peach` `mochi-pink`, or a hex string
`"#c96f4a"`.

`atom` still exists, and a material may point at one with `voxel_atom = NAME`
when several materials must share a single atom. You almost never need this —
prefer the one-line form above, and reach for `atom` only when the sharing is
the point.

## Entities and parts

````moxi
entity Skeleton {
    part Skull { shape = sphere(radius=4),                material = Bone }
    part Spine { shape = cylinder(height=24, radius=0.8), material = Bone }

    relation {
        Skull above Spine
    }

    constraint Skull above Spine
    resolve voxel_size = 1.0
}
````

A part is **either** a shape **or** an instance of another entity — never both.

## Shapes and CSG

The full shape table, argument list, and anchor vocabulary for each shape are
generated below in the Generated Reference. What matters here is composition:
every shape is a containment predicate, so `union`, `intersect`, `difference`,
`at`, and `spin` nest arbitrarily and combine with everything else. Use them
whenever a form is one *object* rather than an assembly — a mug body, a gear,
an arch:

````moxi
part Body {
    shape = difference(
        cylinder(height=10, radius=5),
        at(cylinder(height=10, radius=4), y=1)
    ),
    material = Ceramic
}
````

A CSG shape's anchors follow its **first** operand (the base, for
`difference`), transformed through any `at` / `spin`.

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

````moxi
Subject.anchor on Object.anchor  twist=  pitch=  gap=
````

The two anchors coincide and their normals oppose. Arbitrary angles are legal
and realize exactly.

**Relation keywords** are sugar for a default anchor pair — an explicit
anchor on either side overrides that side's default. The full sugar table is
in the Generated Reference.

`center` is orientation-free: the subject inherits the object's rotation
instead of flipping to face it.

**Mirroring**:

````moxi
LeftArm symmetric_across Spine from=RightArm
````

Reflects the **solved** frame of `from=` across a plane through the named
part's anchor. `axis=x` (default) is bilateral left/right symmetry.

**The one hard rule**: each part may be the subject of **at most one**
placement. Cycles and double-placements are compile errors.

## Composition — entities instance entities

````moxi
entity Arm {
    part Humerus { shape = cylinder(height=9, radius=0.8), material = Bone }
    part Forearm { shape = cylinder(height=8, radius=0.7), material = Bone }
    relation { Forearm.top on Humerus.bottom }
    anchor socket = Humerus.top
    resolve voxel_size = 1.0
}

entity Skeleton {
    part RightArm { entity = Arm }
    part LeftArm  { entity = Arm }
    relation {
        RightArm.socket on Ribcage.east twist=-90 pitch=70 gap=1
        LeftArm symmetric_across Spine from=RightArm
    }
    resolve voxel_size = 1.0
}
````

Instances flatten with prefixed names (`RightArm.Humerus`), so nesting works
to any depth. **Instances answer the compass for free** — `Middle.west on
Left.east` works with no exports declared, resolved against the instance's
whole solved assembly bounding box. Declare `anchor NAME = Part.anchor` only
when you need a socket the box cannot express.

Rules that produce errors: the template entity must be declared before it is
instanced; when an instance is the *subject* of a placement, the anchor
gripping it must be on the instance's root part; mirroring instance-to-instance
requires both sides to instance the same entity.

## Parameters

````moxi
entity PalmTree(height=6, crown=3) {
    part Trunk { shape = cylinder(height=height, radius=crown*0.2), material = Bark }
    part Crown { shape = blob(radius=crown, roughness=0.4), material = Leafy }
    relation { Crown.bottom on Trunk.top gap=-1 }
    resolve voxel_size = 1.0
}

part Tall  { entity = PalmTree(height=10, crown=4) }
part Mid   { entity = PalmTree }
````

Defaults are **required**. Arithmetic folds at compile time. Instance
arguments must be constants. Compass anchors reflect each instance's *actual*
size.

## Constraints

Checked against solved geometry, with half a voxel of tolerance. A violation
aborts compilation with expected vs actual numbers.

````moxi
constraint Skull above Ribcage
````

Enforced today: `above`, `below`, `inside`, `surrounds`.

## Generators

Scatter an entity over the primary terrain (the first entity containing a
`heightfield` part):

````moxi
generator ForestGen {
    scatter PalmTree
    count       = 60
    min_spacing = 5
    seed        = 7
    where       = elevation > 3 and elevation < 13
}
````

`where` variables: `elevation`, `slope`, `x`, `z`, `depth`. Combine with `and`,
`or`, `not`. `count`, `min_spacing`, and `seed` are all required for
deterministic output.

## Print

Render order, bottom layer first — each overwrites the one below:

````moxi
print Ocean       detail=low
print SandBase    detail=low
print SoilTerrain detail=low
````

Entities used as instance templates or generator targets are **not** printed
as layers.

## Rules — never break these

1. **Declaration order is strict**, and a template entity must precede every
   entity that instances it.
2. **All code goes inside ` ```moxi ` fences.** Anything outside a fence is
   prose and is silently ignored — a declaration written outside one simply
   does not exist. Close every fence you open.
3. **ASCII only in code lines.** No em-dashes, no smart quotes.
4. **Never write coordinates.** Use an anchor. `point()` exists as an escape
   hatch — reach for it last, not first.
5. **`resolve voxel_size` on every entity.**
6. **Prefer `cylinder` or `box` to `heightfield` for flat uniform layers.**
   Heightfield noise gives ragged, run-varying edges. Ocean, sand, floors:
   never heightfield.
7. **Same seed for terrain layers that must align spatially.**
8. **Print order is render order**, bottom to top.
9. **Repetition means an entity plus instances or a generator**, not fifty
   hand-written parts.
10. **One placement per part.** If a part needs two constraints, one of them
    is a `constraint`, not a `relation`.
11. **Read the error.** Unknown anchors and unknown parameters come back with
    the complete valid vocabulary. The fix is almost always in the message.

## Worked example — attaching with anchors

````md
# Mug

A hollow body is a difference, not a special shape. The handle mates to a
point on the body's side, found by anchor rather than by coordinate.

```moxi
material Ceramic { color = "#c96f4a" }

entity Mug {
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
| `scripts/TREES.md` | entity parameters |
| `scripts/GROVE_v2.md` | instance compass anchors, zero exports |

---

# Generated Reference

Emitted by `moxi skill` from `src/spec.rs` — version `0.3.0`.

## Shapes

| Shape | Arguments | Local origin |
|---|---|---|
| `sphere` | `radius` (f64, default 1) | centered |
| `cylinder` | `height` (f64, default 1), `radius` (f64, default 0.5) | base at origin, axis +Y |
| `box` | `width` (f64, default 2), `height` (f64, default 2), `depth` (f64, default 2) | centered |
| `cone` | `height` (f64, default 1), `radius` (f64, default 0.5) | base at origin, apex +Y |
| `ellipsoid` | `rx` (f64, default 1), `ry` (f64, default 1), `rz` (f64, default 1) | centered |
| `blob` | `radius` (f64, default 1), `roughness` (f64, default 0.2) | centered (nominal sphere; noise never perturbs anchors) |
| `heightfield` | `seed` (i64, default 42), `radius` (f64, default 50), `noise` (f64, default 0.3), `max_height` (f64, default 20) | base at origin |
| `shell` | `inner_offset` (f64, default 1) | same as its inner shape |
| `extrude` | `height` (f64, default 1) | base of profile at origin, extruded +Y |
| `union` | — | delegates to the first operand |
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
- **`InstanceError`**: [1:1] instance 'RightArm': entity 'Arm' must be declared before it is instanced

## Reserved keywords

atom, legend, voxel, translate, merge, print, entity, part, relation, constraint, shape, material, generator, world, refine, detail, biome, terrain, water, resolve, scatter, over, where, avoid, parts, on, box, sphere, cylinder, cone, ellipsoid, blob, heightfield, shell, extrude, inside, outside, adjacent_to, above, below, left_of, right_of, in_front_of, behind, symmetric_across, attached_to, touch, surrounds, and, or, not


