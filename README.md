<!-- LOGO -->
<p align="center">
  <img width="240" alt="moxiboi" src="https://github.com/user-attachments/assets/55a90196-865d-405a-a16e-64f07cdef162" />
</p>

<h1 align="center">Moxi</h1>

<p align="center">
  <strong>A compiler for structured 3D worlds.</strong><br/>
  From <strong>semantics → geometry → voxels → export / render</strong><br/>
  Explicit for humans. Deterministic for machines.
</p>

---

## What Moxi Is

Moxi is a spatial description language. You describe what things *are*, how they *relate*, and what rules they must *satisfy* — the compiler turns that into geometry.

Scripts are plain Markdown files. The compiler reads them directly. `#` headings and `>` blockquotes are ignored as comments. Code declarations are compiled.

```md
# Skeleton
> This is a comment. The compiler ignores it.

atom BONE { color = ivory }

entity Skeleton {
    part Skull { shape = sphere(radius=4), material = Bone }
    part Spine { shape = cylinder(height=24, radius=0.8), material = Bone }

    relation {
        Skull above Spine
    }
}

print Skeleton detail=low
```

---

## Pipeline

```
script.md
  ↓  Lexer          characters → tokens
  ↓  Parser         tokens → AST
  ↓  Resolver       names → indices, all errors caught
  ↓  Geometry       ShapeExpr → VoxelGrid (per part)
  ↓  Relations      above/below/surrounds → world offsets
  ↓  Generators     scatter PalmTree count=60 where=elevation>3
  ↓  Merge          all parts → single VoxelScene
  ↓  Export         .obj + .mtl
  ↓  Viewer         Bevy 3D preview (optional)
```

---

## Install

### CLI only

```bash
cargo install moxi
```

### With 3D viewer (recommended)

```bash
cargo install moxi --features viewer
```

> ⚠️ The viewer is **optional** and must be enabled at install time.
> If you skip `--features viewer`, `moxi view` will not open a window.

### From source

```bash
git clone https://github.com/andrewrgarcia/moxi
cd moxi
cargo install --path . --features viewer
```

---

## Usage

```bash
# Check a script for errors
moxi check scripts/ISLAND.md

# Compile to OBJ
moxi compile scripts/ISLAND.md

# Compile to a specific output directory
moxi compile scripts/ISLAND.md --out my_output/

# Open 3D viewer
moxi view scripts/ISLAND.md
```

---

## Script Format

Moxi scripts are `.md` files. Any line starting with `#` or `>` is a comment. Everything else is compiled.

```md
# This is a heading — ignored by compiler
> This is a blockquote — ignored by compiler

atom BONE { color = ivory }   # this line is compiled
```

This means a Moxi script is simultaneously valid Markdown (GitHub renders it as documentation) and valid Moxi source (the compiler reads it directly). One file, two audiences.

---

## Language

### Atoms
The lowest-level unit. Every material references one.
```
atom BONE { color = ivory }
```

### Materials
Bind atoms to semantic surface descriptions.
```
material Bone { color = ivory, voxel_atom = BONE }
```

### Entities and Parts
Named objects built from shape primitives.
```
entity Skeleton {
    part Skull   { shape = sphere(radius=4),            material = Bone }
    part Spine   { shape = cylinder(height=24, radius=0.8), material = Bone }
}
```

### Built-in shapes
`sphere`, `cylinder`, `box`, `cone`, `ellipsoid`, `blob`, `heightfield`, `shell`, `extrude`

### Relations
Spatial relationships between parts — compiled into world-space offsets.
```
relation {
    Skull above Spine
    Ribcage surrounds Spine
    Pelvis below Spine
}
```
Supported: `above`, `below`, `inside`, `outside`, `surrounds`, `adjacent_to`, `left_of`, `right_of`, `in_front_of`, `behind`, `attached_to`, `touch`, `symmetric_across`

### Constraints
Hard rules validated after geometry resolution.
```
constraint Skull above Spine
```

### Generators
Procedural placement over terrain.
```
generator ForestGen {
    scatter PalmTree
    count       = 60
    min_spacing = 5
    seed        = 7
    where       = elevation > 3 and elevation < 13
}
```

### World layering
Multiple entities render in print order. Bottom layers first — each overwrites the one below.
```
print Ocean       detail=low
print SandBase    detail=low
print SoilTerrain detail=low
print RockyPeaks  detail=low
print PalmTree    detail=low
```

---

## Examples

Two example scripts are included in `scripts/`:

**`scripts/ISLAND.md`** — tropical island with ocean, sand beach, soil terrain, rocky peaks, and 70 palm trees placed by generators.

**`scripts/SKELETON.md`** — human skeleton with skull, spine, ribcage, and pelvis assembled via spatial relations.

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
  lexer/        characters → tokens (.md comments handled here)
  parser/       tokens → typed AST
  ast/          every Moxi construct as Rust types
  resolver/     symbol table, name resolution, error detection
  geometry/     shape stampers → VoxelGrid per part
  relation_resolver.rs   spatial relations → world offsets
  generator.rs  scatter pass, elevation sampling, spacing
  voxel/        flat u16[x][y][z] grid
  types.rs      VoxelScene bridge to viewer and exporter
  export.rs     OBJ + MTL writer
  bevy_viewer.rs  merged-mesh 3D viewer (--features viewer)
  colors.rs     color name → hex resolution
  geom.rs       rotation math
  main.rs       CLI: compile / view / check
```

---

## Design Principles

- **Explicit over implicit** — every mapping declared, no inference
- **Strict mode default** — errors reported, never silently wrong
- **Semantics before geometry** — describe what things *are*, not where every voxel goes
- **Voxels as assembly language** — authors work at entity level, compiler handles voxels
- **AI-friendly grammar** — low ambiguity, consistent structure, predictable parse behavior
- **Named everything** — anonymous geometry is forbidden
- **Composable** — every construct combinable with every other

---

## Status — v0.2.0

| Component | Status |
|-----------|--------|
| Lexer | ✅ complete |
| Parser | ✅ complete |
| Resolver | ✅ complete |
| Geometry backend | ✅ complete |
| Relation resolver | ✅ complete |
| Generator pass | ✅ complete |
| OBJ + MTL export | ✅ complete |
| Bevy viewer | ✅ complete |
| CLI (compile/view/check) | ✅ complete |
| Constraint validator | 🔧 parsed, not enforced |
| World block | 🔧 parsed, not compiled |
| GLTF export | 📋 planned |
| Detail levels | 📋 planned |

---

## License

Apache-2.0