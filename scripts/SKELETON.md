# Human Skeleton
> Low-detail anatomical skeleton built from semantic parts and spatial relations.
> Compile with:  moxi compile scripts/skeleton.md

# Atoms

atom BONE    { color = ivory }
atom MUSCLE  { color = red }
atom VISCERA { color = maroon }

# Materials

material Bone   { color = ivory,  voxel_atom = BONE }
material Muscle { color = red,    voxel_atom = MUSCLE }
material Organ  { color = maroon, voxel_atom = VISCERA }

# Skeleton
> Parts are stamped at origin and positioned by the relation resolver.
> Relations read as plain English: Skull above Ribcage, Pelvis below Spine.

entity Skeleton {
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