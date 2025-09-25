# Moxi Programming Language Guide

Moxi is a domain-specific language (DSL) for creating voxel-based models and worlds.  
It mixes declarative voxel grids with imperative commands for procedural generation.  
Moxi is designed to be simple, composable, and AI-friendly.

---

## Core Concepts

- **Voxel** → a cube at `(x,y,z)` with a symbol and a color.
- **Voxel model** → a collection of voxels grouped under a name.
- **Symbol** → an identity marker (like `X`, `🌳`, or `🐉`). By itself, a symbol is just one cube.
- **Color mapping** → assigns a color to each symbol.

---

## Syntax Overview

### 1. Voxel Models
Define models using `voxel` blocks.

```moxi
voxel Tree {
    [Layer 0]
    .X.
    .X.

    [Colors]
    X: brown
}
```

### 2. Direct Placement

Place voxels at coordinates.

```moxi
voxel MonkeyTree {
    add Layers(0,2,1){🐒}
    add Colors { 🐒: green }
}
```

### 3. Colors

Two interchangeable ways:

```moxi
add Colors {
    A: red
    B: blue
}

[Colors]
A: red
B: blue
```

### 4. Parameters and Functions

Voxel models can take parameters.

```moxi
voxel Tree(steps, tree_colors) {
    for move_id, color in steps, tree_colors:
        add Colors { move_id: color }
}
```

### 5. Transformations (stdlib)

Operations return new models.

```moxi
clone(model)
translate(model, (x,y,z))
rotate(model, "y", 90)
mirror(model, "x")
merge(model1, model2, ...)
```

### 6. Generation Helpers

```moxi
grid(model, nx, ny, spacing=(2,2))
circle(model, radius, count)
```

### 7. Runtime Actions

```moxi
view      # open preview
export    # save to file (future feature)
```

---

## Example Program

```moxi
# Define a tree
voxel Tree {
    [Layer 0]
    .X.
    .X.
    [Colors]
    X: brown
}

# Create a forest
forest = grid(Tree, 5, 5, spacing=(3,3))

# Add a dragon above
voxel Dragon {
    [Layer 0]
    ..🐉..
    .🐉🐉🐉.
    ..🐉..
    [Colors]
    🐉: neon-blue
}

dragon1 = translate(Dragon, (7, 5, 7))

# Merge into final scene
scene = merge(forest, dragon1)

view
```

---

## Notes for AI Generation

* Always define voxel models with `voxel Name { ... }`.
* Symbols (emojis, letters) are just voxel identities; colors must be mapped.
* Use transformations (`translate`, `rotate`, etc.) to position models.
* Use generation helpers (`grid`, `circle`) for procedural placement.
* End with `view` to preview or `export` to save.


