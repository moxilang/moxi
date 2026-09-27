<!-- LOGO -->
<p align="center">
  <img width="240" alt="Moxi" src="https://github.com/user-attachments/assets/2a189048-5069-4c54-8cf1-0b9d6c417ed1" />
</p>

<h1 align="center">Moxi</h1>

<p align="center">
  <strong>A compiler for structured 3D worlds.</strong><br/>
  From <strong>semantics → geometry → voxels / mesh / raymarch</strong><br/>
  Explicit for humans. Deterministic for machines. Total, so an LLM
  can never write an infinite loop.
</p>

> **This README is drifting again — check NOTES.md and the git log
> before trusting anything below past the "What Moxi Is" section.**
> Language surface (`thing`, `for`, `fn`, sculptor primitives) and the
> three-backend pipeline (voxel / mesh / raymarch) below are current as
> of the last full pass; anything about `entity`, `atom` as required,
> or a voxel-only pipeline is stale.

---

## What Moxi Is

Moxi is a spatial description language. You describe what things *are*,
how they *relate*, and what rules they must *satisfy* — the compiler
turns that into geometry. Nobody writes coordinates: every number is
relative to a named frame.

Scripts are plain Markdown; code lives inside ` ```moxi ` fences and
everything else is prose. One file is simultaneously readable
documentation and compilable source.

````md
# Skeleton
A skull sitting on a spine.

```moxi
material Bone { color = ivory }

thing Skeleton {
    part Skull { shape = sphere(radius=4),                material = Bone }
    part Spine { shape = cylinder(height=24, radius=0.8), material = Bone }

    relation {
        Skull above Spine
    }

    resolve voxel_size = 1.0
}

print Skeleton detail=low
```
````

---

## Contributing

- **[`ROADMAP.md`](ROADMAP.md)** — the original plan and rationale.
  Stale past Phase D; see NOTES.md for what actually shipped.
- **[`CLAUDE.md`](CLAUDE.md)** — the four non-negotiable rules, the
  human-review boundary, and PR discipline.
- **[`NOTES.md`](NOTES.md)** — working notes: known gaps with
  diagnoses, decisions with reasons, script-versioning convention.
  Read this before ROADMAP.md.
- **[`SKILL.md`](SKILL.md)** — the generated language reference,
  written for a language model. Do not hand-edit; it is rendered from
  `docs/skill_preamble.md` and `src/spec.rs` by `moxi skill`.

---

## Pipeline

```
script.md
  ↓  Lexer            characters → tokens; ```moxi fences mask prose
  ↓  Parser           tokens → AST; relation sugar desugars; for/fn parsed
  ↓  Resolver         names, instance flattening, parameter substitution,
                       fn-call expansion, for-loop unrolling, index folding
  ↓  Frame solver     placements → one exact world Frame per part (toposort)
  ↓  Constraints      declared rules checked against solved frames
  ↓  Scene IR         the canonical artifact: shapes + frames + colors,
                       NO voxels — every backend below reads this
       ├─ Voxel     — contains(shape, F⁻¹·p) per voxel → VoxelGrid
       ├─ Mesh      — distance(shape, p) → surface-nets triangle mesh (OBJ)
       └─ Raymarch  — distance(shape, p) → GLSL, a self-contained
                       WebGL2 page (`moxi web`), zero server cost
  ↓  Generators       scatter over the primary terrain, analytic elevation
  ↓  Export           OBJ + MTL · JSON · self-contained HTML · Bevy viewer
```

Every stage before geometry is analytic — anchors and extents come from
shape *parameters*, never from voxel data — which is why arbitrary
rotations realize exactly and why errors are static.

---

## Install

```bash
# CLI only
cargo install moxi

# With the 3D viewer (optional — feature-gated; `cargo test` does NOT
# build it, use `cargo build --features viewer`)
cargo install moxi --features viewer

# From source
git clone https://github.com/moxilang/moxi
cd moxi
cargo install --path . --features viewer
```

---

## Usage

```bash
moxi check scripts/ISLAND.md          # errors only, no output
moxi json  scripts/MUG.md             # voxel machine surface (JSON)
moxi scene scripts/MUG.md             # the canonical IR — shapes, frames, colors
moxi compile scripts/ISLAND.md        # → output/world.obj + .mtl (voxel cubes)
moxi mesh  scripts/MUG.md             # → smooth OBJ via surface nets
moxi web   scripts/RIBCAGE.md         # → self-contained HTML, GPU raymarch
moxi view  scripts/SKELETON.md        # 3D preview (needs --features viewer)
```

---

## Language

Full reference: **[`SKILL.md`](SKILL.md)**. The shape of it:

### Materials

```moxi
material Bone { color = ivory }
```

Self-contained. `atom` still exists for the rare case of two materials
sharing one voxel identity.

### Shapes

`sphere` · `cylinder` · `box(round=)` · `cone` · `ellipsoid` · `blob` ·
`heightfield` · `shell` · `extrude` · `capsule` · `torus`

Composed with CSG — every shape is both a containment predicate and a
signed distance function, closed under composition:

```moxi
union(a, b, …, blend=k)   intersect(a, b, …)   difference(base, cut, …)
at(shape, x=, y=, z=)     spin(shape, axis=, degrees=)
```

`blend > 0` fillets a union's seams instead of leaving a crease.

### Anchors

Named frames on a shape — a position and an outward normal. Universal
on every shape: `center`, `top`, `bottom`, `north`, `south`, `east`,
`west`, `point(…)`. Refined per shape: `surface(yaw, pitch)` on
spheres/ellipsoids, `side(t, angle)` on cylinders/capsules,
`surface(angle, phi)`/`outer`/`inner` on a torus, `surface(x, z)` on a
heightfield.

### Placement

```moxi
relation {
    Handle.center   on Body.side(t=0.55, angle=90)
    RightArm.socket on Ribcage.east twist=-90 pitch=70 gap=1
    LeftEye.south   on Head.north shift=(1.5, -2.6) gap=-0.9
}
```

`shift=(a, b)` slides a mate within its socket's tangent plane — the
only way to put two features on one flat-faced anchor. `pitch`/`twist`
convention differs between face anchors and `side`/`surface` anchors;
see SKILL.md's Placement section before posing a limb.

Each part is the subject of at most one placement. Cycles and
double-placements are hard errors with source spans.

### Values, functions, loops

```moxi
fn taper(i, n) = sin(180 * (i + 0.5) / n)

thing Ribcage(pairs=12) {
    part Spine { shape = capsule(height=20, radius=0.8), material = Bone }
    for i in 0..pairs {
        let reach = 2.5 + 3.5 * taper(i, pairs)
        part RibR[i] { thing = Rib(reach=reach) }
    }
    resolve voxel_size = 0.5
}
```

`let`/`if` fold at resolve time; `fn` is a pure one-expression function
resolved by substitution (no recursion); `for` unrolls at resolve time
into ordinary named parts (`RibR[0]` … `RibR[11]`) — nothing downstream
of the resolver knows a loop existed. The language stays total: no
`while`, no unbounded recursion, compile always terminates.

### Composition

```moxi
thing Skeleton {
    part RightArm { thing = Arm }
    part LeftArm  { thing = Arm }
}
```

Instances flatten with prefixed names, expose exported anchors
(`anchor socket = Humerus.top`), and answer the universal compass from
their solved assembly box with no exports required. A thing whose
structure depends on its own parameters (loops, indexed parts) is
re-resolved per distinct override and cached.

⚠ **Known bug**: `symmetric_across` mirrors a part's placed *frame*,
not its geometry — correct for locally symmetric parts (a capsule, a
sphere) and wrong for a chiral part whose origin sits on the mirror
plane (see NOTES.md). Fix in progress.

### Constraints, generators

```moxi
constraint Skull above Ribcage

generator ForestGen {
    scatter PalmTree
    count = 60, min_spacing = 5, seed = 7
    where = elevation > 3 and elevation < 13
}
```

Generators scatter over the printed world's own solved surface —
analytic, no voxel grid read.

---

## Examples

| Script | Shows |
|---|---|
| `scripts/RIBCAGE.md` | fn + for + loops sizing/mirroring instanced parts |
| `scripts/ISLAND.md` | terrain layers, generators, world-as-thing |
| `scripts/SKELETON.md` | current-idiom parts, relations, blended CSG |
| `scripts/SKELETON_v2.md` / `_v3.md` | earlier idiom generations, kept for the capability gaps they demonstrate |
| `scripts/MUG.md` | CSG `difference` + a torus handle |
| `scripts/AXLE.md` | CSG `union` + `spin`, derived (not hand-typed) offsets |
| `scripts/TREES.md` | thing parameters, proportional derived values |
| `scripts/SCARECROW.md` | `surface()` vs `shift` for face features |
| `scripts/MOXIBOI.md` | a fully posed figure; also documents the pitch/twist posing pitfall |

---

## Viewer Controls

| Input | Action |
|-------|--------|
| Left / right drag | Orbit |
| Scroll | Zoom |
| Middle drag | Pan |
| Arrow keys / WASD | Pan |

---

## Architecture

```
src/
  lexer/            characters → tokens; fence masking
  parser/           tokens → typed AST; relation sugar, for/fn parsing
  ast/              every Moxi construct as Rust types
  value.rs          the value domain + math builtins (Phase D/E1)
  resolver/         flattening, substitution, fn expansion, loop unrolling
  frame.rs          Vec3 / Mat3 / Frame — rigid transform math
  anchors.rs        the anchor vocabulary; analytic extents
  frame_resolver.rs placements → world frames (Kahn toposort); constraints
  geometry/         contains() [voxel] AND distance() [SDF] per shape
  scene.rs          the canonical IR every backend reads
  mesh.rs           surface-nets mesher (SDF → triangles)
  shader.rs         scene IR → GLSL raymarcher + self-contained HTML
  voxel/            flat u16[x][y][z] grid
  generator.rs      scatter pass, analytic elevation, spacing
  bench.rs          bench/ corpus runner — property assertions, not golden files
  spec.rs           grammar surface emitted as JSON, source of SKILL.md
  skill.rs          renders SKILL.md from docs/skill_preamble.md + spec.rs
  pipeline.rs       compile_source / compile_to_scene — the entry points
  export.rs         OBJ + MTL writer
  bevy_viewer.rs    3D viewer (--features viewer), draws primitives + meshes
  wasm_abi.rs       raw C-ABI WebAssembly exports
  main.rs           CLI: check / json / scene / compile / mesh / web / view
```

---

## Design Principles

- **Explicit over implicit** — every mapping declared, no inference
- **Strict mode default** — errors reported, never silently wrong
  (except the known mirroring bug above — being fixed)
- **Semantics before geometry** — describe what things *are*
- **Total, not Turing-complete** — no `while`, no unbounded recursion;
  compile-time-constant loop bounds; deliberate, not a limitation
- **Analytic until the last step** — placement never reads a voxel grid
- **AI-friendly grammar** — low ambiguity, every error names the valid
  vocabulary so a model can repair its own output
- **One canonical scene, many backends** — voxels, mesh, raymarch all
  read the same IR; none is privileged
- **Named everything** — anonymous geometry is forbidden
- **Composable** — every construct combinable with every other

---

## License

Apache-2.0