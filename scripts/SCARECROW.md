# Scarecrow

A post, a sack head, and a hat. The original version of this script was
the first to use explicit anchor mates and bilateral mirroring —
`ArmR.bottom on Post.side(t=0.75, angle=90)` with `ArmL` a reflection of
the solved frame. Both are still here, unchanged in spirit. What is new is
everything hanging off them.

**When to use `surface()` and when to use `shift`.** These are the two ways
to put several features on one part, and picking the wrong one is the
most common attachment mistake in the language:

- A shape with a real surface parametrization — sphere, ellipsoid,
  cylinder, cone, heightfield — has infinitely many anchors already.
  The face here is four `Head.surface(yaw, pitch)` calls. Each eye gets
  its own yaw; nothing needs sliding.
- A shape with only face-centre anchors — a box, most CSG hulls — has one
  anchor per face, and `shift=(a, b)` slides within that face's tangent
  plane. `MOXIBOI.md` uses it, because its head is a rounded box.

Reach for `shift` when the anchor you need does not exist. Reach for
`surface()` when it does.

**A hat is two shapes and one mate.** `torus` for the brim, `cone` for the
crown, joined at the crown's base. Assembling it as its own `thing` means
the scarecrow mates *one* thing to its head rather than juggling two
parts, and the hat is reusable.

**The brim is the hat's root part, deliberately.** An instance used as the
subject of a placement must be gripped by a part that nothing inside the
thing has already placed — otherwise its internal chain would place that
part twice. `Crown.base on Brim.center` (rather than the reverse) leaves
`Brim` unplaced, so `anchor rest = Brim.center` is a legal export.
Writing it the other way round is a compile error, and the message says so.

**Buttons use `center`, which needs no tuning.** A `center` anchor is
orientation-free: the mate skips the flip and simply makes the two points
coincide. A sphere's centre landing exactly on the head's surface is a
button half-sunk into burlap — no gap to guess at.

**Expression qualifiers.** `gap=0-thick*1.2` pulls the straw cuff into the
sleeve by a fraction of the sleeve's own thickness, so `Arm(thick=)`
rescales the whole detail. Qualifiers take expressions, not just literals.

**A blended head.** The sack is `union(..., blend=)` — a body and a tied
knot fused rather than stacked. `Head.top` reads the union's own extents
and lands on the knot, which is where the hat goes; `Head.surface(...)`
is shape-specific and delegates to the first operand, the sack sphere,
which is where the face goes. One part, both halves of the rule.

```moxi
material Wood   { color = brown }
material Straw  { color = yellow }
material Sack   { color = peach }
material Felt   { color = "#4a3b2a" }
material Button { color = black }

# Sleeve with a burst of straw at the cuff. The cuff is pulled in by a
# fraction of the sleeve's own radius, so the detail scales with `thick`.
thing Arm(length=6, thick=0.5) {
    part Sleeve { shape = capsule(height=length, radius=thick), material = Wood }
    part Cuff   { shape = blob(radius=thick*2.0, roughness=0.4), material = Straw }

    relation {
        Cuff.bottom on Sleeve.top gap=0-thick*1.2
    }

    anchor socket = Sleeve.bottom
    resolve voxel_size = 1.0
}

# Brim is the ROOT: the crown mates to it, not the other way round, so
# `rest` exports an unplaced part and the hat can be gripped from outside.
thing Hat(crown=2.2, brim=1.5) {
    part Brim  { shape = torus(major_radius=brim, minor_radius=0.3), material = Felt }
    part Crown { shape = cone(height=crown, radius=brim*0.72),       material = Felt }

    relation {
        Crown.base on Brim.center
    }

    anchor rest = Brim.center
    resolve voxel_size = 1.0
}

thing Scarecrow {
    part Post { shape = capsule(height=14, radius=0.8), material = Wood }

    # Sack body fused to its tied knot. Compass anchors read the whole
    # fused shape; `surface()` reads the sack sphere underneath it.
    part Head {
        shape = union(
            sphere(radius=3),
            at(sphere(radius=1.1), y=3.2),
            blend=1.0
        ),
        material = Sack
    }

    part Collar { shape = torus(major_radius=1.0, minor_radius=0.5), material = Straw }

    part EyeL  { shape = sphere(radius=0.38),                          material = Button }
    part EyeR  { shape = sphere(radius=0.38),                          material = Button }
    part Nose  { shape = cone(height=1.1, radius=0.42),                material = Straw }
    part Mouth { shape = box(width=1.6, height=0.3, depth=0.3, round=0.12), material = Button }

    part RightArm { thing = Arm }
    part LeftArm  { thing = Arm }
    part TheHat   { thing = Hat }

    relation {
        Head.bottom   on Post.top
        Collar.center on Post.top gap=0 - 0.6

        # Each eye gets its own yaw — the sphere already has an anchor
        # everywhere, so nothing needs to slide. `center` is
        # orientation-free, so a button lands half-sunk with no gap.
        EyeL.center  on Head.surface(yaw=-20, pitch=18)
        EyeR.center  on Head.surface(yaw=20,  pitch=18)
        Mouth.center on Head.surface(yaw=0,   pitch=-16)

        # The cone's base normal opposes the surface normal, so the nose
        # points out of the face rather than into it.
        Nose.base on Head.surface(yaw=0, pitch=1)

        # Perched on the knot, an inch down so the brim sits on burlap.
        TheHat.rest on Head.top gap=0 - 1.0

        # The original script's two lines, unchanged: an explicit mate to
        # a point on the post's side, and a reflection of its solved frame.
        #
        # No `pitch` here, deliberately. On a `side` anchor the meridian
        # convention puts the socket's tangent X along the shape's AXIS,
        # so `pitch` sweeps the arm horizontally around the post rather
        # than drooping it — and `twist` spins it about its own length,
        # which a capsule does not show. To droop, use `lean=(-8, 0)`:
        # it tips the arm down toward the face's `a` direction without
        # sweeping or rolling it. Straight out is what a scarecrow wants.
        RightArm.socket on Post.side(t=0.72, angle=90)
        LeftArm symmetric_across Post from=RightArm
    }

    constraint Head above Post

    resolve voxel_size = 1.0
}

print Scarecrow detail=low
```