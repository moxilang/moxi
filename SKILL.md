# Moxi v2 — SKILL.md

A prompt guide for LLMs generating Moxi scripts. Read this before generating any Moxi source.

---

## What Moxi is

Moxi is a semantic spatial description language that compiles to voxels. Scripts are `.md` files. `#` headings and `>` blockquotes are ignored by the compiler as comments. Everything else is compiled source.

The LLM works at the semantic layer. The compiler handles geometry. Never think in coordinates. Never hand-place voxels. Describe what things *are* and how they *relate* — the compiler does the rest.

---

## Script format

Every Moxi script is a `.md` file. `#` and `>` lines are comments. Code is bare.

```md
# This heading is ignored by the compiler
> This blockquote is ignored by the compiler

atom BONE { color = ivory }   ← this line is compiled
```

---

## Declaration order

Always follow this order. Forward references do not work.

1. `atom` declarations
2. `material` declarations
3. `entity` declarations (with parts, relations, constraints, resolve)
4. `generator` declarations
5. `print` statements

---

## Atoms

The lowest-level semantic unit. Every material references one via `voxel_atom`.

```
atom BONE    { color = ivory }
atom MUSCLE  { color = red }
atom LEAF    { color = green }
```

Built-in color names: `red` `orange` `yellow` `green` `blue` `purple` `white` `black` `gray` `grey` `brown` `ivory` `maroon` `peach` `mochi-pink`

---

## Materials

Bind atoms to semantic surface descriptions.

```
material Bone   { color = ivory, voxel_atom = BONE }
material Muscle { color = red,   voxel_atom = MUSCLE }
```

Both `color` and `voxel_atom` are required.

---

## Entities

Named objects built from parts. Parts have a shape and a material. Relations position them relative to each other.

```
entity Skeleton {
    part Skull   { shape = sphere(radius=4),                                    material = Bone }
    part Spine   { shape = cylinder(height=24, radius=0.8),                     material = Bone }
    part Ribcage { shape = shell(ellipsoid(rx=8, ry=10, rz=6), inner_offset=1), material = Bone }
    part Pelvis  { shape = ellipsoid(rx=7, ry=4, rz=5),                         material = Bone }

    relation {
        Spine   above    Pelvis
        Ribcage surrounds Spine
        Skull   above    Ribcage
    }

    constraint Skull above Ribcage

    resolve voxel_size = 1.0
}
```

`resolve voxel_size` is required on every entity.

---

## Shape primitives

Arguments are always named (`key=value`).

| Shape | Arguments |
|-------|-----------|
| `sphere` | `radius` |
| `cylinder` | `height`, `radius` |
| `box` | `width`, `height`, `depth` |
| `cone` | `height`, `radius` |
| `ellipsoid` | `rx`, `ry`, `rz` |
| `blob` | `radius`, `roughness` — organic noise-perturbed sphere |
| `heightfield` | `seed`, `radius`, `noise`, `max_height` — terrain surface |
| `shell` | `inner_shape`, `inner_offset` — hollow version of any shape |
| `extrude` | `profile_shape`, `height` — 2D profile extruded upward |

---

## Relations

Spatial relationships between parts. The compiler resolves these into world-space offsets. Never write coordinates manually.

| Keyword | Effect |
|---------|--------|
| `above` | subject base sits at object top |
| `below` | subject top sits at object base |
| `inside` | subject center aligns with object center |
| `surrounds` | subject center aligns with object center, wraps around |
| `adjacent_to` | subject touches object, centered |
| `left_of` | subject placed left of object bounding box |
| `right_of` | subject placed right of object bounding box |
| `in_front_of` | subject placed in front of object |
| `behind` | subject placed behind object |
| `attached_to` | subject placed above object, centered |
| `touch` | surface contact |
| `symmetric_across` | mirror across axis |

---

## Generators

Scatter entities over the primary terrain (first entity with a heightfield part).

```
generator ForestGen {
    scatter PalmTree
    count       = 60
    min_spacing = 5
    seed        = 7
    where       = elevation > 3 and elevation < 13
}
```

`where` condition variables: `elevation`, `slope`, `x`, `z`

`count`, `min_spacing`, and `seed` are all required for deterministic output.

---

## Print statements

Control what gets rendered and in what order. For layered worlds, print bottom to top — each layer overwrites the one below it.

```
print Ocean       detail=low
print SandBase    detail=low
print SoilTerrain detail=low
print RockyPeaks  detail=low
print PalmTree    detail=low
```

Generator target entities do not need a print statement to appear in the world — they are placed by the generator. But they must still be declared as entities.

---

## Rules — never break these

**1. Declaration order is strict.** Atoms before materials. Materials before entities. Always.

**2. Prose lines in scripts must start with `>` or `#`.** A bare line that is not valid Moxi syntax causes a parse error.

**3. ASCII only in code lines.** No em-dashes, no smart quotes, no unicode in declarations.

**4. Use `cylinder` not `heightfield` for flat uniform layers.** Heightfield with `noise > 0` produces ragged edges that vary between runs due to floating point rounding, even with the same seed. Ocean, sand, floors — always `cylinder` or `box`.

**5. Use the same seed for terrain layers that should align spatially.** If `RockyPeaks` and `SoilTerrain` use the same seed, their noise patterns match and rocks appear at the actual soil peaks.

**6. Layer order in print statements is render order.** Print bottom to top: ocean → sand → soil → rock → trees.

**7. Relation chains need bottom-up ordering.** Declare the anchor first, then parts relative to it. Spine is anchor. Ribcage surrounds Spine. Skull above Ribcage.

**8. `resolve voxel_size` is required on every entity.**

**9. Never hand-place more than ~10 objects.** Use generators for repetition.

**10. Never write coordinates.** If you find yourself thinking about `(x, y, z)` positions, use a relation instead.

---

## Minimal complete example

```md
# My Boulder
> A simple rock formation.

atom STONE   { color = gray }
material Stone { color = gray, voxel_atom = STONE }

entity Boulder {
    part Body { shape = blob(radius=5, roughness=0.4), material = Stone }
    resolve voxel_size = 1.0
}

print Boulder detail=low
```

---

For a complete world example see `scripts/ISLAND.md`.
For a complete anatomy example see `scripts/SKELETON.md`.