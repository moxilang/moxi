# arch-001 — a simple stone archway

Two upright columns with a flat lintel across the top. Columns are placed
via the universal `point(x=,y=,z=,free=1)` escape hatch relative to
`ColumnR` (position-only, so each keeps its own default upright
orientation) — the whole assembly is exactly mirror-symmetric about the
midpoint between the two columns.

```moxi
material Stone { color = gray }

thing Archway {
    part ColumnR { shape = cylinder(height=10, radius=1.2), material = Stone }
    part ColumnL { shape = cylinder(height=10, radius=1.2), material = Stone }
    part Lintel  { shape = box(width=14, height=2, depth=3), material = Stone }

    relation {
        ColumnL.bottom on ColumnR.point(x=-10, y=0, z=0, free=1)
        Lintel.bottom  on ColumnR.point(x=-5,  y=10, z=0, free=1)
    }

    resolve voxel_size = 0.5
}

print Archway detail=low
```
