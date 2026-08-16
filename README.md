<!-- LOGO -->
<p align="center">
  <img width="240" alt="Moxi" src="https://github.com/user-attachments/assets/2a189048-5069-4c54-8cf1-0b9d6c417ed1" />
</p>

<h1 align="center">Moxi</h1>

<p align="center">
  <strong>A compiler for structured 3D worlds.</strong><br/>
  From <strong>semantics → geometry → voxels → export / render</strong><br/>
  Explicit for humans. Deterministic for machines.
</p>

---

## What Moxi Is

Moxi is a spatial description language. You describe what things *are*, how
they *relate*, and what rules they must *satisfy* — the compiler turns that
into geometry. Nobody writes coordinates.

Scripts are plain Markdown. `#` headings and `>` blockquotes are ignored as
comments; everything else is compiled. One file is simultaneously readable
documentation and compilable source.

```md
# Skeleton
> This is a comment. The compiler ignores it.

atom BONE { color = ivory }
material Bone { color = ivory, voxel_atom = BONE }

entity Skeleton {
    part Skull { shape = sphere(radius=4),                material = Bone }
    part Spine { shape = cylinder(height=24, radius=0.8), material = Bone }

    relation {
        Skull above Spine
    }

    resolve voxel_size = 1.0
}

print Skeleton detail=low
```

---

## Contributing

- **[`ROADMAP.md`](ROADMAP.md)** — where the language is going, in what order,
  and which obvious-looking features are deliberately rejected.
- **[`CONTRIBUTING.md`](CONTRIBUTING.md)** — how to build it, and four rules
  that are non-obvious enough to break in good faith.
- **[`SKILL.md`](SKILL.md)** — the language reference, written for a language
  model. It is also the fastest way for a human to learn Moxi.

Open issues are labelled by roadmap phase (`phase:M`, `phase:S`, `phase:P`).
Items tagged `good-first-issue` are chosen so you can finish them without
understanding the frame solver or the rasterizer.

---

## Pipeline

```
script.md
  ↓  Lexer            characters → tokens (Markdown comments handled here)
  ↓  Parser           tokens → AST; relation keywords desugar to Align / Mirror
  ↓  Resolver         names → indices, instances flattened, parameters substituted
  ↓  Frame solver     placements → one exact world Frame per part (toposort)
  ↓  Constraints      declared rules checked against solved frames
  ↓  Rasterizer       contains(shape, F⁻¹·p) per voxel → VoxelGrid
  ↓  Generators       scatter PalmTree count=60 where=elevation>3
  ↓  Assembly         all layers → VoxelScene
  ↓  Export           .obj + .mtl  ·  JSON  ·  Bevy viewer (optional)
```

Every stage is analytic until the rasterizer. Anchors and extents come from
shape *parameters*, never from voxel data, which is why arbitrary rotations
realize exactly and why errors are static.

---

## Install

```bash
# CLI only
cargo install moxi

# With the 3D viewer (optional — must be enabled at install time)
cargo install moxi --features viewer

# From source
git clone https://github.com/moxilang/moxi
cd moxi
cargo install --path . --features viewer
```

---

## Usage

```bash
moxi check   scripts/ISLAND.md          # errors only, no output
moxi compile scripts/ISLAND.md          # → output/world.obj + .mtl
moxi compile scripts/ISLAND.md --out my_output/
moxi view    scripts/SKELETON_v2.md     # 3D preview (needs --features viewer)
moxi json    scripts/MUG.md             # structured output for web / tooling
```

`moxi json` is the machine surface: `{"ok":true,"voxels":[…]}` on success, or
`{"ok":false,"errors":[{"stage","message","line","col"}…]}` on failure. Same
function backs the CLI, the WASM build and any server.

---

## Language

Full reference: **[`SKILL.md`](SKILL.md)**. The shape of it:

### Atoms and materials
```
atom BONE { color = ivory }
material Bone { color = ivory, voxel_atom = BONE }
```

### Shapes
`sphere` · `cylinder` · `box` · `cone` · `ellipsoid` · `blob` ·
`heightfield` · `shell` · `extrude`

Composed with CSG — every shape is a containment predicate, so these are
closed under each other and nest arbitrarily:

```
union(a, b, …)        intersect(a, b, …)      difference(base, cut, …)
at(shape, x=, y=, z=)                spin(shape, axis=, degrees=)
```

### Anchors
Named frames on a shape — a position and an outward normal. Universal on every
shape: `center`, `top`, `bottom`, `north`, `south`, `east`, `west`,
`point(…)`. Refined per shape: `surface(yaw, pitch)` on spheres and
ellipsoids, `side(t, angle)` and `rim_top/rim_bottom(angle)` on cylinders,
`apex`/`base`/`side` on cones, `surface(x, z)` on heightfields.

### Placement
Mate two anchors; normals oppose:

```
relation {
    Handle.west     on Body.side(t=0.55, angle=90)
    RightArm.socket on Ribcage.east twist=-90 pitch=70 gap=1
    LeftArm  symmetric_across Spine from=RightArm
}
```

Relation keywords are sugar over the same mate — `above` is
`subject.bottom on object.top`. Available: `above`, `below`, `inside`,
`outside`, `surrounds`, `adjacent_to`, `left_of`, `right_of`, `in_front_of`,
`behind`, `attached_to`, `touch`, `symmetric_across`.

Each part is the subject of at most one placement. Cycles and
double-placements are hard errors with source spans.

### Composition
Entities instance entities, to any depth:

```
entity Skeleton {
    part RightArm { entity = Arm }
    part LeftArm  { entity = Arm }
}
```

Instances expose exported anchors (`anchor socket = Humerus.top`) and answer
the universal compass automatically, computed from their solved assembly box.

### Parameters
```
entity PalmTree(height=6, crown=3) { … }

part Tall { entity = PalmTree(height=10, crown=4) }
part Mid  { entity = PalmTree }
```

### Constraints
Checked against solved geometry; a violation aborts with expected vs actual.
```
constraint Skull above Ribcage
```

### Generators and layering
```
generator ForestGen {
    scatter PalmTree
    count = 60, min_spacing = 5, seed = 7
    where = elevation > 3 and elevation < 13
}

print Ocean detail=low
print SandBase detail=low
```

---

## Examples

| Script | Shows |
|---|---|
| `scripts/ISLAND.md` | terrain layers, generators, determinism notes |
| `scripts/SKELETON.md` | the basics — parts and relations |
| `scripts/SKELETON_v2.md` | instancing, exported sockets, mirroring |
| `scripts/SKELETON_v3.md` | posed limbs at arbitrary angles |
| `scripts/MUG.md` | CSG: `difference` + `at` |
| `scripts/AXLE.md` | CSG: `union` + `spin` |
| `scripts/GROVE_v2.md` | instance compass anchors, zero exports |
| `scripts/TREES.md` | entity parameters |

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
  lexer/            characters → tokens (Markdown comments handled here)
  parser/           tokens → typed AST; relation sugar desugars here
  ast/              every Moxi construct as Rust types
  resolver/         symbol table, instance flattening, parameter substitution
  frame.rs          Vec3 / Mat3 / Frame — rigid transform math
  anchors.rs        the anchor vocabulary; analytic extents
  frame_resolver.rs placements → world frames (Kahn toposort); constraint checks
  geometry/         containment predicates + the rasterizer
  voxel/            flat u16[x][y][z] grid
  generator.rs      scatter pass, elevation sampling, spacing
  pipeline.rs       compile_source / compile_to_json — the one entry point
  types.rs          VoxelScene bridge to viewer and exporter
  export.rs         OBJ + MTL writer
  bevy_viewer.rs    merged-mesh 3D viewer (--features viewer)
  wasm_abi.rs       raw C-ABI WebAssembly exports (no wasm-bindgen needed)
  colors.rs         color name → hex
  main.rs           CLI: compile / view / check / json
```

---

## Design Principles

- **Explicit over implicit** — every mapping declared, no inference
- **Strict mode default** — errors reported, never silently wrong
- **Semantics before geometry** — describe what things *are*
- **Voxels as assembly language** — authors work at entity level
- **Analytic until the last step** — placement never reads a voxel grid
- **AI-friendly grammar** — low ambiguity, and every error names the valid
  vocabulary so a model can repair its own output
- **Named everything** — anonymous geometry is forbidden
- **Composable** — every construct combinable with every other

---

## Status — v0.3.0

| Component | Status |
|-----------|--------|
| Lexer / Parser / Resolver | ✅ complete |
| Frame solver (exact, toposorted) | ✅ complete |
| Anchor vocabulary | ✅ complete |
| Containment rasterizer (arbitrary rotation) | ✅ complete |
| CSG — union / difference / intersect / at / spin | ✅ complete |
| Entity composition and instancing | ✅ complete |
| Entity parameters | ✅ complete |
| Constraint validator | ✅ enforced (`above`, `below`, `inside`, `surrounds`) |
| Generator pass | ✅ complete |
| OBJ + MTL export · JSON surface · WASM | ✅ complete |
| Bevy viewer | ✅ complete |
| Remaining constraint predicates (lateral) | 🔧 partial |
| `world` block | 🔧 parsed, not compiled |
| Generated language spec / SKILL.md | 📋 planned — Phase M |
| GLTF export | 📋 planned |
| Detail levels | 📋 planned |

See [`ROADMAP.md`](ROADMAP.md) for what comes next and why in that order.

---

## License

Apache-2.0