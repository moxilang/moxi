# Hearth — using another file's things

`from "lib/FLAME.md" import Flame, material Coal` brings the flame from its
own file (DOC-20261011-imports, Issue J). The flame comes with what it needs
from that file (its `Ember`, its materials), privately: this file has its
own material called `Fire`, and the two never meet. `Coal` is imported by
name, so the hearth's stones can wear it. Each instance takes its own
arguments, and brings the flame's `Low` state with it (`Left.Low`).

In a glb the flame's materials keep their names (`Fire`, `Core`), and each
flame's root node says where it came from (`moxi.from`).

```moxi
from "lib/FLAME.md" import Flame, material Coal

material Stone { color = "#7a7570" }
material Fire  { color = "#c03010" }   # this file's own Fire: a red brick

thing Hearth {
    part Floor { shape = box(width=120, height=4, depth=50), material = Stone }
    part Back  { shape = box(width=120, height=60, depth=6), material = Fire }
    part Pit   { shape = box(width=40, height=3, depth=30), material = Coal }
    part Left  { thing = Flame(height=26) }
    part Right { thing = Flame(height=14) }
    relation {
        Back.bottom on Floor.top shift=(0, -22)
        Pit.bottom  on Floor.top shift=(-30, 0)
        Left.Bed.bottom  on Pit.top
        Right.Bed.bottom on Floor.top shift=(30, 0)
    }
}

print Hearth
```
