# Flame — a library file

A flame other scripts import (`scripts/HEARTH.md`). It prints itself, so it
checks and renders on its own; when another file imports it, its `print` is
ignored. `Ember` and the materials are private to this file unless an
importer names them (`material Coal`).

```moxi
material Fire { color = "#ff7a22", emissive = 1.6, opacity = 0.55, blend = additive }
material Core { color = "#fff2b0", emissive = 2.4 }
material Coal { color = "#2a1a14", roughness = 0.95 }

thing Ember(r=2) {
    part Glow { shape = sphere(radius=r), material = Core }
}

thing Flame(height=20) {
    part Bed   { shape = cylinder(height=2, radius=height / 4), material = Coal }
    part Tongue { shape = cone(height=height, radius=height / 5), material = Fire }
    part Heart  { thing = Ember(r=height / 10) }
    relation {
        Tongue.bottom on Bed.top
        Heart.Glow.bottom on Bed.top
    }
    state Low { Fire.opacity = 0.2  Core.emissive = 0.6 }
}

print Flame
```
