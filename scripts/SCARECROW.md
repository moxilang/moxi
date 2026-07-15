# Scarecrow
> First script using explicit anchor mates and bilateral mirroring.
> ArmR attaches to the post's side socket — the mate rotates it to point
> radially outward (a quarter-turn: realizable by the phase-1 voxel
> backend). ArmL is its mirror image across the post, axis=x by default.

atom WOOD  { color = brown }
atom STRAW { color = yellow }

material Wood  { color = brown,  voxel_atom = WOOD }
material Straw { color = yellow, voxel_atom = STRAW }

entity Scarecrow {
    part Post { shape = cylinder(height=14, radius=0.8), material = Wood }
    part Head { shape = sphere(radius=3),                material = Straw }
    part ArmR { shape = cylinder(height=6, radius=0.5),  material = Wood }
    part ArmL { shape = cylinder(height=6, radius=0.5),  material = Wood }

    relation {
        Head above Post
        ArmR.bottom on Post.side(t=0.75, angle=90)
        ArmL symmetric_across Post from=ArmR
    }

    resolve voxel_size = 1.0
}

print Scarecrow detail=low