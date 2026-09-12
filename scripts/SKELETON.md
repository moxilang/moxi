# Human Skeleton

The current-idiom skeleton. Three earlier generations sit beside it and
show how the language got here:

| file | what it demonstrated |
|---|---|
| `SKELETON_v1.md` | flat parts, relation sugar only (`above`, `surrounds`) — no composition, no limbs |
| `SKELETON_v2.md` | reusable `thing`s, exported anchors, `symmetric_across` — but quarter-turn poses only |
| `SKELETON_v3.md` | arbitrary limb angles once B1 landed (`pitch=70`, `surface(yaw, pitch)`) |

What this version adds on top of v3:

**Capsule bones.** A sphere-swept segment has rounded ends, so joints read
as joints. v2 and v3 used bare cylinders, whose flat caps meet at a hard
disc wherever two bones touch.

**Limbs posed by one number.** `Arm(bend=)` and `Leg(bend=)` are
parameters, and a parameter can drive a mate qualifier directly
(`pitch=0-bend`). The elbow and knee angles are the entity's interface,
not numbers buried in its relation block.

**Shoulders that sit on the shoulders.** This is the `shift=` payoff.
`Ribcage.east` is a single point at the ribcage's widest place — its
vertical middle — so v2 and v3 both grew arms out of the torso's midline,
at armpit height. `shift=(6.0, -1.5)` slides the socket 6 units up its own
tangent plane and 1.5 forward, without touching the pose. An anchor is a
patch now, not a point.

**A fused pelvis, and both halves of the union anchor rule.** The pelvis
is `union(..., blend=)` — the iliac bowl fused into the sacrum rather than
stacked on it. It exercises the two halves of how a union answers anchors,
in one part:

- `Pelvis.top` comes from the union's **own** extents, so it lands on top
  of the sacrum (y = 6.2), where the spine belongs. Delegating to the
  first operand would have put it at y = 3.2 — inside the body, and the
  spine would sink into the hips.
- `Pelvis.surface(yaw, pitch)` is shape-specific, so it still delegates to
  the first operand, the iliac ellipsoid. That is exactly what the hip
  sockets want: a point on the bowl, not on the fused silhouette.

**CSG detail.** The skull carries its orbits as `difference` cuts, and the
foot is a rounded box. Anchors pass through both — `Skull.bottom` still
answers from the base sphere.

The ribcage stays a `shell(ellipsoid)`. Individual ribs are a loop, and
loops are Phase E; hand-writing twelve of them would be worse than
honest shorthand.

```moxi
material Bone { color = ivory }

thing Arm(upper=9, lower=8, thick=0.85, bend=14) {
    part Humerus { shape = capsule(height=upper, radius=thick),     material = Bone }
    part Ulna    { shape = capsule(height=lower, radius=thick*0.9), material = Bone }
    part Hand    { shape = ellipsoid(rx=1.1, ry=2.0, rz=0.8),       material = Bone }

    relation {
        Ulna.top on Humerus.bottom pitch=0-bend
        Hand.top on Ulna.bottom
    }

    anchor socket = Humerus.top
    resolve voxel_size = 1.0
}

thing Leg(femur=11, shin=10, thick=0.95, bend=6) {
    part Femur { shape = capsule(height=femur, radius=thick),      material = Bone }
    part Shin  { shape = capsule(height=shin,  radius=thick*0.88), material = Bone }
    part Foot  { shape = box(width=2.6, height=1.2, depth=5.0, round=0.5), material = Bone }

    relation {
        Shin.top on Femur.bottom pitch=bend
        Foot.top on Shin.bottom
    }

    anchor hip = Femur.top
    resolve voxel_size = 1.0
}

thing Skeleton {
    # Iliac bowl fused into the sacrum. `Pelvis.top` is the sacrum's top
    # because a union's compass anchors read the whole fused body.
    part Pelvis {
        shape = union(
            ellipsoid(rx=7, ry=3.2, rz=4.5),
            at(ellipsoid(rx=3.2, ry=3.4, rz=3.0), y=2.8),
            blend=1.8
        ),
        material = Bone
    }

    part Spine   { shape = capsule(height=20, radius=0.8), material = Bone }
    part Ribcage { shape = shell(ellipsoid(rx=8, ry=10, rz=6), inner_offset=1), material = Bone }
    part Neck    { shape = capsule(height=2.4, radius=0.7), material = Bone }

    # Orbits cut straight out of the cranium; the difference still answers
    # `bottom` from its base sphere, so the neck mate is unaffected.
    part Skull {
        shape = difference(
            sphere(radius=4),
            at(sphere(radius=1.2), x=-1.8, y=0.4, z=3.0),
            at(sphere(radius=1.2), x=1.8,  y=0.4, z=3.0)
        ),
        material = Bone
    }

    part RightArm { thing = Arm }
    part LeftArm  { thing = Arm }
    part RightLeg { thing = Leg }
    part LeftLeg  { thing = Leg }

    relation {
        Spine.bottom   on Pelvis.top
        Ribcage.center on Spine.side(t=0.62, angle=0)
        Neck.bottom    on Spine.top
        Skull.bottom   on Neck.top

        # `shift` rides the socket's tangent plane: +X is up the ribcage,
        # +Z runs backward, so (6.0, -1.5) is high and slightly forward.
        # The pose itself is v3's, untouched — shift only moves where it
        # happens.
        RightArm.socket on Ribcage.east shift=(6.0, -1.5) twist=-90 pitch=70 gap=1

        # Hips aim down and out from the iliac bowl, not the fused hull.
        RightLeg.hip on Pelvis.surface(yaw=90, pitch=-70)

        # Mirroring composes with shift for free — nobody places the left
        # side; it is a reflection of the solved right-hand frame.
        LeftArm symmetric_across Spine from=RightArm
        LeftLeg symmetric_across Spine from=RightLeg
    }

    constraint Skull above Ribcage

    resolve voxel_size = 1.0
}

print Skeleton detail=low
```
