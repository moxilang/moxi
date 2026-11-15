# Moxi Boy, with marks

The 0.9 baseline, drawn closer to the cartoon. Three changes from
`MOXIBOI_FORM.md`:

**No neck.** The head is bigger and sits sunk into a wide trapezius that
slopes from the head's lower sides straight out to the shoulders, so he
reads as a tank: head, then shoulders, then arms.

**Broader on top, shorter below.** Bigger, higher deltoids and a deeper
chest; shorter, thicker legs. The hanging fist still comes down past the
hip, gorilla-long.

**A flat face, set low.** The eyes, their shines and the blush are marks:
they lie flat on the head instead of bulging, and the eyes blink. The eyes
sit just below the middle of the face, the blush under their outer edge.
Each eye has a shine and a small glint opposite it, lit from one side.

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
            at(ellipsoid(rx=13, ry=6.5, rz=6.2), y=11.5),
            at(ellipsoid(rx=12, ry=5, rz=5.2), y=17),
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
        shape = union(at(sphere(radius=6.2), x=15, y=16), at(sphere(radius=6.2), x=-15, y=16)),
        fuses = Torso, blend = 0.5
    }
    part Head { shape = box(width=12.5, height=10.5, depth=8.5, round=2.6), material = Skin }

    part RightFist { shape = Fist(1),  material = Skin }
    part LeftFist  { shape = Fist(-1), material = Skin }
    part RightFoot { shape = box(width=6, height=2.8, depth=8.6, round=1.2), material = Skin }
    part LeftFoot  { shape = box(width=6, height=2.8, depth=8.6, round=1.2), material = Skin }

    # A limb pinches at its joints and bulges between them: shoulder,
    # bicep, elbow, a forearm bigger than the bicep, wrist.
    # The bent arm is two straight pieces meeting at an elbow, the way an
    # arm is: one curved tube through a tight bend reads as a hose.
    part RightUpper   { shape = tube(radius=[3.4, 4.9, 3.0]), material = Skin }
    part RightForearm { shape = tube(radius=[3.3, 5.6, 3.0]), material = Skin }
    part LeftArm  { shape = tube(radius=[3.4, 4.9, 2.9, 5.6, 3.3]), material = Skin }
    # Hip, thigh, knee, calf, ankle.
    part RightLeg { shape = tube(radius=[3.8, 4.8, 3.3, 3.9, 2.9]), material = Skin }
    part LeftLeg  { shape = tube(radius=[3.8, 4.8, 3.3, 3.9, 2.9]), material = Skin }

    # The face is marks: flat on the head, so the eyes don't bulge and can
    # blink. A shine on each eye, a glint opposite it, blush under them.
    part LeftEye    { mark = oval(width=3.0, height=3.4), material = Black }
    part RightEye   { mark = oval(width=3.0, height=3.4), material = Black }
    part LeftShine  { mark = oval(width=0.9, height=1.0), material = White }
    part RightShine { mark = oval(width=0.9, height=1.0), material = White }
    part LeftGlint  { mark = oval(width=0.4, height=0.4), material = White }
    part RightGlint { mark = oval(width=0.4, height=0.4), material = White }
    part LeftCheek  { mark = oval(width=2.0, height=0.9), material = Cheek }
    part RightCheek { mark = oval(width=2.0, height=0.9), material = Cheek }
    part MouthHole  { shape = ellipsoid(rx=1.3, ry=1.4, rz=1.6),  cuts = Head, blend = 0.35 }
    part MouthBack  { shape = ellipsoid(rx=1.2, ry=1.3, rz=0.6),  material = Mouth }

    part PlumFruit { shape = sphere(radius=2.6), material = Plum }
    part PlumStem  { shape = tube(path=[[0, 0, 0], [0.15, 0.7, 0], [0.55, 1.25, 0]], radius=[0.22, 0.14]), material = Stem }
    part PlumLeaf  { shape = ellipsoid(rx=1.3, ry=0.24, rz=0.65), material = Leaf }

    relation {
        Head.bottom on Torso.top gap=-3.5
        Pecs.origin  on Torso.origin
        Delts.origin on Torso.origin

        # Hands and feet go where the pose wants them: the right fist
        # brought up in front of his chest, under his chin, the left
        # hanging to the knee, gorilla-long...
        RightFist.bottom on Torso.point(x=5.5, y=9.3, z=11.6, nx=0, ny=1, nz=0)
        LeftFist.top     on Torso.point(x=-21.5, y=-6, z=3, nx=0, ny=-1, nz=0)
        RightFoot.bottom on Torso.point(x=6.4, y=-20, z=1.4, nx=0, ny=1, nz=0)
        LeftFoot.bottom  on Torso.point(x=-6.4, y=-20, z=1.4, nx=0, ny=1, nz=0)

        # ...and the limbs reach them.
        RightUpper   from Torso.point(x=16, y=16, z=0, nx=1, ny=0, nz=0) to [18, 3, 5] reach=0
        RightForearm from RightFist.bottom to [18, 3, 5] reach=0
        LeftArm  from Torso.point(x=-16, y=15.5, z=0, nx=-0.5, ny=-0.87, nz=0) to LeftFist.top reach=0.3
        RightLeg from Torso.point(x=4.9, y=-1.0, z=0, nx=0, ny=-1, nz=0) to RightFoot.top reach=0.2
        LeftLeg  from Torso.point(x=-4.9, y=-1.0, z=0, nx=0, ny=-1, nz=0) to LeftFoot.top reach=0.2

        LeftEye  on Head.north shift=(-0.6, -3.0)
        RightEye on Head.north shift=(-0.6,  3.0)
        LeftShine  on LeftEye.center  shift=(0.55, 0.4)
        RightShine on RightEye.center shift=(0.55, 0.4)
        LeftGlint  on LeftEye.center  shift=(-0.7, -0.45)
        RightGlint on RightEye.center shift=(-0.7, -0.45)
        LeftCheek  on Head.north shift=(-2.5, -3.7)
        RightCheek on Head.north shift=(-2.5,  3.7)

        MouthHole.south on Head.north shift=(-3.4, 0) gap=-1.4
        MouthBack.south on Head.north shift=(-3.4, 0) gap=-1.5

        PlumFruit.bottom on Torso.point(x=3.4, y=15.5, z=9.3, nx=0, ny=1, nz=0)
        PlumStem.start   on PlumFruit.top gap=-0.3
        PlumLeaf.west    on PlumStem.end
    }

    fuse Torso, Head, RightUpper, RightForearm, LeftArm, RightFist, LeftFist, RightLeg, LeftLeg, RightFoot, LeftFoot blend=0.45

    pose Shut     { LeftEye height=0.25  RightEye height=0.25
                    LeftShine height=0.01  RightShine height=0.01  LeftGlint height=0.01  RightGlint height=0.01 }
    animate Blink { 0 rest  0.08 Shut  0.16 rest }

    resolve grain = 0.45
}

print MoxiBoy
```

`pose Shut` squashes both eyes to a line and hides their shines;
`animate Blink` plays it. In glTF the blink is a morph target on each eye.
