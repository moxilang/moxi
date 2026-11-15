<p align="center">
  <img width="200" alt="Moxi" src="https://github.com/user-attachments/assets/2a189048-5069-4c54-8cf1-0b9d6c417ed1" />
</p>

<h1 align="center">Moxi</h1>

<p align="center">
  A 3D modeling language written for language models.<br/>
  Name the parts and how they attach; the compiler works out where
  everything goes and builds the model.
</p>

---

```moxi
material Wood  { color = "#7a5a3c" }
material Rope  { color = "#c9b48a" }
material Iron  { color = "#3b3b3f", roughness = 0.5, metalness = 0.6 }
material Glass { color = "#ffcf6b", emissive = 1.5 }

thing Lantern {
    part Body   { shape = lathe(profile=[[1.2, 0], [1.5, 0.6], [1.5, 2.6], [0.9, 3.3], [0.9, 3.6]]), material = Glass }
    part Handle { shape = tube(radius=0.12), material = Iron }
    part Bail   { shape = sphere(radius=0.25), material = Iron }
    relation {
        Handle from Body.rim_top(angle=90) to Body.rim_top(angle=270) reach=0.9
        Bail.bottom on Handle.top
    }
    resolve grain = 0.1
}

thing Line {
    part Ground { shape = box(width=30, height=1, depth=8), material = Wood }
    part PostA  { shape = cylinder(height=12, radius=0.4), material = Wood }
    part PostB  { shape = cylinder(height=12, radius=0.4), material = Wood }
    part Rope   { shape = tube(radius=0.15), material = Rope }
    part Lamp   { thing = Lantern }
    relation {
        PostA.bottom on Ground.top shift=(-12, 0)
        PostB.bottom on Ground.top shift=(12, 0)
        Rope from PostA.top to PostB.top reach=0 sag=3
        Lamp.top on Rope.bottom
    }
    resolve grain = 0.25
}

print Line
```

No coordinates: the rope sags between the posts, and the lantern hangs
from its lowest point. Move a post and both follow. `moxi report` reads
back what was built, so a model can check its own work:

```
part          on                    center          size
Ground        —                     (0, 0, 0)       30×1×8
PostA         Ground                (-12, 6.5, 0)   0.8×12×0.8
Rope          PostA→PostB           (0, 11, 0)      24.3×3.3×0.3
Lamp.Body     Rope                  (0, 5.1, 0)     3.2×3.6×3.2
touching: Ground–PostA, Ground–PostB, PostA–Rope, PostB–Rope, Rope–Lamp.Bail, …
```

Scripts are Markdown: the code sits in ` ```moxi ` fences, so one file
is both the description and the source. The language is total (no
`while`, no recursion), so a model cannot write a script that never
finishes, and every error names what would have been valid.

## Install

```bash
cargo install moxi
```

## Use

```bash
moxi check  scripts/LANTERN.md    # errors only
moxi report scripts/LANTERN.md    # what was built: parts, sizes, contacts, what floats
moxi gltf   scripts/LANTERN.md    # → output/LANTERN.glb, one node per part
moxi mesh   scripts/LANTERN.md    # → output/world.obj + .mtl
moxi web    scripts/LANTERN.md    # → one HTML page that raymarches the scene on the GPU
```

## Learn it

- **[`SKILL.md`](SKILL.md)** is the whole language, written for a
  language model: give it to the model with your request.
- **[`scripts/`](scripts)** holds worked examples, from a mug to a posed
  character.
- **[`CHANGELOG.md`](CHANGELOG.md)** says what changed in each version.
- **[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)** is for working on the
  compiler itself.

## License

Apache-2.0
