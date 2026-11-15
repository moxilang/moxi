# Changelog

## 0.8.1 — Tighten

No new words. Every script that compiled on 0.8.0 compiles to the same
files, except one whose part was never drawn and one that mates a tube by
its end.

- Arguments are computed once: a function or named shape given a
  computed point no longer copies the computation into every use. The
  Kodomata canopy checks in 0.3 s (was 28.6 s and 3.5 GB); scripts that
  were killed for memory now build.
- A part too thin to be drawn is an error that says how thick it must be,
  where it used to vanish from the mesh without a word.
- `moxi report` names parts wholly inside another solid part on a
  `buried:` line (teeth behind closed lips, a rug under the floor).
- A tube mated by `start` or `end` keeps its own axes, as a cylinder mated
  by `bottom` or `top` does. It used to come out turned a quarter around,
  so its bends went the wrong way. Any tube mated by an end, and what
  hangs off it, moves (in the examples, only Moxi Boy's plum stem).
- One mistake, one error: a slip inside `relation { }` no longer brings
  dozens of errors after it, and a `for` written there says where it goes.
- `moxi mesh` and `moxi gltf` print the heaviest part and the cell it was
  meshed at, and for a part over 500,000 triangles the grain that
  lightens it.
- The guide says when to use a span and when loops, and how a tube's
  `start` and `side(angle=0)` sit.

## 0.8.0 — Form

Organic shapes without coordinates. Every script written for 0.7 compiles
to the same output in 0.8 (json, scene, report, glTF and OBJ byte-identical).

**New**

- `tube(path=…, radius=…)`: a smooth tube through a list of points, its
  radius tapering or swelling along the length. `lathe(profile=…)`: a
  silhouette spun about the vertical.
- Spans: `P from END to END` makes a tube run between two parts, and the
  compiler works out its path. Options `reach`, `sag`, `rise`, `via`.
- `cuts = host`: a part's shape is removed from what it is placed on, so a
  module brings its own hole. `fuses = host` and `fuse A, B, …` make parts
  one surface, with `blend` for a fillet where they meet.
- `shape Name(params) = …` names a shape for reuse; a `for` loop can sit
  inside a `union`; `guide` parts place others without being drawn; every
  shape has an `origin` anchor; `rand` gives seeded random numbers.
- Sparse meshing: the mesher skips empty space, with the same output;
  meshing every example script went from 46 s to 13 s.
- Demos: `CRAG`, `PITCHER`, `LANTERN`, `FACADE`, `MUG_FUSED`, and
  `MOXIBOI_FORM`, Moxi Boy rebuilt with the new constructs.
- `tools/studio.py`: a studio renderer for comparing versions under the
  same lights.

**Fixed before release**

- A tube's radius list was ignored between its samples, so a straight tube
  told to swell came out as a plain cone. The radius is now a smooth curve
  through the values written, never past them.
- Inside a tube, the distance stopped at the first round cone holding the
  point, which made bent tubes lumpy.
- `.mtl` materials are written in a fixed order.

**Known limits**

- Spans are not mirrored and cannot be moved in a pose; they follow their
  ends.
- One `blend` per `fuse` line; parts hosted with `fuses =` carry their own.
- A tube shows in the browser view as many small pieces, which can make
  `moxi web` slow on scenes with long tubes.
