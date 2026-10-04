# Desk lamp — poses

The living-models demo (DOC-20261004-living-models-design, step 2). A pose is
the thing with some mate qualifiers set to other values, solved again: what
`pose Reading` says is exactly what you would get by writing its numbers into
the mate lines. `moxi scene --pose Reading` and `moxi gltf --pose Reading`
print it; every pose is solved and its constraints checked on every compile,
so `constraint Shade above Base` holds in all of them.

`Desk` shows the two other rules: an instance takes its thing's pose by name
(`Left pose=Folded`), and a `symmetric_across` image follows its source — the
right lamp is posed through the left one.

```moxi
material Iron   { color = "#2b2b2b" }
material Enamel { color = "#c8442c" }
material Glow   { color = "#fff2b0" }
material Oak    { color = "#9a6b3f" }

thing DeskLamp(upper=60, fore=55) {
    part Base  { shape = cylinder(height=6, radius=26), material = Iron }
    part Upper { shape = capsule(height=upper, radius=2.5), material = Iron }
    part Fore  { shape = capsule(height=fore, radius=2.2), material = Iron }
    part Shade { shape = cone(height=22, radius=16), material = Enamel }
    part Bulb  { shape = sphere(radius=6), material = Glow }

    relation {
        Upper.bottom on Base.top lean=(15, 0)
        Fore.bottom  on Upper.top lean=(-80, 0)
        Shade.top    on Fore.top lean=(-60, 0)
        Bulb.center  on Shade.bottom gap=-6
    }

    anchor foot = Base.bottom

    pose Reading { Upper lean=(25, 0)  Fore lean=(-50, 0)  Shade lean=(-80, 0) }
    pose Folded  { Upper lean=(5, 0)   Fore lean=(-150, 0) Shade lean=(-20, 0) }
    pose Craning { Upper lean=(40, 0)  Fore lean=(-20, 0)  Shade lean=(-100, 0) }

    constraint Shade above Base

    resolve voxel_size = 1
}

thing Desk {
    part Top   { shape = box(width=200, height=6, depth=90), material = Oak }
    part Left  { thing = DeskLamp }
    part Right { thing = DeskLamp }

    relation {
        Left.foot on Top.top shift=(-60, 0)
        Right symmetric_across Top from=Left
    }

    pose Tidy  { Left pose=Folded }
    pose Study { Left pose=Reading }

    resolve voxel_size = 1
}

print Desk detail=low
```
