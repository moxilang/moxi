# SKELETON v3 — posed limbs from angled sockets (requires Phase B1)

The Phase B1 showcase: arbitrary limb angles. Legs pose from angled
socket normals alone (`surface(yaw, pitch)`); arms pose from an
arbitrary mate pitch (70°). Either way, on the Phase-A backend this
script fails with NonAxisAlignedRotation; on B1 it just renders.

```moxi

material Bone { color = ivory }

## Reusable limbs (same as v2)

entity Arm {
    part Humerus { shape = cylinder(height=9, radius=0.8), material = Bone }
    part Forearm { shape = cylinder(height=8, radius=0.7), material = Bone }
    part Hand    { shape = ellipsoid(rx=1.2, ry=2.2, rz=0.9), material = Bone }

    relation {
        Forearm.top on Humerus.bottom
        Hand.top on Forearm.bottom
    }

    anchor socket = Humerus.top
    resolve voxel_size = 1.0
}

entity Leg {
    part Femur { shape = cylinder(height=11, radius=0.9), material = Bone }
    part Shin  { shape = cylinder(height=10, radius=0.8), material = Bone }
    part Foot  { shape = ellipsoid(rx=1.3, ry=1.0, rz=2.8), material = Bone }

    relation {
        Shin.top on Femur.bottom
        Foot.top on Shin.bottom
    }

    anchor hip = Femur.top
    resolve voxel_size = 1.0
}

## The skeleton

entity Skeleton {
    # Axial column
    part Skull   { shape = sphere(radius=4), material = Bone }
    part Neck    { shape = cylinder(height=3, radius=0.7), material = Bone }
    part Spine   { shape = cylinder(height=22, radius=0.8), material = Bone }
    part Ribcage { shape = shell(ellipsoid(rx=8, ry=10, rz=6), inner_offset=1), material = Bone }
    part Pelvis  { shape = ellipsoid(rx=7, ry=4, rz=5), material = Bone }

    # Limbs as instances
    part RightArm { entity = Arm }
    part LeftArm  { entity = Arm }
    part RightLeg { entity = Leg }
    part LeftLeg  { entity = Leg }

    relation {
        # Axial chain
        Ribcage.center on Spine.side(t=0.7, angle=0)
        Pelvis.center on Spine.side(t=0.12, angle=0)
        Neck.bottom on Spine.top
        Skull.bottom on Neck.top

        # Shoulders: at the ribcage SIDES. (surface(pitch=-55) would slide
        # the socket 55° down toward the bottom pole — surface anchors
        # couple position and normal, which is what put arms at the hips.)
        # The hang comes from the mate instead: pitch=70 tilts the arm 70°
        # below horizontal with a 20° outward flare — an arbitrary angle
        # only the B1 backend can realize (Phase A allowed only 0/90).
        RightArm.socket on Ribcage.east twist=-90 pitch=70 gap=1

        # Hips: pelvis sockets aimed steeply down-and-out (70° below
        # equator) — legs nearly vertical with a slight stance splay.
        RightLeg.hip on Pelvis.surface(yaw=90, pitch=-70)

        # Bilateral symmetry across the spine
        LeftArm symmetric_across Spine from=RightArm
        LeftLeg symmetric_across Spine from=RightLeg
    }

    constraint Skull above Ribcage

    resolve voxel_size = 1.0
}

print Skeleton detail=low
```