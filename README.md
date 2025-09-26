<p align="center">
  <img height="400" alt="moxilang-cropped" src="https://github.com/user-attachments/assets/aca8e6bb-4a8c-4ed9-abc5-2972e89f89b0" />
</p>

**MoxiLang** (pronounced *Mochi*) is a voxel programming language and engine for building 3D worlds.  
It’s designed to be **simple and intuitive for humans**, while also being **straightforward enough that GPT-style AIs can generate scripts** from prompts.  

MoxiLang mixes **declarative voxel grids** with **imperative commands** (clone, rotate, translate, merge) — so you can script voxel scenes by hand, or let an AI help you imagine and generate them.

⚠️ **Status**: This project is in **active development**.  
It is not yet stable or officially released — expect changes, breaking updates, and experimentation.

---

## ✨ Features
- **Voxel Models**: define layers and assign colors to symbols or emojis.  
- **Procedural Commands**: clone, translate, rotate, merge, and print voxel scenes.  
- **AI-Friendly Syntax**: designed so LLMs can generate runnable `.mi` scripts.  
- **Export**: output voxel scenes as `.obj` files.  
- **Viewers**:  
  - Lightweight **minifb** viewer (isometric).  
  - Interactive **Bevy 3D** previewer with orbit controls.  
- **Legacy Support**: includes an older ASCII grid parser (`voxgrid`) for compatibility.  

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

## 📜 Example

```mi
# A parameterized 2x2 checkerboard tile
voxel Checkers(dark_color, light_color) {
    [Layer 0]
    ⬜⬛
    ⬛⬜

    add Colors { ⬛: dark_color, ⬜: light_color }
}

# --- Base colors
dark  = ["black"]
light = ["#ffffffff"]

# --- One small tile (2x2)
plane = Checkers(dark, light)

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
```

---

<img width="555" height="666" alt="Screenshot from 2025-09-26 15-25-24" src="https://github.com/user-attachments/assets/d18b24f7-3de4-4d9f-95f2-edc3fceb0396" />

---

## 📖 Language Guide

See [MOXI_LANG.md](./MOXI_LANG.md) for the full specification:

* Voxel models
* Layers & colors
* Built-in commands
* Transformations & helpers
* Runtime actions

---

## 🧩 Project Layout

* `src/moxi/` → core language (lexer, parser, runtime, commands).
* `src/bevy_viewer.rs` → interactive 3D preview.
* `src/viewer.rs` → minimal isometric viewer.
* `src/export.rs` → export to `.obj`.
* `examples/` → sample `.mi` programs.
* `src/legacy/` → legacy ASCII voxel grid parser.

---

## 💡 Vision

MoxiLang is experimental but aims to become a **playground for AI-assisted 3D generation**.
By keeping syntax simple and explicit, it’s easy for LLMs to generate scripts that render into voxel worlds.

---

## 📜 License

MIT

