# TOOLKIT — sculptor primitives

Three new shapes together: a capsule limb, a torus ring, a filleted crate.

```moxi
material Skin { color = "#e0a878" }
material Gold { color = "#d4af37" }
material Wood { color = brown }

thing Toolkit {
    part Arm   { shape = capsule(height=8, radius=1.2), material = Skin }
    part Ring  { shape = torus(major_radius=3, minor_radius=0.6), material = Gold }
    part Crate { shape = box(width=4, height=4, depth=4, round=0.4), material = Wood }

    relation {
        Ring.bottom  on Arm.top
        Crate.bottom on Arm.bottom gap=6
    }

    resolve voxel_size = 1.0
}

print Toolkit detail=low
```