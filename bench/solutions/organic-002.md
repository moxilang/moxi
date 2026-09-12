# organic-002 — a mushroom with a wide domed cap on a thin stalk

A thin `cylinder` stalk with a flattened `ellipsoid` cap sitting low on it,
so the cap reads as a dome rather than a full sphere.

```moxi
material Stalk { color = ivory }
material Cap   { color = "#a0522d" }

thing Mushroom {
    part Stalk { shape = cylinder(height=6, radius=0.8), material = Stalk }
    part Cap   { shape = ellipsoid(rx=5, ry=2.2, rz=5),  material = Cap }
    relation { Cap.bottom on Stalk.top gap=-1 }
    resolve voxel_size = 0.5
}

print Mushroom detail=low
```
