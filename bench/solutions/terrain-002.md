# terrain-002 — a rocky hill with boulders scattered on higher slopes only

Probes the generator `where=` predicate. Only `elevation` (world y of the
top voxel in a column) is a bound predicate variable today — no `slope`
binding exists — so "higher slopes only" is approximated as an elevation
threshold. Honest simplification, not a silent mismatch with the prompt.

```moxi
material Rock   { color = gray }
material Rubble { color = "#6b6b6b" }

thing Hill {
    part Ground { shape = heightfield(seed=5, radius=20, noise=0.3, max_height=10), material = Rock }
    resolve voxel_size = 1.0
}

thing Boulder {
    part Body { shape = blob(radius=1.2, roughness=0.5), material = Rubble }
    resolve voxel_size = 1.0
}

generator BoulderGen {
    scatter Boulder
    count       = 20
    min_spacing = 3
    seed        = 9
    where       = elevation > 5
}

print Hill detail=low
```
