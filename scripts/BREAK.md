# Break — pieces that come off

`breakable Part { … }` says the mate that places `Part` can break. When it
breaks, `Part` and everything mated under it leave as one lump, and the lump
keeps its own breakables. The compiler declares and checks this; the game
counts the hits.

- **The demon.** The head comes off after five hits and takes its eyes and
  horns with it. Each eye pops on one hit, before or after the head has
  fallen. A hit on an eye also counts toward the head. The left arm is the
  right arm's mirror image, so it breaks the same way, with its own count,
  and the right arm's `push=west` (outward: a character's own right is
  `west`) becomes east, outward on the left.
- **The rocket.** No hits: pieces come off when the game fires an event.
  `Launch` breaks the stack off the pad and `push` sends it up (`out`).
  `SeparateUpper` breaks the upper stack off the spent first stage and
  pushes it onward, up.

`push` says which way a lump is kicked when it breaks. `out` (the default)
means away from the part it hung from. `north`, `south`, `east`, `west`,
`up` and `down` are in the thing's own space.

```moxi
material Hide  { color = "#6b1f1f" }
material Horn  { color = "#e8dcc0" }
material Eye   { color = "#39ff6a", emissive = 1.3 }
material Steel { color = "#c9ccd1" }
material Paint { color = "#d8402f" }

thing Demon(toughness=5) {
    part Body  { shape = capsule(height=50, radius=16), material = Hide }
    part Head  { shape = sphere(radius=14), material = Hide }
    part HornR { shape = cone(height=14, radius=3), material = Horn }
    part HornL { shape = cone(height=14, radius=3), material = Horn }
    part EyeR  { shape = sphere(radius=3), material = Eye }
    part EyeL  { shape = sphere(radius=3), material = Eye }
    part ArmR  { shape = capsule(height=34, radius=5), material = Hide }
    part ArmL  { shape = capsule(height=34, radius=5), material = Hide }

    relation {
        Head.bottom  on Body.top gap=-4
        HornR.bottom on Head.surface(yaw=40, pitch=40) aim=up
        HornL.bottom on Head.surface(yaw=-40, pitch=40) aim=up
        EyeR.center  on Head.north shift=(3, -5) gap=-1
        EyeL.center  on Head.north shift=(3, 5) gap=-1
        ArmR.top     on Body.side(t=0.8, angle=270) aim=down
        ArmL symmetric_across Body from=ArmR
    }

    breakable Head { hits=toughness }
    breakable EyeR { hits=1 }
    breakable EyeL { hits=1 }
    breakable ArmR { hits=3  push=west }

    resolve voxel_size = 1
}

thing Rocket {
    part Pad      { shape = cylinder(height=6, radius=40), material = Steel }
    part Stage1   { shape = cylinder(height=90, radius=14), material = Paint }
    part Stage2   { shape = cylinder(height=60, radius=12), material = Steel }
    part Capsule  { shape = cone(height=30, radius=12), material = Paint }

    relation {
        Stage1.bottom  on Pad.top
        Stage2.bottom  on Stage1.top
        Capsule.bottom on Stage2.top
    }

    breakable Stage2 { event=SeparateUpper }
    breakable Stage1 { event=Launch }

    resolve voxel_size = 1
}

# One printed scene: every `print` is placed at the origin, so things shown
# together are instanced side by side. Instances inherit their breakables
# (`D.Head`, `R.Stage2`, …).
thing Showcase {
    part Floor { shape = box(width=260, height=4, depth=120), material = Steel }
    part D     { thing = Demon }
    part R     { thing = Rocket }

    relation {
        D.bottom on Floor.top shift=(-60, 0)
        R.bottom on Floor.top shift=(60, 0)
    }

    resolve voxel_size = 1
}

print Showcase
```

In the rocket, `Stage1` breaking off the pad is the launch: the whole stack
leaves together, because everything above hangs off `Stage1`. `Stage2`
separating later leaves `Capsule` riding on it, until something breaks that
too.
