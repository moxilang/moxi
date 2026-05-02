<!-- LOGO -->
<p align="center">
  <img width="240" alt="moxiboi" src="https://github.com/user-attachments/assets/56d9970c-5d4a-4687-a4ab-e5c223624ee0" />
</p>

<h1 align="center">Moxi</h1>

<p align="center">
  <strong>A compiler for structured 3D worlds.</strong><br/>
  From  <strong>semantics → geometry → voxels → export/render</strong><br/>
  Explicit for humans. Deterministic for machines.
</p>

---

## What Moxi Is

Moxi is not just a voxel DSL.

It is a **multi-stage pipeline**:

```
Source (.mi)
→ Lexer
→ Parser (AST)
→ Semantic Resolver
→ Geometry Compiler
→ Relation Resolver
→ Voxel Scene
→ Viewer / OBJ Export
```

You are not writing grids — you are defining **structured 3D systems**.

---

## Core Concepts

### 1. Structure (Geometry)

Primitive shapes compiled into voxels:

```mi
part Skull {
  shape = sphere(radius=4)
}
```

---

### 2. Semantics (Meaning)

Named components and materials:

```mi
material Bone {
  color = ivory
  voxel_atom = BONE
}

entity Skeleton {
  part Skull { shape = sphere(radius=4), material = Bone }
}
```

---

### 3. Relations (Spatial Logic)

Relative placement between parts:

```mi
relation Skull above Spine
```

Relations are resolved into offsets during compilation
(currently heuristic and evolving).

---

### 4. Constraints (Early System)

Constraints exist in the language and are partially enforced:

```mi
constraint Spine.height > 20
```

This system is under active development.

---

## Features

### Explicit Semantics

* `atom`, `material`, `entity`, `part`
* no implicit naming or hidden behavior

---

### Deterministic Geometry

* Built-in shapes:

  * `sphere`, `box`, `cylinder`, `cone`, `ellipsoid`
  * procedural: `blob`, `heightfield`
  * composition: `shell`, `extrude`
* Fully reproducible voxelization

---

### Composition & Resolution

* Part-based entity construction
* Relation-based placement (heuristic)
* Merge + transform at voxel level

---

### Export & Visualization

* `.obj + .mtl` export
* Optional viewer using Bevy Engine

---

## Example

```mi
atom BONE { color = ivory }

material Bone {
  color = ivory
  voxel_atom = BONE
}

entity Skeleton {
  part Skull {
    shape = sphere(radius=4)
    material = Bone
  }

  part Spine {
    shape = cylinder(height=24, radius=1)
    material = Bone
  }

  relation Skull above Spine
}
```

---

## Architecture

From the codebase:

* Lexer → `src/lexer/`
* Parser → `src/parser/`
* AST → `src/ast/`
* Resolver → `src/resolver/`
* Geometry → `src/geometry/`
* Relations → `src/relation_resolver.rs`
* Export → `src/export.rs`



---

## Design Principles

* **No implicit behavior**
* **Everything is named**
* **Deterministic execution**
* **Semantics before geometry**

---

## Status

⚠️ Active development
⚠️ Breaking changes expected

Areas still evolving:

* relation resolution
* constraint enforcement
* world / generator systems

---


## Viewer 

```bash
cargo run --features viewer
```

---

## Vision

Moxi sits between:


> natural language ↔ structured 3D ↔ geometry


Moxi is a **language layer for reasoning about 3D structure**, not just rendering it.

---

## License

Apache License
