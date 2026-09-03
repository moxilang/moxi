# Moxi-Boi Eating a Purple Plum
> The heroic mascot holds a ripe plum beside his open mouth.


material Body { color = ivory }
material Black { color = black }
material White { color = white }
material Cheek { color = mochi-pink }
material Plum { color = purple }
material Stem { color = brown }
material Leaf { color = green }

thing MoxiBoiEatingPlum {
    part Hips {
        shape = ellipsoid(rx=6, ry=4, rz=4),
        material = Body
    }

    part Abdomen {
        shape = box(width=10, height=9, depth=6),
        material = Body
    }

    part Chest {
        shape = ellipsoid(rx=11, ry=7, rz=6),
        material = Body
    }

    part Neck {
        shape = cylinder(height=2, radius=4),
        material = Body
    }

    part Head {
        shape = box(width=12, height=10, depth=8),
        material = Body
    }

    part FaceGuide {
        shape = box(width=2, height=1, depth=1),
        material = Body
    }

    part LeftEye {
        shape = sphere(radius=1.6),
        material = Black
    }

    part RightEye {
        shape = sphere(radius=1.6),
        material = Black
    }

    part LeftShine {
        shape = sphere(radius=0.5),
        material = White
    }

    part RightShine {
        shape = sphere(radius=0.5),
        material = White
    }

    part Mouth {
        shape = ellipsoid(rx=1.3, ry=1.6, rz=0.6),
        material = Black
    }

    part PlumFruit {
        shape = sphere(radius=2.2),
        material = Plum
    }

    part PlumStem {
        shape = cylinder(height=1.5, radius=0.25),
        material = Stem
    }

    part PlumLeaf {
        shape = ellipsoid(rx=1.1, ry=0.35, rz=0.6),
        material = Leaf
    }

    part LeftCheek {
        shape = ellipsoid(rx=1.3, ry=0.6, rz=0.4),
        material = Cheek
    }

    part RightHand {
        shape = sphere(radius=2.5),
        material = Body
    }

    part RightForearm {
        shape = ellipsoid(rx=3.3, ry=5, rz=3.3),
        material = Body
    }

    part RightUpperArm {
        shape = ellipsoid(rx=4.2, ry=5.5, rz=4.2),
        material = Body
    }

    part LeftShoulder {
        shape = sphere(radius=5),
        material = Body
    }

    part LeftUpperArm {
        shape = ellipsoid(rx=4.5, ry=6, rz=4.5),
        material = Body
    }

    part LeftForearm {
        shape = ellipsoid(rx=4.5, ry=7, rz=4.5),
        material = Body
    }

    part LeftFist {
        shape = ellipsoid(rx=4.8, ry=4, rz=4.8),
        material = Body
    }

    part LegGuide {
        shape = box(width=2, height=1, depth=3),
        material = Body
    }

    part LeftThigh {
        shape = ellipsoid(rx=3.5, ry=6, rz=4),
        material = Body
    }

    part RightThigh {
        shape = ellipsoid(rx=3.5, ry=6, rz=4),
        material = Body
    }

    part LeftLowerLeg {
        shape = box(width=5, height=6, depth=6),
        material = Body
    }

    part RightLowerLeg {
        shape = box(width=5, height=6, depth=6),
        material = Body
    }

    part LeftFoot {
        shape = box(width=6, height=3, depth=8),
        material = Body
    }

    part RightFoot {
        shape = box(width=6, height=3, depth=8),
        material = Body
    }

    relation {
        Abdomen above Hips
        Chest above Abdomen
        Neck above Chest
        Head above Neck

        FaceGuide behind Head
        LeftEye left_of FaceGuide
        RightEye right_of FaceGuide
        LeftShine behind LeftEye
        RightShine behind RightEye
        Mouth below FaceGuide
        PlumFruit right_of Mouth
        PlumStem above PlumFruit
        PlumLeaf right_of PlumStem
        LeftCheek left_of Mouth

        RightHand below PlumFruit
        RightForearm below RightHand
        RightUpperArm below RightForearm

        LeftShoulder left_of Chest
        LeftUpperArm below LeftShoulder
        LeftForearm below LeftUpperArm
        LeftFist below LeftForearm

        LegGuide below Hips
        LeftThigh left_of LegGuide
        RightThigh right_of LegGuide
        LeftLowerLeg below LeftThigh
        RightLowerLeg below RightThigh
        LeftFoot below LeftLowerLeg
        RightFoot below RightLowerLeg
    }

    constraint Head above Chest
    constraint PlumFruit right_of Mouth
    constraint RightHand below PlumFruit
    constraint LeftFist below LeftShoulder
    constraint LeftFoot below Hips
    constraint RightFoot below Hips

    resolve voxel_size = 0.75
}

print MoxiBoiEatingPlum detail=low
