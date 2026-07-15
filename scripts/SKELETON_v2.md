entity Arm {
    part Humerus { shape = cylinder(height=9, radius=0.7), material = Bone }
    part Forearm { shape = cylinder(height=8, radius=0.6), material = Bone }
    part Hand    { shape = ellipsoid(rx=1.5, ry=2, rz=0.8), material = Bone }
    relation {
        Forearm.top on Humerus.bottom
        Hand.top    on Forearm.bottom
    }
    anchor socket = Humerus.top
    resolve voxel_size = 1.0
}

entity Skeleton {
    part Skull    { shape = sphere(radius=4), material = Bone }
    part Spine    { shape = cylinder(height=24, radius=0.8), material = Bone }
    part Ribcage  { shape = shell(ellipsoid(rx=8, ry=10, rz=6), inner_offset=1), material = Bone }
    part Pelvis   { shape = ellipsoid(rx=7, ry=4, rz=5), material = Bone }
    part RightArm { entity = Arm }
    part LeftArm  { entity = Arm }
    part RightLeg { shape = cylinder(height=16, radius=0.9), material = Bone }
    part LeftLeg  { shape = cylinder(height=16, radius=0.9), material = Bone }

    relation {
        Spine   above     Pelvis
        Ribcage surrounds Spine
        Skull   above     Ribcage
        RightArm.socket on Ribcage.surface(yaw=90,  pitch=55)
        LeftArm  symmetric_across Spine  from=RightArm
        RightLeg.top on Pelvis.surface(yaw=90,  pitch=-40)
        LeftLeg  symmetric_across Spine  from=RightLeg
    }
    resolve voxel_size = 1.0
}