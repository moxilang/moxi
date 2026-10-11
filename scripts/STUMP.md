# Stump — what is left after a break

What a lump leaves behind (DOC-20261008-detachment §4): the same action
that detaches it says what is left. `Part material = Material` swaps a look,
`reveal Part` shows a part that is not there until then.

The demon's head takes five hits; then the neck and the head's cut turn to
raw flesh and a stump appears where the head stood. Each arm comes off on
its own and leaves a shoulder stump: the left arm and its stump are the
mirror images of the right ones, and the left arm's action reveals the left
stump. The rocket's upper stack separates when the game fires `Separate`
and uncovers the interstage ring.

Revealed parts are hidden at rest in every output that draws rest (voxels,
`moxi mesh`, `moxi web`); glTF keeps them with `moxi.hidden`, and
`moxi report STUMP.md --run BreakHead` shows the stump, the swaps, and no
torn contact for it.

```moxi
material Bone  { color = "#ddd8c4" }
material Flesh { color = "#7a1414", roughness = 0.25 }
material Eye   { color = "#f2e14a", emissive = 1 }
material Steel { color = "#b8bcc4", roughness = 0.4, metalness = 1 }
material Ring  { color = "#6a6e76" }

thing Demon {
    part Body   { shape = box(width=40, height=60, depth=30), material = Bone }
    part Neck   { shape = cylinder(height=8, radius=7), material = Bone }
    part Head   { shape = sphere(radius=14), material = Bone }
    part Stump  { shape = cylinder(height=3, radius=6), material = Flesh }
    part EyeR   { shape = sphere(radius=3), material = Eye }
    part EyeL   { shape = sphere(radius=3), material = Eye }
    part ArmR   { shape = capsule(height=30, radius=5), material = Bone }
    part ArmL   { shape = capsule(height=30, radius=5), material = Bone }
    part NubR   { shape = sphere(radius=5), material = Flesh }
    part NubL   { shape = sphere(radius=5), material = Flesh }
    relation {
        Neck.bottom  on Body.top
        Head.bottom  on Neck.top gap=-2
        Stump.bottom on Neck.top
        EyeR.center  on Head.north shift=(3, 5)
        EyeL.center  on Head.north shift=(3, -5)
        ArmR.top     on Body.east shift=(20, 0)
        ArmL symmetric_across Body from=ArmR
        NubR.center  on Body.east shift=(20, 0)
        NubL symmetric_across Body from=NubR
    }
    action BreakHead { trigger Head hits=5  detach Head  Neck material = Flesh  Head material = Flesh  reveal Stump }
    action BreakEyeR { trigger EyeR hits=1  detach EyeR }
    action BreakEyeL { trigger EyeL hits=1  detach EyeL }
    action BreakArmR { trigger ArmR hits=3  detach ArmR  reveal NubR }
    action BreakArmL { trigger ArmL hits=3  detach ArmL  reveal NubL }
}

thing Rocket {
    part Pad      { shape = cylinder(height=4, radius=20), material = Ring }
    part Stage1   { shape = cylinder(height=60, radius=8), material = Steel }
    part Stage2   { shape = cylinder(height=40, radius=7), material = Steel }
    part Capsule  { shape = cone(height=14, radius=7), material = Steel }
    part Inter    { shape = cylinder(height=2, radius=8.5), material = Ring }
    relation {
        Stage1.bottom  on Pad.top
        Stage2.bottom  on Stage1.top
        Capsule.bottom on Stage2.top
        Inter.bottom   on Stage1.top
    }
    action Separate { detach Stage2  reveal Inter }
}

thing Yard {
    part Ground { shape = box(width=260, height=2, depth=120), material = Ring }
    part D { thing = Demon }
    part R { thing = Rocket }
    relation {
        D.Body.bottom on Ground.top shift=(-60, 0)
        R.Pad.bottom  on Ground.top shift=(60, 0)
    }
}

print Yard
```
