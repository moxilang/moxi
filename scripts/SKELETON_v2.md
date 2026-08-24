# Human Skeleton v2 — composition showcase
The Phase A script: Arm and Leg are ENTITIES, defined once and
instanced twice each. Instances attach through EXPORTED anchors
(`anchor socket = Humerus.top` inside Arm makes `RightArm.socket`
meaningful outside it), and each left limb is a MIRROR of its right —
nobody places LeftArm; it is a reflection of RightArm across the spine.

Compile with:  cargo run --features viewer -- view scripts/SKELETON_v2.md

# Design notes
QUARTER-TURNS ONLY: the phase-1 voxel backend realizes exactly the 24
axis-aligned orientations. twist/pitch in multiples of 90 stay inside
that group. `twist=-90 pitch=90` on an east socket makes the mated bone
hang straight DOWN — that is how both limb pairs attach here. (The old
draft used surface(yaw=90, pitch=55), a rotation the backend correctly
refuses; Phase B's containment-function stamper lifts the restriction.)

ROOT SOCKET RULE: when an instance is the SUBJECT of a placement, its
anchor must live on the instance's root part (the part with no internal
placement). Arm's root is Humerus, Leg's root is Femur — so `socket`
and `hip` are exported from those.

GAP: world units along the socket normal. gap=1 floats the shoulder a
voxel off the ribcage; gap=-3 tucks the hip 3 units INTO the pelvis so
the legs hang under the body instead of off its widest edge.

FEET: the feet splay sideways because their long axis rides along with
the leg's quarter-turn. Anatomically casual, deterministically correct.

```moxi
# Atoms & materials


material Bone { color = ivory }

# Arm — defined once, used twice
> Internal chain hangs Forearm off Humerus and Hand off Forearm.
> `anchor socket` exports the shoulder end for the outside world.

entity Arm {
    part Humerus { shape = cylinder(height=9, radius=0.8),    material = Bone }
    part Forearm { shape = cylinder(height=8, radius=0.7),    material = Bone }
    part Hand    { shape = ellipsoid(rx=1.2, ry=2.2, rz=0.9), material = Bone }

    relation {
        Forearm.top on Humerus.bottom
        Hand.top    on Forearm.bottom
    }

    anchor socket = Humerus.top

    resolve voxel_size = 1.0
}

# Leg

entity Leg {
    part Femur { shape = cylinder(height=11, radius=0.9),   material = Bone }
    part Shin  { shape = cylinder(height=10, radius=0.8),   material = Bone }
    part Foot  { shape = ellipsoid(rx=1.3, ry=1.0, rz=2.8), material = Bone }

    relation {
        Shin.top on Femur.bottom
        Foot.top on Shin.bottom
    }

    anchor hip = Femur.top

    resolve voxel_size = 1.0
}

# Skeleton
> Axial column is plain parts; all four limbs are instances.
> The constraint is a CHECK on the solved frames, not a placement —
> break the Neck relation and the compile aborts with expected vs actual.

entity Skeleton {
    part Skull    { shape = sphere(radius=4),                                    material = Bone }
    part Neck     { shape = cylinder(height=3, radius=0.9),                      material = Bone }
    part Spine    { shape = cylinder(height=22, radius=0.8),                     material = Bone }
    part Ribcage  { shape = shell(ellipsoid(rx=8, ry=10, rz=6), inner_offset=1), material = Bone }
    part Pelvis   { shape = ellipsoid(rx=7, ry=4, rz=5),                         material = Bone }
    part RightArm { entity = Arm }
    part LeftArm  { entity = Arm }
    part RightLeg { entity = Leg }
    part LeftLeg  { entity = Leg }

    relation {
        Spine   above     Pelvis
        Ribcage surrounds Spine
        Neck    above     Ribcage
        Skull   above     Neck
        RightArm.socket on Ribcage.east twist=-90 pitch=90 gap=1
        LeftArm  symmetric_across Spine from=RightArm
        RightLeg.hip on Pelvis.east twist=-90 pitch=90 gap=-3
        LeftLeg  symmetric_across Spine from=RightLeg
    }

    constraint Skull above Ribcage

    resolve voxel_size = 1.0
}

# Output

print Skeleton detail=low
```