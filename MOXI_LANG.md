# Moxi Programming Language Guide

Moxi is a domain-specific language (DSL) for creating voxel-based models and worlds.  
It combines declarative voxel grids with composable transformations, and is designed
to be **explicit, modular, and AI-friendly**.

As of **Phase 1.3**, Moxi operates in **strict semantic mode by default**.

---

## Core Concepts (Strict Mode)

Moxi separates **structure**, **meaning**, and **appearance** explicitly.

- **Atom**  
  A semantic unit with properties (e.g. `color`).  
  Atoms carry *meaning*, not geometry.

- **Glyph**  
  A single ASCII character used inside layers (e.g. `T`, `L`).  
  Glyphs are **pure structure** and have no meaning by themselves.

- **Legend**  
  An explicit mapping from glyphs → atoms.  
  This is the only way glyphs acquire meaning.

- **Voxel**  
  A cube produced by resolving:  
  `glyph → atom → atom properties`.

- **Voxel Model**  
  A named collection of voxels defined by layers and a legend.

> In strict mode:
> - Colors cannot be assigned directly to glyphs.
> - All glyphs must be declared in a `legend`.
> - Non-ASCII glyphs are disallowed.

---

## Syntax Overview

### 1. Atoms

Atoms define semantic properties.

```mi
atom TRUNK { color = brown }
atom LEAF  { color = green }
```

---

### 2. Voxel Models

Voxel models define structure using layers and legends.

```mi
voxel Tree {

    legend {
        T = TRUNK
        L = LEAF
    }

    [Layer 0]
    .T.
    TTT
    .T.

    [Layer 1]
    LLL
    LTL
    LLL
}
```

---

### 3. Parameters

Voxel models may accept atoms as parameters.

```mi
voxel Checkers(dark_atom, light_atom) {

    legend {
        D = dark_atom
        L = light_atom
    }

    [Layer 0]
    DL
    LD
}
```

Invocation:

```mi
atom DARK  { color = black }
atom LIGHT { color = white }

tile = Checkers(DARK, LIGHT)
```

---


### 4. Transformations (Standard Library)

Transformations operate on **model instances** and return **new instances**.
They do not mutate existing objects.

```moxi
clone(model)
translate(model, (x, y, z))
rotate(model, axis, turns)
mirror(model, axis)
merge(model1, model2, ...)
```

**Notes:**

* `translate` and `merge` are fully implemented.
* `rotate` and `mirror` are defined at the language level and supported by the transform system.
* `clone` is a semantic alias for identity + instance duplication.

All transformations are composable:

```moxi
shifted = translate(tree, (x=5, y=0, z=0))
rotated = rotate(shifted, "y", 1)
scene   = merge(tree, rotated)
```

---

### 5. Generation Helpers

Generation helpers produce **structured arrangements** of instances.

```moxi
grid(model, nx, ny, spacing=(dx, dy))
circle(model, radius, count)
```

These helpers are intended for:

* procedural layouts
* AI-driven generation
* large-scale composition

> Some helpers may be partially implemented or planned, but are part of the language surface.

---

### 6. Runtime Actions

Runtime actions affect execution, not geometry.

```moxi
view      # open preview window
export    # export scene to file (future)
print     # inspect values or scene state
```

---

---

## Deprecated (Legacy Mode)

The following constructs are **disabled in strict mode**:

* `[Colors]`
* `add Colors { ... }`
* Emoji glyphs
* Implicit symbol → color mappings

They existed in early prototypes and are kept only for historical reference.

---

## Notes for AI Generation

* Always define **atoms first**.
* Always use a **legend** for every voxel model.
* Glyphs are structural only — never semantic.
* Prefer small ASCII alphabets (`A–Z`, `0–9`).
* Avoid inference: make mappings explicit.

Strictness is intentional — it improves composability, correctness, and AI reliability.

