# Lamp with a hat — the `moxi report` demo

`scripts/LAMP.md` plus a hat that is placed 9 above the bulb (`gap=9`) instead
of on it, so it floats. `moxi report scripts/LAMP_REPORT.md` shows what a model
should look for after a build: the hat is its own piece, and the report names
how far it is from the lamp.

```text
Lamp · voxel 3 · 5 parts · 2 pieces
…
touching: Base–Post, Post–Arm, Arm–Bulb
piece 2: Hat — 9 from Bulb
```

Two parts touch when their surfaces are within one voxel (3 here), so a hat at
`gap=2` would read as touching: a gap below the build's resolution is below
what the report can tell apart. Change `gap=9` to `gap=0` and the hat sits on
the bulb: one piece, `Bulb–Hat` in the touching line.

```moxi
material Iron  { color = "#333333" }
material Glass { color = yellow }
material Felt  { color = "#7a1f2b" }

thing Lamp(height=300, reach=90) {
    part Base { shape = cylinder(height=12, radius=45), material = Iron }
    part Post { shape = cylinder(height=height, radius=6), material = Iron }
    part Arm  { shape = capsule(height=reach, radius=4), material = Iron }
    part Bulb { shape = sphere(radius=18), material = Glass }
    part Hat  { shape = cone(height=24, radius=26), material = Felt }

    relation {
        Post.bottom on Base.top
        Arm.bottom  on Post.side(t=0.92, angle=0) lean=(20, 0)
        Bulb.center on Arm.top
        Hat.bottom  on Bulb.top gap=9
    }

    resolve voxel_size = 3
}

print Lamp detail=low
```
