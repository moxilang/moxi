# States — on at once

A lamp that can be `On` and `Broken` at the same time, with a face whose
eyes are `Calm` or `Angry` but never both. DOC-20261011-closing-the-circle,
Issue I.

**Any set of states may be on at once.** Where two on states change the same
part, the one declared later wins that part's whole look: `Broken` comes
after `On`, so a broken lamp's bulb is dark even when it is on, while `On`
still lights the shade. `moxi report STATES.md --state On,Broken` lists each
look and the overlap.

**A family** (`state Calm in Eyes`) says its states exclude each other: a
consumer switching `Angry` on switches `Calm` off, and `--state Calm,Angry`
is an error.

`moxi gltf` keeps one material variant per state for any viewer, and adds
`moxi.states` to every node a state changes plus the scene's
`moxi.stateOrder` and `moxi.stateFamilies`, so a game can combine them.

```moxi
material Iron  { color = "#3a3a3a", roughness = 0.5, metalness = 1 }
material Glass { color = "#d8d0b8", opacity = 0.8 }
material Lit   { color = "#fff0b0", emissive = 2 }
material Ash   { color = "#2a2622" }
material Skin  { color = "#c9a48a" }
material Iris  { color = "#4a6a8a" }

thing Lamp {
    part Base  { shape = cylinder(height=3, radius=8), material = Iron }
    part Post  { shape = capsule(height=30, radius=1.2), material = Iron }
    part Shade { shape = cone(height=8, radius=7), material = Glass }
    part Bulb  { shape = sphere(radius=2.5), material = Glass }
    part Face  { shape = sphere(radius=4), material = Skin }
    part EyeR  { shape = sphere(radius=0.8), material = Iris }
    part EyeL  { shape = sphere(radius=0.8), material = Iris }
    relation {
        Post.bottom  on Base.top
        Shade.top    on Post.top gap=-1
        Bulb.top     on Shade.bottom gap=-3
        Face.bottom  on Base.top shift=(6, 0)
        EyeR.center  on Face.surface(yaw=25, pitch=10)
        EyeL.center  on Face.surface(yaw=-25, pitch=10)
    }
    state On     { Shade material = Lit  Bulb material = Lit }
    state Broken { Bulb material = Ash  Iron.roughness = 0.95  Iron.metalness = 0.3 }
    state Calm  in Eyes { Iris.color = "#4a6a8a" }
    state Angry in Eyes { Iris.color = "#ff2414"  Iris.emissive = 1.5 }
    resolve voxel_size = 0.5
}

print Lamp
```
