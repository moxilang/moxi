# Tropical Island World

A procedural island with palm trees, beach, rocky peaks, and ocean.
Compile with: `moxi compile scripts/ISLAND.md`

## Design notes

**A scene is a thing.** The island is one `World` thing whose parts are
the ocean, beach, terrain and peaks, each placed by relation. Earlier
versions printed five separate things and relied on a viewer convention to
stack them; nothing aligned them, they merely happened to be centered at
the same origin. Now the sand sits *on* the ocean because a relation says
so.

**Layer order is part order.** Within a thing, later parts overwrite
earlier ones where they overlap, so declaring Sea, Shore, Land, Peaks in
that order gives exactly the old bottom-to-top result. This is why Land
and Peaks are both mated to the beach top rather than to each other: they
are coincident heightfields that interpenetrate on purpose, and the later
declaration wins at the summit.

**Determinism.** Use `cylinder` for flat layers. Heightfield noise gives
ragged edges that vary with floating-point rounding at grid boundaries even
at the same seed, so ocean and beach must never be heightfields.

**Beach ring width** = sand radius (55) − soil radius (40) = 15 voxels.

**Rocky peaks** share SoilTerrain's seed so the noise patterns align
spatially. Larger `noise` (0.6 against 0.35) makes them jagged.

**Elevation is measured on the whole world**, not on the terrain alone.
The terrain now rests on 2 voxels of ocean and 2 of sand, so heights in a
generator's `where` are about 2 higher than in the multi-print version:
`elevation > 5` here is the same ground as `elevation > 3` was there.

## Materials

Two atoms share the brown color but stay semantically distinct — useful
for later material logic. Everything else is a self-contained material.

```moxi
atom SOIL  { color = brown }
atom TRUNK { color = brown }

material Soil   { color = brown,  voxel_atom = SOIL }
material Bark   { color = brown,  voxel_atom = TRUNK }
material Sand   { color = yellow }
material Rock   { color = gray }
material Ocean  { color = blue }
material Leaves { color = green }
```

## Palm tree

`Crown above Trunk` puts the blob canopy on the cylinder's top; the solver
computes the offset from the trunk's height.

```moxi
thing PalmTree {
    part Trunk { shape = cylinder(height=6, radius=0.6), material = Bark }
    part Crown { shape = blob(radius=3, roughness=0.35), material = Leaves }
    relation {
        Crown above Trunk
    }
    resolve voxel_size = 1.0
}
```

## The pieces

Flat, deterministic discs for water and sand; heightfields for the
landmass and the summit.

```moxi
thing Ocean {
    part Water { shape = cylinder(height=1, radius=200), material = Ocean }
    resolve voxel_size = 1.0
}

thing SandBase {
    part Shore { shape = cylinder(height=1, radius=55), material = Sand }
    resolve voxel_size = 1.0
}

thing SoilTerrain {
    part Body { shape = heightfield(seed=42, radius=40, noise=0.35, max_height=18), material = Soil }
    resolve voxel_size = 1.0
}

thing RockyPeaks {
    part Crags { shape = heightfield(seed=42, radius=35, noise=0.6, max_height=20), material = Rock }
    resolve voxel_size = 1.0
}
```

## The world

Four instances, three relations. Each part is the subject of exactly one
placement; Land and Peaks both rise from the beach, so they share an
origin and the later declaration paints over the earlier at the summit.

```moxi
thing World {
    part Sea   { thing = Ocean }
    part Shore { thing = SandBase }
    part Land  { thing = SoilTerrain }
    part Peaks { thing = RockyPeaks }

    relation {
        Shore.bottom on Sea.top
        Land.bottom  on Shore.top
        Peaks.bottom on Shore.top
    }

    resolve voxel_size = 1.0
}
```

## Generators

Scatter over the printed world's surface. `min_spacing` keeps instances
apart; changing `seed` gives a different pattern at the same density.

```moxi
generator ForestGen {
    scatter PalmTree
    count       = 60
    min_spacing = 5
    seed        = 7
    where       = elevation > 5 and elevation < 15
}

generator BeachGen {
    scatter PalmTree
    count       = 10
    min_spacing = 7
    seed        = 99
    where       = elevation > 3 and elevation < 5
}
```

## Output

One world, one print. `PalmTree` is a generator target and is never a
layer, so it is not printed — the old script listed it, which did nothing.

```moxi
print World detail=low
```