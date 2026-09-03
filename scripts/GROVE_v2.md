# GROVE v2 — instance compass anchors (requires A.2)

The v1 grove needed hand-written `anchor` exports on PalmTree before
instances could be mated. A.2 removes that: every instance answers the
universal compass (top/bottom/north/south/east/west/center) computed
from its SOLVED ASSEMBLY extents — the whole tree's bounding box, trunk
and crown together. `Middle.west on Left.east` just works. Note this
thing exports nothing.

```moxi

material Bark  { color = brown }
material Leafy { color = green }

# This is where an object gets defined
thing PalmTree {
    part Trunk { shape = cylinder(height=6, radius=0.6), material = Bark }
    part Crown { shape = blob(radius=3, roughness=0.4), material = Leafy }

    relation {
        Crown.bottom on Trunk.top gap=-1
    }

    # No `anchor` exports — the compass comes free.
    resolve voxel_size = 1.0
}

thing Grove {
    part Left   { thing = PalmTree }
    part Middle { thing = PalmTree }
    part Right  { thing = PalmTree }

    relation {
        # Assembly-box compass anchors: each tree's west face mates the
        # previous tree's east face. Identical assemblies mate level, so
        # all trunks share the ground plane.
        Middle.west on Left.east gap=1
        Right.west on Middle.east gap=1
    }

    resolve voxel_size = 1.0
}

print Grove detail=low
```