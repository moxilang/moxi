# WORLD — a scene is a thing

Every layer in older scripts was a separate `print`, stacked by a viewer
convention. Here the scene is one thing: an ocean, a sand bar on it,
terrain on that, a hut placed at a named point on the terrain's surface,
and trees scattered where the ground is high enough. Everything is placed
by relation. Nothing is centered by convention.

This is bench case `world-001`: a hut sited on the island rather than
dropped at the origin.

```moxi
material Water  { color = blue }
material Sand   { color = peach }
material Grass  { color = green }
material Timber { color = brown }
material Thatch { color = "#c9a24a" }
material Leafy  { color = "#2e8b57" }

thing Ocean {
    part Disc { shape = cylinder(height=2, radius=40), material = Water }
    resolve voxel_size = 1.0
}

thing Beach {
    part Bar { shape = cylinder(height=1, radius=28), material = Sand }
    resolve voxel_size = 1.0
}

thing Terrain {
    part Ground { shape = heightfield(radius=24, max_height=12, noise=0.3, seed=7), material = Grass }
    anchor ground = Ground.surface
    resolve voxel_size = 1.0
}

thing Cabin {
    part Body { shape = box(width=6, height=4, depth=6), material = Timber }
    part Roof { shape = cone(height=3, radius=4.5),     material = Thatch }
    relation { Roof.base on Body.top }
    resolve voxel_size = 1.0
}

thing PalmTree {
    part Trunk { shape = cylinder(height=6, radius=0.6),  material = Timber }
    part Crown { shape = blob(radius=3, roughness=0.4),   material = Leafy }
    relation { Crown.bottom on Trunk.top gap=-1 }
    resolve voxel_size = 1.0
}

thing World {
    part Sea  { thing = Ocean }
    part Bar  { thing = Beach }
    part Land { thing = Terrain }
    part Hut  { thing = Cabin }

    relation {
        Bar.bottom  on Sea.top
        Land.bottom on Bar.top
        Hut.bottom  on Land.ground(x=12, z=4)
    }

    resolve voxel_size = 1.0
}

generator Grove {
    scatter PalmTree
    count       = 20
    min_spacing = 4
    seed        = 3
    where       = elevation > 6
}

print World detail=low
```

The hut takes the terrain's normal at `(12, 4)`, so it tilts with the
slope. Move it by changing two numbers — relative to the terrain, never to
the world.