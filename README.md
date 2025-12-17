<!-- LOGO -->
<p align="center">
  <img src="https://github.com/user-attachments/assets/89195ff1-f449-433e-8115-09b4ad793392"
       width="220"
       alt="moxilang logo"/>
</p>

<h1 align="center">Moxi</h1>

<p align="center">
  <strong>A voxel programming language for building 3D worlds</strong><br/>
  Explicit for humans. Reliable for AI generation.
</p>

<p align="center">
  Clear separation between <em>structure</em>, <em>meaning</em>, and <em>appearance</em>.
</p>

<p align="center">
  ⚠️ <strong>Status:</strong> Active development · Breaking changes expected
</p>

**Moxi** (pronounced *Mochi*) is a voxel programming language and engine for building 3D worlds.


---

## ✨ Features

- **Atoms & Legends**: semantic atoms define properties; legends map glyphs to atoms.
- **Voxel Models**: layered ASCII grids with explicit meaning.
- **Transformations**: `translate`, `merge`, and compositional instance building.
- **AI-Friendly Syntax**: deterministic, low-ambiguity grammar.
- **Viewers**:
  - Bevy 3D interactive preview
  - Lightweight isometric viewer
- **Export**: voxel scenes → `.obj`

---

## ⚠️ Strict Mode (Default)

Moxi now enforces **strict semantic rules**:

- Colors must be defined via **atoms**
- Glyphs must be mapped using **legend**
- Non-ASCII glyphs are disallowed
- Implicit color mappings are removed

This prevents semantic collapse and improves large-scale composition.

---

## 🚀 Getting Started

### Build

```bash
git clone https://github.com/moxilang/moxi-lang.git
cd moxi-lang
cargo build --release
```

### Run

```bash
# Preview a .mi script
cargo run examples/forest.mi

# Run without preview
cargo run examples/forest.mi --no-show

# Export to .obj
cargo run examples/test.mi --output out.obj
```

---

## 📜 Example (Strict Mode)

```mi
# === ATOMS ===
atom DARK  { color = black }
atom LIGHT { color = white }

# A parameterized 2x2 checkerboard tile
voxel Checkers(dark_atom, light_atom) {

    legend {
        D = dark_atom
        L = light_atom
    }

    [Layer 0]
    DL
    LD
}

# --- One small tile (2x2)
plane = Checkers(DARK, LIGHT)

# --- Build a supertile (2x2 planes, offset by 2)
tile1 = translate(plane, (x=0, y=0, z=0))
tile2 = translate(plane, (x=2, y=0, z=0))
tile3 = translate(plane, (x=0, y=2, z=0))
tile4 = translate(plane, (x=2, y=2, z=0))

supertile = merge(tile1, tile2, tile3, tile4)

# --- Build a mega_checker (2x2 supertiles)
mega1 = translate(supertile, (x=0, y=0, z=0))
mega2 = translate(supertile, (x=4, y=0, z=0))
mega3 = translate(supertile, (x=0, y=4, z=0))
mega4 = translate(supertile, (x=4, y=4, z=0))

mega_checker = merge(mega1, mega2, mega3, mega4)

# --- Show results
print mega_checker
print
```

---

## 📖 Language Guide

See [MOXI_LANG.md](./MOXI_LANG.md) for the full specification:

* Atoms
* Legends
* Voxel models
* Transformations
* Runtime behavior

---

## 🧠 Vision

MoxiLang is a playground for **AI-assisted 3D generation**.

By enforcing explicit semantics and compositional structure, it enables
machines to generate worlds without guessing — and humans to reason about them.

---

## 📜 License

MIT

