# terrain-001 — a small island with a sandy beach and a forest inland

Two independent top-level things (`Beach`, `Forest`) — each un-instanced,
so each becomes its own layer, satisfying `layers: >=2`. There's no
mechanism yet to site one top-level thing relative to another (Phase I,
see `world-001`/`world-002`), so this is the honest limit of what's
expressible today: a heightfield beach and a small stand of trees, not a
single sited scene.

```moxi
material Sand  { color = peach }
material Bark  { color = brown }
material Leafy { color = green }

thing Beach {
    part Ground { shape = heightfield(seed=3, radius=18, noise=0.25, max_height=4), material = Sand }
    resolve voxel_size = 1.0
}

thing PalmTree(height=8, crown=3) {
    part Trunk { shape = cylinder(height=height, radius=crown*0.2), material = Bark }
    part Crown { shape = blob(radius=crown, roughness=0.4), material = Leafy }
    relation { Crown.bottom on Trunk.top gap=-1 }
    resolve voxel_size = 1.0
}

thing Forest {
    part TreeA { thing = PalmTree }
    part TreeB { thing = PalmTree(height=6, crown=2.4) }
    relation { TreeB.west on TreeA.east gap=2 }
    resolve voxel_size = 1.0
}

print Beach detail=low
print Forest detail=low
```
