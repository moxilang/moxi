# Human Skeleton
Low-detail anatomical skeleton built from semantic parts and spatial relations.
Compile with:  moxi compile scripts/skeleton.md

```moxi
# Atoms


# Materials

material Bone   { color = ivory }
material Muscle { color = red }
material Organ  { color = maroon }

# Skeleton
> Parts are stamped at origin and positioned by the relation resolver.
> Relations read as plain English: Skull above Ribcage, Pelvis below Spine.

thing Skeleton {
    part Skull   { shape = sphere(radius=4),                                    material = Bone }
    part Spine   { shape = cylinder(height=24, radius=0.8),                     material = Bone }
    part Ribcage { shape = shell(ellipsoid(rx=8, ry=10, rz=6), inner_offset=1), material = Bone }
    part Pelvis  { shape = ellipsoid(rx=7, ry=4, rz=5),                         material = Bone }

    relation {
        Spine   above    Pelvis
        Ribcage surrounds Spine
        Skull   above    Ribcage
    }

    constraint Skull above Ribcage

    resolve voxel_size = 1.0
}

# Output

print Skeleton detail=low
```