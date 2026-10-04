# Desk lamp — poses

The living-models demo (DOC-20261004-living-models-design, step 2). A pose is
the thing with some mate qualifiers set to other values, solved again: what
`pose Reading` says is exactly what you would get by writing its numbers into
the mate lines. `moxi scene --pose Reading` and `moxi gltf --pose Reading`
print it; every pose is solved and its constraints checked on every compile,
so `constraint Shade above Base` holds in all of them.

`Desk` shows the two other rules: an instance takes its thing's pose by name
(`Left pose=Folded`), and a `symmetric_across` image follows its source — the
right lamp is posed through the left one.

`animate` strings poses together in seconds: `moxi gltf` exports each as a
glTF animation (one clip per `animate`; `loop` is recorded in its extras).
Only keyframes are solved; a player interpolates between them, so no joint
may turn more than 179° from one keyframe to the next — the compiler says
which joint and where to insert a keyframe.

`joint` bounds what a mate may do: every pose (and rest) is checked against
it, so a pose that over-bends the forearm is a compile error that quotes the
number and the range. `moxi gltf` writes the ranges into each node's extras.

`emissive` makes a material glow in its own colour. A `state` changes
materials only — `Off` dims every bulb, `Alarm` gives the left bulb another
material — and `moxi gltf` exports states as KHR_materials_variants, so a
game switches them with one call (three.js: `gltf.functions.selectVariant`).

```moxi
material Iron   { color = "#2b2b2b" }
material Enamel { color = "#c8442c" }
material Glow   { color = "#fff2b0", emissive = 1.5 }
material Hot    { color = red, emissive = 2 }
material Oak    { color = "#9a6b3f" }

thing DeskLamp(upper=60, fore=55) {
    part Base  { shape = cylinder(height=6, radius=26), material = Iron }
    part Upper { shape = capsule(height=upper, radius=2.5), material = Iron }
    part Fore  { shape = capsule(height=fore, radius=2.2), material = Iron }
    part Shade { shape = cone(height=22, radius=16), material = Enamel }
    part Bulb  { shape = sphere(radius=6), material = Glow }

    relation {
        Upper.bottom on Base.top lean=(15, 0)
        Fore.bottom  on Upper.top lean=(-80, 0)
        Shade.top    on Fore.top lean=(-60, 0)
        Bulb.center  on Shade.bottom gap=3
    }

    anchor foot = Base.bottom

    joint Upper { lean=(0..45, 0..0) }
    joint Fore  { lean=(-160..-15, 0..0) }
    joint Shade { lean=(-110..0, 0..0) }

    pose Reading { Upper lean=(25, 0)  Fore lean=(-50, 0)  Shade lean=(-80, 0) }
    pose Folded  { Upper lean=(5, 0)   Fore lean=(-150, 0) Shade lean=(-20, 0) }
    pose Craning { Upper lean=(40, 0)  Fore lean=(-20, 0)  Shade lean=(-100, 0) }

    constraint Shade above Base

    resolve voxel_size = 1
}

thing Desk {
    part Top   { shape = box(width=200, height=6, depth=90), material = Oak }
    part Left  { thing = DeskLamp }
    part Right { thing = DeskLamp }

    relation {
        Left.foot on Top.top shift=(-60, 0)
        Right symmetric_across Top from=Left
    }

    pose Tidy  { Left pose=Folded }
    pose Study { Left pose=Reading }

    state Off   { Glow.emissive = 0  Glow.color = gray }
    state Alarm { Left.Bulb material = Hot }

    animate Wake { 0 Tidy  0.6 rest  1.2 Study }
    animate Nod loop { 0 Study  0.4 rest  0.8 Study }

    resolve voxel_size = 1
}

print Desk detail=low
```
