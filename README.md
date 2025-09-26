# MoxiLang

**MoxiLang** (pronounced *Mochi*) is a voxel programming language designed to be **LLM-friendly** and composable.  
It mixes **declarative voxel grids** with **imperative commands** for procedural generation, making it easy to script 3D voxel worlds by hand or by AI.

⚠️ **Status**: This project is in **active development**.  
It is not yet stable or officially released — expect changes, breaking updates, and experimentation.

---

## ✨ Features
- **Voxel Models**: define layers and assign colors to symbols or emojis.  
- **Procedural Commands**: clone, translate, rotate, merge, and print voxel scenes.  
- **AI-Friendly Syntax**: designed so LLMs can generate runnable `.moxi` scripts.  
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
# Preview a .moxi script
cargo run -- --input examples/forest.moxi --preview

# Export to .obj
cargo run -- --input examples/test.moxi --output out.obj
```

---

## 📜 Example

**`examples/forest.moxi`**

```moxi
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
    r🌳🌳

    [Colors]
    X: brown
    r: red 
    🌳: green
    🍡: mochi-pink
}

print
# Clone 1 of scene // 1*2 models
clone
translate 0 0 10
rotate x 1
```

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
* `examples/` → sample `.moxi` programs.
* `src/legacy/` → legacy ASCII voxel grid parser.

---

## 💡 Vision

MoxiLang is experimental but aims to become a **playground for AI-assisted 3D generation**.
By keeping syntax simple and explicit, it’s easy for LLMs to generate scripts that render into voxel worlds.

---

## 📜 License

MIT

