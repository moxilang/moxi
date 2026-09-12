# organic-003 — a bare deciduous tree in winter

Trunk with four branches radiating from `side(t, angle)` around the trunk,
tilted upward with mate `pitch`. Pre-P1 this should pass — branches want
radial placement, which `side()` already gives, no `shift=` needed.

```moxi
material Bark { color = brown }

thing Tree {
    part Trunk { shape = cylinder(height=14, radius=0.9), material = Bark }
    part BranchN { shape = cylinder(height=6, radius=0.35), material = Bark }
    part BranchE { shape = cylinder(height=6, radius=0.35), material = Bark }
    part BranchS { shape = cylinder(height=6, radius=0.35), material = Bark }
    part BranchW { shape = cylinder(height=6, radius=0.35), material = Bark }

    relation {
        BranchN.bottom on Trunk.side(t=0.6, angle=0)   pitch=-55
        BranchE.bottom on Trunk.side(t=0.7, angle=90)  pitch=-55
        BranchS.bottom on Trunk.side(t=0.8, angle=180) pitch=-55
        BranchW.bottom on Trunk.side(t=0.9, angle=270) pitch=-55
    }

    resolve voxel_size = 0.5
}

print Tree detail=low
```
