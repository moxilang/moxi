# Grove — nested composition demo
PalmTree is an entity; Grove instances it three times, side by side.
Nesting recurses for free: an Orchard could `part G1 { entity = Grove }`
and everything prefixes one more level.

Compile with:  cargo run --features viewer -- view scripts/GROVE.md

# Design notes
SIDE-BY-SIDE VIA HIP SOCKETS: instances expose ONLY their exported
anchors in Phase A — `Left left_of Center` would desugar to east/west
anchors that instances don't have (yet). So PalmTree exports lateral
`east_hip`/`west_hip` sockets low on the trunk, and the grove mates
hip-to-hip. Opposed horizontal radials keep every tree upright, and
the rotation stays a quarter-turn — phase-1 realizable.
Instance compass defaults (center/top/east/… derived from solved
instance extents) are the first item of Phase A.2.

GAP: 6 world units of daylight between trunk hips = tree spacing.

```moxi
# Atoms & materials

atom TRUNK { color = brown }
atom LEAF  { color = green }

material Bark   { color = brown, voxel_atom = TRUNK }
material Leaves { color = green, voxel_atom = LEAF }

# Palm tree — the reusable unit

entity PalmTree {
    part Trunk { shape = cylinder(height=6, radius=0.6), material = Bark }
    part Crown { shape = blob(radius=3, roughness=0.35), material = Leaves }

    relation {
        Crown above Trunk
    }

    anchor base     = Trunk.bottom
    anchor east_hip = Trunk.side(t=0.1, angle=90)
    anchor west_hip = Trunk.side(t=0.1, angle=270)

    resolve voxel_size = 1.0
}

# Grove — three instances, mated hip to hip

entity Grove {
    part Center { entity = PalmTree }
    part Left   { entity = PalmTree }
    part Right  { entity = PalmTree }

    relation {
        Left.east_hip  on Center.west_hip gap=6
        Right.west_hip on Center.east_hip gap=6
    }

    resolve voxel_size = 1.0
}

# Output

print Grove detail=low
```