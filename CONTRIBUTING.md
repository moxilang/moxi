# Contributing to Moxi

Thanks for helping. Moxi is a compiler for structured 3D worlds — you write
what things *are* and how they *relate*, and the compiler produces geometry.

This document covers how to build it, and four rules that are non-obvious
enough that you will otherwise break them in good faith.

---

## Build and test

```bash
git clone https://github.com/moxilang/moxi
cd moxi

cargo test                      # run this before every push
cargo run -- check scripts/ISLAND.md
cargo run -- json  scripts/MUG.md

# 3D viewer (optional feature, Bevy — first build is slow)
cargo run --features viewer -- view scripts/SKELETON_v2.md
```

Every script in `scripts/` should compile at all times. If your change alters
the voxel output of an existing script, say so in the PR and explain why the
new output is more correct — see "Parity" below.

---

## Architecture in one screen

```
script.md
  ↓  lexer/       characters → tokens (Markdown handled here)
  ↓  parser/      tokens → AST; relation keywords desugar to Align/Mirror
  ↓  resolver/    names → indices, instances flattened, params substituted
  ↓  frame_resolver  placements → one world Frame per part (toposort, exact)
  ↓  geometry/    contains(shape, F⁻¹·p) per voxel → VoxelGrid
  ↓  pipeline.rs  world assembly, JSON surface
  ↓  export / bevy_viewer
```

Two things to internalize:

- **`anchors.rs` is analytic.** Anchors and extents come from shape
  *parameters*, never from voxel data. Nothing upstream of `geometry/` may
  read a grid.
- **`frame_resolver.rs` solves each part exactly once** via Kahn's algorithm.
  Cycles and double-placements are hard errors with spans. Do not add a
  fixpoint loop; the old resolver was one and it silently produced wrong
  offsets for chains longer than four.

`pipeline.rs::compile_source` is the single entry point for every target —
CLI, server, WASM. Nothing in it prints or exits. Keep it that way.

---

## The four rules

### 1. Error messages are a public API

Moxi's primary consumer is a language model, and error text is the model's
only feedback channel at inference time. `compile_to_json` returns
`{stage, message, line, col}` so a model that emits bad Moxi can repair its
own output in one round trip.

That means:

- **Every new construct needs a vocabulary-listing error.** The model to copy
  is `MoxiError::UndefinedAnchor`, which carries `valid: String` — the full
  list of legal anchors for that shape. Unknown parameter, unknown module
  member, out-of-range index: all of them get the same treatment.
- **Never widen an error into a generic one.** "invalid argument" is a
  regression even if it compiles.
- **Changing error text is a reviewable change.** If you reword a message,
  say so in the PR description.

### 2. The language is *total*

Compilation always terminates. There is no `while`, no unbounded recursion,
no general fixpoint, and there will not be. Loop bounds and recursion depths
must be compile-time constants.

This is a deliberate trade — see the "Explicitly rejected" section of
`ROADMAP.md`. If your feature needs unbounded computation, the feature needs
redesigning.

### 3. Don't hand-edit generated documentation

Once `moxi spec` and `moxi skill` land (Phase M), `SKILL.md` is **generated**
from the compiler's own tables. Edit the generator, not the output. CI will
fail if the committed file is stale.

The reason: hand-written docs for a moving DSL drift, and drift is invisible
— the model keeps writing valid-two-versions-ago Moxi and the language gets
blamed.

### 4. Parity is a claim you have to make explicitly

Geometry changes can legitimately shift voxel output (Phase B1 did — sampling
rounds once now instead of twice). That's fine, but it must be stated. If a
PR changes the output of `ISLAND.md`, `SKELETON_v2.md`, or `GROVE.md`, the
description must say which scripts changed and why the new result is more
correct rather than merely different.

Noise determinism specifically: blob and heightfield noise is keyed on
**shape-local** voxel indices, so a rotated blob is the same blob. Don't
break that — there's a test.

---

## Pull requests

- One logical change per PR. Compiler PRs get large fast; resist bundling.
- Tests live next to the code in `#[cfg(test)] mod tests`. Follow the
  existing style: parse real Moxi source, resolve it, assert on the flattened
  output. Integration-style beats unit-style here.
- New language surface needs: a parser test, a resolver test, an error-path
  test, and a script in `scripts/` demonstrating it.
- Run `cargo test` and `cargo clippy`. The codebase is dependency-light by
  design (`anyhow`, `clap`, `serde`, `unicode-segmentation`, optional Bevy and
  wasm-bindgen) — adding a dependency needs justification in the PR.

---

## Picking something to work on

Open issues are labelled by roadmap phase (`phase:M`, `phase:S`, `phase:P`).
`ROADMAP.md` explains the ordering and why some obvious-looking features are
deliberately deferred — worth reading before proposing new ones.

If you're new to the codebase, `good-first-issue` items are chosen so you can
finish them without understanding frames or rasterization.

Issues labelled `needs-design` are not ready to implement. Comment on them
rather than opening a PR.

---

## Questions

Open a discussion or comment on the relevant issue. If something in this file
turned out to be wrong or missing, that's worth a PR on its own.
