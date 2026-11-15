# Working on the compiler

For people changing Moxi itself. To write Moxi, read `SKILL.md`.

## Pipeline

```
script.md
  ↓  Lexer            characters → tokens; ```moxi fences mask the prose
  ↓  Parser           tokens → AST; relation sugar desugars; for/fn parsed.
                       A document that does not parse stops here.
  ↓  Imports          `from "lib/x.md" import Thing` pulled in
  ↓  Named shapes     `shape Name(…)` calls and shape loops expanded
  ↓  Resolver         names, instance flattening, parameter substitution,
                       fn-call expansion (arguments computed once),
                       for-loop unrolling, index folding
  ↓  Frame solver     placements → one exact world frame per part (toposort);
                       spans get their paths
  ↓  Constraints      declared rules checked against the solved frames
  ↓  Scene            the canonical artifact: shapes + frames + materials.
                       Parts too thin to mesh are refused here.
       ├─ gltf      — one node per part, hierarchy = relation tree, poses,
                       animations, material variants
       ├─ mesh      — surface nets over the distance field → OBJ + MTL
       ├─ web       — GLSL raymarcher in one self-contained HTML page
       ├─ report    — what was built: parts, contacts, pieces, buried parts
       └─ json / compile — legacy voxel output, kept for the v0 sandbox
```

Everything before the backends is analytic: anchors and extents come from
shape parameters, never from sampled geometry, so rotations are exact and
errors are found before anything is built.

## Source

```
src/
  lexer/            characters → tokens; fence masking
  parser/           tokens → AST; relation sugar, for/fn parsing, recovery
  ast/              every Moxi construct as Rust types
  imports.rs        one file's things and materials in another
  shapes.rs         named shapes and shape loops
  value.rs          the value domain and math builtins; let/if/fn evaluation
  resolver/         flattening, substitution, fn expansion, loop unrolling
  frame.rs          Vec3 / Mat3 / Frame: rigid transform math
  anchors.rs        the anchor vocabulary; analytic extents
  frame_resolver.rs placements → world frames; constraints
  joints.rs         the joint tree of a solved thing (poses, animation)
  tube.rs           tube and lathe: shapes built from a list of points
  geometry/         contains() and distance() per shape
  scene.rs          the canonical scene every backend reads
  pipeline.rs       compile_to_scene and friends: the entry points
  mesh.rs           surface-nets mesher; the too-thin check; part weights
  gltf.rs           glTF 2.0 export
  shader.rs         scene → GLSL raymarcher + self-contained HTML
  report.rs         `moxi report`
  generator.rs      scatter pass over the printed world's surface
  voxel/, export.rs, types.rs, geom.rs, colors.rs
                    the legacy voxel path and its OBJ cube export
  spec.rs           the grammar surface as JSON, the source of SKILL.md
  skill.rs          renders SKILL.md from docs/skill_preamble.md + spec.rs
  bench.rs          bench/ corpus runner: property assertions, not golden files
  bevy_viewer.rs    3D viewer (--features viewer)
  wasm.rs, wasm_abi.rs  the WebAssembly target
  main.rs           the CLI
  *_tests.rs        cross-module tests, one file per issue
```

## Checks

```bash
cargo test --release
cargo clippy --release --all-targets
moxi skill --check        # SKILL.md matches docs/skill_preamble.md + spec.rs
moxi bench bench --check  # the bench corpus
```

`SKILL.md` is generated: edit `docs/skill_preamble.md` or `src/spec.rs`,
then `moxi skill > SKILL.md`. `cargo test` does not rebuild
`target/release/moxi`; run `cargo build --release` before comparing
outputs.

## Principles

- **Errors, not silent wrongness.** A part that cannot be drawn, placed or
  built is an error that says how to fix it.
- **Total.** No `while`, no recursion, loop bounds known at compile time:
  every script finishes.
- **Every error names the valid vocabulary,** so a model can repair its
  own output.
- **One scene, many backends.** gltf, mesh, web and report all read the
  same solved scene.
- **Named everything.** Every part has a name, and the report and glTF use it.
