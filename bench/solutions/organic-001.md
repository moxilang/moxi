# organic-001 — a palm tree on a small patch of sand

Reference solution for the `organic-001` bench case. A trunk (`cylinder`)
with a crown (`blob`, so the canopy silhouette isn't a perfect sphere),
standing on a shallow sand patch.

```moxi
material Bark { color = brown }
material Leafy { color = green }
material Sand { color = peach }

thing PalmTree(height=10, crown=4) {
    part Trunk { shape = cylinder(height=height, radius=crown*0.2), material = Bark }
    part Crown { shape = blob(radius=crown, roughness=0.4), material = Leafy }
    relation { Crown.bottom on Trunk.top gap=-1 }
    resolve voxel_size = 1.0
}

thing Scene {
    part Ground { shape = box(width=14, height=1, depth=14), material = Sand }
    part Tree   { thing = PalmTree }
    relation { Tree.bottom on Ground.top }
    resolve voxel_size = 1.0
}

print Scene detail=low
```
