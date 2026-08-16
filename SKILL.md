# Moxi — SKILL.md

A prompt guide for LLMs generating Moxi scripts. Read this before generating
any Moxi source.

> **Maintenance note.** Once issue M2 lands, this file becomes *generated
> output* (`moxi skill > SKILL.md`) and the prose sections below move to
> `docs/skill_preamble.md`. Until then it is hand-maintained and must be
> updated whenever the language changes. Covers the compiler through Phase C.

---

## What Moxi is

Moxi is a semantic spatial description language that compiles to voxels.
Scripts are `.md` files: `#` headings and `>` blockquotes are ignored as
comments, everything else is compiled.

Work at the semantic layer. Describe what things *are*, how they *attach*, and
what must *hold*. **Never write coordinates.** If you are computing an `(x, y,
z)` position, you are using the language wrong — there is an anchor for it.

---

## Declaration order

Forward references do not work. Declare in this order:

1. `atom`
2. `material`
3. `entity` — and an entity must be declared **before** any entity that
   instances it
4. `generator`
5. `print`

---

## Atoms and materials

```
atom BONE { color = ivory }

material Bone { color = ivory, voxel_atom = BONE }
```

Both `color` and `voxel_atom` are required on a material. Colors: `red`
`orange` `yellow` `green` `blue` `purple` `white` `black` `gray` `grey`
`brown` `ivory` `maroon` `peach` `mochi-pink`, or a hex string `"#c96f4a"`.

---

## Entities and parts

```
entity Skeleton {
    part Skull { shape = sphere(radius=4),                material = Bone }
    part Spine { shape = cylinder(height=24, radius=0.8), material = Bone }

    relation {
        Skull above Spine
    }

    constraint Skull above Spine
    resolve voxel_size = 1.0
}
```

A part is **either** a shape **or** an instance of another entity — never both.

---

## Shapes

| Shape | Arguments | Local origin |
|---|---|---|
| `sphere` | `radius` | centered |
| `ellipsoid` | `rx`, `ry`, `rz` | centered |
| `box` | `width`, `height`, `depth` | centered |
| `cylinder` | `height`, `radius` | base at origin, axis +Y |
| `cone` | `height`, `radius` | base at origin, apex +Y |
| `blob` | `radius`, `roughness` | centered — noise-perturbed sphere |
| `heightfield` | `seed`, `radius`, `noise`, `max_height` | base at origin |
| `shell(inner, inner_offset=)` | a hollow version of a **primitive** | as inner |
| `extrude(profile, height=)` | 2D profile extruded up +Y | base at origin |

### CSG — shapes compose

```
union(a, b, …)                filled where ANY child is
intersect(a, b, …)            filled where ALL children are
difference(base, cut, …)      base minus every cut  (needs ≥1 cut)
at(shape, x=, y=, z=)         translate a child in local space
spin(shape, axis=x|y|z, degrees=)   rotate a child about its local origin
```

All five nest arbitrarily. Use them whenever a form is one *object* rather
than an assembly — a mug body, a gear, an arch, a wheel-and-axle:

```
part Body {
    shape = difference(
        cylinder(height=10, radius=5),
        at(cylinder(height=10, radius=4), y=1)
    ),
    material = Ceramic
}
```

Notes: a CSG shape's anchors follow its **first** operand (the base, for
`difference`), transformed through any `at` / `spin`. `shell(...)` of a CSG
shape does not hollow — write an explicit `difference` instead.

---

## Anchors — the attachment vocabulary

Every anchor is a named frame on a shape: a position plus an outward normal.
Placement mates two anchors together.

**Universal, on every shape** (from its analytic bounding box):

`center` (orientation-free) · `top` · `bottom` · `north` (+Z) · `south` (−Z) ·
`east` (+X) · `west` (−X) · `point(x=, y=, z=, nx=, ny=, nz=)` — an explicit
part-local frame; add `free=1` to drop the orientation.

**Shape-specific**, which refine the universal ones:

| Shape | Extra anchors |
|---|---|
| `sphere`, `ellipsoid`, `blob` | `surface(yaw=, pitch=)` — degrees; yaw about +Y from +Z, pitch from equator toward +Y |
| `cylinder` | `side(t=, angle=)` — `t ∈ [0,1]` along the axis, angle in degrees; `rim_top(angle=)`, `rim_bottom(angle=)` |
| `cone` | `apex`, `base`, `side(t=, angle=)` |
| `heightfield` | `surface(x=, z=)` — the terrain point at that column, with the true terrain normal |

If you name an anchor that does not exist, the compiler replies with the full
list of valid anchors for that shape. Read it and pick from it.

---

## Placement

Two forms. Both live inside `relation { … }`.

### Explicit mate — the precise form

```
Subject.anchor on Object.anchor  twist=  pitch=  gap=
```

The two anchors coincide and their normals oppose. Qualifiers, all optional:

- `twist=` — degrees about the socket normal
- `pitch=` — degrees of tilt off the socket normal
- `gap=` — world units along the normal; `0` = touching, negative = overlap

Arbitrary angles are legal and realize exactly.

```
RightArm.socket on Ribcage.east twist=-90 pitch=70 gap=1
Handle.west     on Body.side(t=0.55, angle=90)
Crown.bottom    on Trunk.top gap=-1
```

### Relation keywords — sugar for common mates

Each keyword just names a default anchor pair. An explicit anchor on either
side overrides that side's default.

| Keyword | Subject anchor | Object anchor |
|---|---|---|
| `above` | `bottom` | `top` |
| `below` | `top` | `bottom` |
| `left_of` | `east` | `west` |
| `right_of` | `west` | `east` |
| `in_front_of` | `north` | `south` |
| `behind` | `south` | `north` |
| `inside`, `surrounds` | `center` | `center` |
| `outside` | `west` | `east` |
| `touch`, `adjacent_to`, `attached_to` | `bottom` | `top` |

`center` is orientation-free: the subject inherits the object's rotation
instead of flipping to face it. That is why `Ribcage surrounds Spine` keeps
the ribcage upright with the spine.

### Mirroring

```
LeftArm symmetric_across Spine from=RightArm
```

Reflects the **solved** frame of `from=` across a plane through the named
part's anchor. `axis=x` (default) is bilateral left/right symmetry. Nothing
else places `LeftArm` — it *is* the reflection.

### The one hard rule

Each part may be the subject of **at most one** placement. A part with no
placement is a root at the origin. Cycles and double-placements are compile
errors.

---

## Composition — entities instance entities

```
entity Arm {
    part Humerus { shape = cylinder(height=9, radius=0.8), material = Bone }
    part Forearm { shape = cylinder(height=8, radius=0.7), material = Bone }
    relation { Forearm.top on Humerus.bottom }
    anchor socket = Humerus.top          # exported: usable on instances
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
```

Instances flatten with prefixed names (`RightArm.Humerus`), so nesting works
to any depth — a Grove of PalmTrees inside an Orchard of Groves.

**Instances answer the compass for free.** `Middle.west on Left.east` works
with no exports declared: the compass resolves against the instance's whole
solved assembly bounding box. Declare `anchor NAME = Part.anchor` only when
you need a socket the box cannot express.

Rules that produce errors:

- **Declare before instance.** The template entity must appear earlier.
- **Root socket rule.** When an instance is the *subject* of a placement, the
  anchor gripping it must be on the instance's root part (the one with no
  internal placement). Export sockets from the root.
- **Mirror like-for-like.** `symmetric_across … from=` between instances
  requires both to instance the same entity.
- `material =` on an instance part is ignored; template parts carry their own.

---

## Parameters

```
entity PalmTree(height=6, crown=3) {
    part Trunk { shape = cylinder(height=height, radius=crown*0.2), material = Bark }
    part Crown { shape = blob(radius=crown, roughness=0.4), material = Leafy }
    relation { Crown.bottom on Trunk.top gap=-1 }
    resolve voxel_size = 1.0
}

part Tall  { entity = PalmTree(height=10, crown=4) }
part Mid   { entity = PalmTree }                      # defaults
```

Defaults are **required**. Arithmetic (`+ - * /`) folds at compile time.
Instance arguments must be constants — passing an enclosing entity's parameter
down into a nested instance is not yet supported and errors clearly.

Compass anchors reflect each instance's *actual* size: a `height=10` tree's
`east` is where that tree ends.

---

## Constraints

Checked against the solved geometry, with half a voxel of tolerance. A
violation aborts compilation with expected vs actual numbers.

```
constraint Skull above Ribcage
```

Enforced today: `above`, `below`, `inside`, `surrounds`.

---

## Generators

Scatter an entity over the primary terrain (the first entity containing a
`heightfield` part).

```
generator ForestGen {
    scatter PalmTree
    count       = 60
    min_spacing = 5
    seed        = 7
    where       = elevation > 3 and elevation < 13
}
```

`where` variables: `elevation`, `slope`, `x`, `z`, `depth`. Combine with
`and`, `or`, `not`. `count`, `min_spacing` and `seed` are all required for
deterministic output.

---

## Print

Render order, bottom layer first — each overwrites the one below.

```
print Ocean       detail=low
print SandBase    detail=low
print SoilTerrain detail=low
```

Entities used as instance templates or generator targets are **not** printed
as layers; their geometry appears wherever they were used.

---

## Rules — never break these

1. **Declaration order is strict**, and a template entity must precede every
   entity that instances it.
2. **Prose must start with `>` or `#`.** A bare non-Moxi line is a parse
   error.
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

---

## Worked example — attaching with anchors

```md
# Mug
> A hollow body is a difference, not a special shape. The handle mates to a
> point on the body's side, found by anchor rather than by coordinate.

atom CLAY { color = "#c96f4a" }
material Ceramic { color = "#c96f4a", voxel_atom = CLAY }

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

---

## Where to look next

| Script | Shows |
|---|---|
| `scripts/ISLAND.md` | terrain layering, generators, determinism notes |
| `scripts/SKELETON_v2.md` | instancing, exported sockets, mirroring |
| `scripts/SKELETON_v3.md` | posed limbs from arbitrary angles |
| `scripts/MUG.md`, `scripts/AXLE.md` | CSG: difference, union, at, spin |
| `scripts/TREES.md` | entity parameters |
| `scripts/GROVE_v2.md` | instance compass anchors, zero exports |