# arch-002 — a small house

A box body with a pyramid-ish roof (approximated with a `cone`, since
there's no dedicated pyramid primitive) and a door on the front wall,
placed via `shift=` — the same attachment idiom as an eye on a face, not
layout sugar, so the door lands off-center rather than dead-center.

```moxi
material Wall  { color = ivory }
material Roof  { color = brown }
material Entry { color = black }

thing House {
    part Body     { shape = box(width=10, height=8, depth=8), material = Wall }
    part RoofCap  { shape = cone(height=4, radius=7.5), material = Roof }
    part Door     { shape = box(width=2, height=4, depth=0.5), material = Entry }

    relation {
        RoofCap.bottom on Body.top
        Door.south     on Body.north shift=(-2.0, 0.0) gap=-0.3
    }

    resolve voxel_size = 0.5
}

print House detail=low
```
