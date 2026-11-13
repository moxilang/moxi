# Moxi Boy, built with Form

The 0.8 baseline: the same character as `MOXIBOI.md`, written from scratch
with the Form constructs, to see how far the language gets toward a real
object without changing who he is.

**He keeps his face.** Proportions, eyes, shines and blush are the cartoon's.
Realism goes into the body and the materials, never into the features: that
is where a stylized character turns uncanny.

**One skin.** `fuse` makes torso, head, arms, fists, legs and feet one
surface with soft fillets where they meet, the way a vinyl figure is cast,
instead of beads strung on a wire.

**Limbs are spans.** Each arm and leg is a tube that runs from the body to
a hand or foot placed where the pose wants it. Nobody writes an elbow
angle: put the fist somewhere and the arm reaches it. `via` puts the elbow
out to the side, the way a gorilla arm bends.

**Muscle is creases, not fillets.** A wide blend everywhere reads as fat:
it fills every valley. So the fuse is tight (`blend=0.45`), pecs and
shoulders are their own parts with `fuses = Torso` and a tighter blend
still, the two pecs are not blended into each other at all, and every limb
pinches at its joints. The waist is narrow and the legs long enough to
show it: the V is the muscle.

**The mouth is a hole.** `cuts = Head` opens a real mouth with a soft rim,
instead of a black disc glued to the face.

**Pose.** He is about to eat the plum: the fist comes up in front of the
chest, under his chin, with the plum on it at his open mouth. The fist
cannot go to the mouth along the body (the chest is in the way), so the
elbow drops and swings forward and the forearm comes up in front of him.

```moxi
material Skin  { color = "#F2ECDF", roughness = 0.42 }
material Black { color = black,     roughness = 0.06 }
material White { color = white,     roughness = 0.2 }
material Mouth { color = "#3a0d16", roughness = 0.5 }
material Cheek { color = "#FF46A2", roughness = 0.6 }
material Plum  { color = "#4b1a55", roughness = 0.5 }
material Stem  { color = "#5a3a1c", roughness = 0.8 }
material Leaf  { color = "#3f7d2c", roughness = 0.45 }

# A mitten fist: a rounded block with a thumb across the front. `side` is
# 1 for his right hand, -1 for his left, so the thumb is always inside.
shape Fist(side) = union(
    box(width=7.2, height=6.8, depth=7.4, round=2.4),
    at(spin(capsule(height=3.0, radius=1.5), axis=z, degrees=90), x=-1.2 * side, y=-0.9, z=3.3),
    blend=0.4)

thing MoxiBoy {
    # A V, not a barrel: small hips, a narrow waist, then a chest three
    # times the waist's width. The taper is what reads as muscle.
    part Torso {
        shape = union(
            ellipsoid(rx=4.6, ry=3, rz=3.5),
            at(box(width=7, height=6, depth=4.8, round=2), y=4),
            at(ellipsoid(rx=12.5, ry=5.6, rz=5.8), y=10.5),
            at(ellipsoid(rx=6.5, ry=2.6, rz=3.4), y=15.2),
            blend=1.6),
        material = Skin
    }
    # Muscles are separate masses with a crease between them: no blend
    # between the two pecs, and only a tight one onto the chest.
    part Pecs {
        shape = union(at(ellipsoid(rx=5.2, ry=3.3, rz=2.6), x=-5.3, y=11.6, z=4.2),
                      at(ellipsoid(rx=5.2, ry=3.3, rz=2.6), x=5.3, y=11.6, z=4.2)),
        fuses = Torso, blend = 0.35
    }
    part Delts {
        shape = union(at(sphere(radius=4.7), x=13.2, y=13.2), at(sphere(radius=4.7), x=-13.2, y=13.2)),
        fuses = Torso, blend = 0.5
    }
    part Head { shape = box(width=9, height=7.5, depth=6.5, round=2.0), material = Skin }

    part RightFist { shape = Fist(1),  material = Skin }
    part LeftFist  { shape = Fist(-1), material = Skin }
    part RightFoot { shape = box(width=6, height=2.8, depth=8.6, round=1.2), material = Skin }
    part LeftFoot  { shape = box(width=6, height=2.8, depth=8.6, round=1.2), material = Skin }

    # A limb pinches at its joints and bulges between them: shoulder,
    # bicep, elbow, a forearm bigger than the bicep, wrist.
    part RightArm { shape = tube(radius=[3.4, 4.9, 2.9, 5.6, 3.3]), material = Skin }
    part LeftArm  { shape = tube(radius=[3.4, 4.9, 2.9, 5.6, 3.3]), material = Skin }
    # Hip, thigh, knee, calf, ankle.
    part RightLeg { shape = tube(radius=[3.3, 4.1, 2.5, 3.4, 2.3]), material = Skin }
    part LeftLeg  { shape = tube(radius=[3.3, 4.1, 2.5, 3.4, 2.3]), material = Skin }

    part LeftEye    { shape = sphere(radius=1.5),                 material = Black }
    part RightEye   { shape = sphere(radius=1.5),                 material = Black }
    part LeftShine  { shape = sphere(radius=0.45),                material = White }
    part RightShine { shape = sphere(radius=0.45),                material = White }
    part LeftCheek  { shape = ellipsoid(rx=1.2, ry=0.55, rz=0.4), material = Cheek }
    part RightCheek { shape = ellipsoid(rx=1.2, ry=0.55, rz=0.4), material = Cheek }
    part MouthHole  { shape = ellipsoid(rx=1.4, ry=1.7, rz=1.6),  cuts = Head, blend = 0.35 }
    part MouthBack  { shape = ellipsoid(rx=1.3, ry=1.6, rz=0.6),  material = Mouth }

    part PlumFruit { shape = sphere(radius=2.6), material = Plum }
    part PlumStem  { shape = tube(path=[[0, 0, 0], [0.15, 0.7, 0], [0.55, 1.25, 0]], radius=[0.22, 0.14]), material = Stem }
    part PlumLeaf  { shape = ellipsoid(rx=1.3, ry=0.24, rz=0.65), material = Leaf }

    relation {
        Head.bottom on Torso.top gap=-0.8
        Pecs.origin  on Torso.origin
        Delts.origin on Torso.origin

        # Hands and feet go where the pose wants them: the right fist
        # brought up in front of his chest, under his chin, the left
        # hanging to the knee, gorilla-long...
        RightFist.bottom on Torso.point(x=5, y=8.2, z=10.5, nx=0, ny=1, nz=0)
        LeftFist.top     on Torso.point(x=-21, y=-8, z=3, nx=0, ny=-1, nz=0)
        RightFoot.bottom on Torso.point(x=6.2, y=-25, z=1.4, nx=0, ny=1, nz=0)
        LeftFoot.bottom  on Torso.point(x=-6.2, y=-25, z=1.4, nx=0, ny=1, nz=0)

        # ...and the limbs reach them.
        RightArm from Torso.point(x=14.5, y=13, z=0, nx=1, ny=0, nz=0) to RightFist.bottom via=[17, 4.5, 7]
        LeftArm  from Torso.point(x=-14.5, y=12.5, z=0, nx=-0.5, ny=-0.87, nz=0) to LeftFist.top reach=0.3
        RightLeg from Torso.point(x=4.9, y=-1.0, z=0, nx=0, ny=-1, nz=0) to RightFoot.top reach=0.2
        LeftLeg  from Torso.point(x=-4.9, y=-1.0, z=0, nx=0, ny=-1, nz=0) to LeftFoot.top reach=0.2

        LeftEye.south  on Head.north shift=(1.5, -2.6) gap=-0.9
        RightEye.south on Head.north shift=(1.5,  2.6) gap=-0.9
        LeftShine.south  on LeftEye.north  gap=-0.7
        RightShine.south on RightEye.north gap=-0.7
        LeftCheek.south  on Head.north shift=(-1.2, -3.4) gap=-0.4
        RightCheek.south on Head.north shift=(-1.2,  3.4) gap=-0.4

        MouthHole.south on Head.north shift=(-2.0, 0) gap=-1.4
        MouthBack.south on Head.north shift=(-2.0, 0) gap=-1.5

        PlumFruit.bottom on Torso.point(x=3.4, y=14.8, z=8.2, nx=0, ny=1, nz=0)
        PlumStem.start   on PlumFruit.top gap=-0.3
        PlumLeaf.west    on PlumStem.end
    }

    fuse Torso, Head, RightArm, LeftArm, RightFist, LeftFist, RightLeg, LeftLeg, RightFoot, LeftFoot blend=0.45

    resolve grain = 0.45
}

print MoxiBoy
```

Everything the pose needs is four points: where each fist and foot goes.
Move one and its limb follows. The build is the gorilla's: a chest far wider than the waist, forearms
thicker than the upper arms, a hanging fist that reaches the knee.
