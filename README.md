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

**`examples/forest.mi`**

```mi
voxel Tree {

    [Layer 0]
    .X.
    XXX
    .X.

    [Layer 1]
    XXX
    .X.
    XXX

    [Layer 2]
    🌳🌳🌳
    🌳🍡🌳
    🌳🌳🌳

    [Colors]
    X: brown
    🌳: green
    🍡: mochi-pink
}

print
# Clone of scene // 1*2 models
clone
translate 5 0 0

# Clone of Clone of scene // 1*2^2 models
clone
translate 0 5 0

# Clone of Clone of Clone of scene // 1*2^3 models
clone
translate 0 10 0

# you get the idea...
clone
translate 10 0 0

clone
translate 0 0 6

print
```

---

<img height="444" alt="Screenshot from 2025-09-25 19-59-14" src="https://github.com/user-attachments/assets/929477d3-6a6b-4f04-a761-2c29f26a1079" />

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

