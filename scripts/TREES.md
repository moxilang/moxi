# TREES — thing parameters (requires Phase C)

One PalmTree definition, three different trees. Parameters have
defaults (`thing PalmTree(height=6, crown=3)`), instances override
them (`thing = PalmTree(height=10, crown=4)`), and arguments can be
arithmetic (`radius = crown * 0.2`). The compass mates use each tree's
ACTUAL size — the tall tree's east face is where ITS crown ends.

```moxi

material Bark  { color = brown }
material Leafy { color = green }

thing PalmTree(height=6, crown=3) {
    part Trunk { shape = cylinder(height=height, radius=crown*0.2), material = Bark }
    part Crown { shape = blob(radius=crown, roughness=0.4), material = Leafy }

    relation {
        Crown.bottom on Trunk.top gap=-1
    }

    resolve voxel_size = 1.0
}

thing Grove {
    part Small { thing = PalmTree(height=4, crown=2) }
    part Mid   { thing = PalmTree }
    part Tall  { thing = PalmTree(height=10, crown=4) }

    relation {
        Mid.west on Small.east gap=1
        Tall.west on Mid.east gap=1
    }

    resolve voxel_size = 1.0
}

print Grove detail=low
```