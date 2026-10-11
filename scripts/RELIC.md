# Relic — surfaces

Three materials, one colour each, three surfaces: dry bone (the default
roughness 0.9), wet blood (`roughness = 0.2`) and polished brass
(`roughness = 0.35, metalness = 1`). The `Dry` state dulls the blood and
tarnishes the brass. Surfaces reach glTF (`roughnessFactor`,
`metallicFactor`), OBJ (`Ns`) and the raymarcher's highlight; voxels have no
surface. DOC-20261011-closing-the-circle, Issue K.

```moxi
material Bone  { color = "#d8ccb0" }
material Blood { color = "#5a0b0b", roughness = 0.2 }
material Brass { color = "#b08a3c", roughness = 0.35, metalness = 1 }

thing Relic {
    part Slab  { shape = box(width=40, height=6, depth=24, round=1), material = Bone }
    part Pool  { shape = cylinder(height=0.6, radius=7), material = Blood }
    part Ring  { shape = torus(major_radius=6, minor_radius=1.2), material = Brass }
    part Skull { shape = sphere(radius=5), material = Bone }
    relation {
        Pool.bottom  on Slab.top shift=(-10, 0)
        Ring.bottom  on Slab.top shift=(10, 0)
        Skull.bottom on Slab.top shift=(-10, 0) gap=-1
    }
    state Dry { Blood.roughness = 0.9  Brass.metalness = 0.4  Brass.roughness = 0.8 }
    resolve voxel_size = 0.5
}

print Relic
```
