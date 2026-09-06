# Moxi-Boi Eating a Purple Plum

The mascot raises a ripe plum to his open mouth.

Rewritten for the current architecture:

**No guide parts.** The old version had invisible `FaceGuide` and
`LegGuide` boxes whose only job was to give `left_of` and `right_of`
something to sit beside, because every mate landed dead-centre on its
socket. `shift=(a, b)` slides within the socket's tangent plane, so the
eyes attach to the face itself and the legs to the hips.

**One blended body.** The torso is a single `union(..., blend=)` — hips
into belly into chest into neck, fused rather than stacked.

**Capsule limbs, and they are thick.** A sphere-swept segment is a better
arm than a chain of ellipsoids, and `Arm(thick=)` is the one number that
makes him buff.

**Note on posing.** A mate's `pitch` tilts *off the socket normal*, so an
arm on a horizontal (±X) shoulder socket swings up or down from
horizontal, not from vertical. And on a socket whose normal is straight
down, the tangent basis is degenerate, so `twist` must be given
explicitly or the part's rotation about that axis is arbitrary — which is
what made the legs splay sideways before.

```moxi
material Body  { color = ivory }
material Black { color = black }
material White { color = white }
material Cheek { color = "#FF46A2" }
material Plum  { color = purple }
material Stem  { color = brown }
material Leaf  { color = green }

# Gorilla proportions: the upper arm is nearly as thick as it is long, and
# the forearm is thicker still — this is where the silhouette comes from.
thing Arm(upper=6, lower=5.5, thick=4.2, bend=0) {
    part Upper { shape = capsule(height=upper, radius=thick),        material = Body }
    part Lower { shape = capsule(height=lower, radius=thick*1.05),   material = Body }
    part Hand  { shape = sphere(radius=thick*1.1),                   material = Body }

    # `pitch` tilts about the socket's tangent X. On a downward-hanging
    # capsule's `bottom` that axis runs backward, so a POSITIVE bend
    # folds the forearm behind him — negate it to curl forward.
    relation {
        Lower.top on Upper.bottom pitch=0-bend
        Hand.top  on Lower.bottom
    }

    anchor socket = Upper.top
    # The hand's forward face, not its top: a capsule's local +Y runs up
    # the limb, so `Hand.top` is the wrist end and points back up the arm.
    anchor grip   = Hand.north
    resolve voxel_size = 0.75
}

# Short and thick: in the reference the legs are stubby next to the mass
# above them, not the long limbs of a humanoid.
thing Leg(thigh=4.5, shin=4, thick=3.4) {
    part Thigh { shape = capsule(height=thigh, radius=thick),        material = Body }
    part Shin  { shape = capsule(height=shin,  radius=thick*0.92),   material = Body }
    part Foot  { shape = box(width=5.5, height=2.5, depth=7.5, round=1.0), material = Body }

    relation {
        Shin.top on Thigh.bottom
        Foot.top on Shin.bottom
    }

    anchor hip = Thigh.top
    resolve voxel_size = 0.75
}

thing MoxiBoiEatingPlum {
    # A wedge, not a stack: narrow hips widening into an enormous chest.
    # The neck is a stub — in the reference the head sits straight on the
    # shoulders. Sections: hips 0, waist 3, chest 8, stub 12–13.
    part Torso {
        shape = union(
            ellipsoid(rx=4.5, ry=3, rz=3.5),
            at(box(width=8, height=5, depth=5.5, round=2), y=3),
            at(ellipsoid(rx=11, ry=5.5, rz=6), y=8),
            at(cylinder(height=1.5, radius=3.5), y=12),
            blend=2.6
        ),
        material = Body
    }

    # Small head, big body — the contrast is the character.
    part Head {
        shape = box(width=9, height=7.5, depth=6.5, round=2.0),
        material = Body
    }

    part LeftEye    { shape = sphere(radius=1.5),                 material = Black }
    part RightEye   { shape = sphere(radius=1.5),                 material = Black }
    part LeftShine  { shape = sphere(radius=0.45),                material = White }
    part RightShine { shape = sphere(radius=0.45),                material = White }
    part Mouth      { shape = ellipsoid(rx=1.6, ry=2.0, rz=0.7),  material = Black }
    part LeftCheek  { shape = ellipsoid(rx=1.2, ry=0.55, rz=0.4), material = Cheek }
    part RightCheek { shape = ellipsoid(rx=1.2, ry=0.55, rz=0.4), material = Cheek }

    part PlumFruit { shape = sphere(radius=2.1),                 material = Plum }
    part PlumStem  { shape = cylinder(height=1.4, radius=0.22),  material = Stem }
    part PlumLeaf  { shape = ellipsoid(rx=1.0, ry=0.3, rz=0.55), material = Leaf }

    # The eating arm folds hard at the elbow; the other hangs relaxed.
    part RightArm { thing = Arm(bend=115) }
    part LeftArm  { thing = Arm(bend=20) }
    part RightLeg { thing = Leg }
    part LeftLeg  { thing = Leg }

    relation {
        # The neck stub runs y=12 to 13.5, so this is its cap. If the
        # torso's sections move, this number must move with them — the
        # cost of hand-written coordinates, and the reason the union
        # compass-anchor fix is the next task.
        Head.bottom on Torso.point(x=0, y=13.5, z=0, nx=0, ny=1, nz=0)

        LeftEye.south  on Head.north shift=(1.5, -2.6) gap=-0.9
        RightEye.south on Head.north shift=(1.5,  2.6) gap=-0.9
        LeftShine.south  on LeftEye.north  gap=-0.7
        RightShine.south on RightEye.north gap=-0.7

        Mouth.south      on Head.north shift=(-2.2, 0.0) gap=-0.6
        LeftCheek.south  on Head.north shift=(-1.2, -3.4) gap=-0.4
        RightCheek.south on Head.north shift=(-1.2,  3.4) gap=-0.4

        # Shoulders wide and high on the chest, arms hanging OUTSIDE the
        # torso rather than against it — the gorilla silhouette.
        RightArm.socket on Torso.point(x=10, y=10, z=0, nx=0.45, ny=-0.89, nz=0) twist=0 pitch=-20
        LeftArm.socket  on Torso.point(x=-10, y=10, z=0, nx=-0.45, ny=-0.89, nz=0) twist=0 pitch=12

        # The Foot box is 5.5 wide by 7.5 deep, so a correctly oriented
        # foot is longer front-to-back. 0 and 180 both put the long axis
        # across, so the answer is a quarter turn: 270 (90 was wrong the
        # other way).
        RightLeg.hip on Torso.point(x=0, y=-2.5, z=-3.0, nx=0, ny=-1, nz=0) twist=270
        LeftLeg.hip  on Torso.point(x=0, y=-2.5, z=3.0,  nx=0, ny=-1, nz=0) twist=270

        # The plum rests in the folded hand, right at the mouth.
        PlumFruit.south on RightArm.grip gap=0.3
        PlumStem.bottom  on PlumFruit.top
        PlumLeaf.west    on PlumStem.east
    }

    constraint Head above Torso

    resolve voxel_size = 0.75
}

print MoxiBoiEatingPlum detail=low
```

The eyes are two numbers each, relative to the face; the arm pose is one
number, `bend`. Change the head's shape and the features follow it,
because they are attached to the face rather than to a coordinate.