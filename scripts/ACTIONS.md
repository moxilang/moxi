# Actions — behaviour you compose

What a thing can do, written as actions (Issue N,
DOC-20261011-composable-actions). An action has a name, an optional
`trigger` (hits on a part or anything inside it), and effects on the
thing's own structure: `detach` a lump, `reveal` or `hide` a part, swap a
part's look, switch a `state`, `play` a clip, `run` another action. Any line
may wait (`delay=` seconds). With no trigger the game fires the action by
name. Moxi only declares; the game runs the effects and keeps the physics.

The hut: three blows on the door break it in and show the dark doorway.
`Explode` (fired by the game, a barrel inside going off) throws the roof up,
the four planks of the front wall outward one after another, chars the
walls, and after a moment runs `Settle`: the chimney sways and drops. One
blow knocks the lantern off (`BreakLantern`): a piece that comes off is just
an action with a trigger and a `detach`.

The yard has two huts. Each brings its actions, named and placed by its
instance (`A.Explode`, `B.Door`), so the game can blow up one and not the
other.

```moxi
material Wood   { color = "#7a5a3a" }
material Plank  { color = "#8a6a45" }
material Ash    { color = "#2b2622", roughness = 0.95 }
material Dark   { color = "#120d0a" }
material Stone  { color = "#77736d" }
material Flame  { color = "#ffb050", emissive = 2 }

thing Hut(w=60, h=40) {
    part Floor   { shape = box(width=w, height=4, depth=w), material = Stone }
    part Back    { shape = box(width=w, height=h, depth=4), material = Wood }
    part Roof    { shape = box(width=w + 10, height=4, depth=w + 10), material = Wood }
    part Door    { shape = box(width=14, height=26, depth=3), material = Plank }
    part Doorway { shape = box(width=14, height=26, depth=1), material = Dark }
    part Chimney { shape = box(width=8, height=20, depth=8), material = Stone }
    part Lantern { shape = sphere(radius=3), material = Flame }
    relation {
        Back.bottom    on Floor.top shift=(0, -w / 2 + 2)
        Roof.bottom    on Back.top shift=(0, w / 2 - 2)
        Chimney.bottom on Back.top shift=(w / 4, 0) gap=-2
        Door.bottom    on Floor.top shift=(0, w / 2 - 2)
        Doorway.bottom on Floor.top shift=(0, w / 2 - 3)
        Lantern.center on Back.top shift=(0, 6)
    }
    # the front wall: four planks
    for k in 0..4 {
        part Wall[k] { shape = box(width=w / 4 - 1, height=h, depth=4), material = Plank }
        relation { Wall[k].bottom on Floor.top shift=(-w / 2 + w / 8 + k * w / 4, w / 2 + 2) }
    }

    state Charred { Back material = Ash  Roof material = Ash }
    pose Leaning { Chimney lean=(0, 25) }
    animate Sway { 0 rest  1 Leaning  2 rest }

    action Kick {
        trigger Door hits=3
        detach Door push=north strength=40
        reveal Doorway
    }
    action Explode {
        detach Roof push=up strength=60
        for k in 0..4 { detach Wall[k] push=north strength=45 delay=k * 0.05 }
        state Charred on
        Floor material = Ash
        hide Lantern
        run Settle delay=0.6
    }
    action Settle {
        play Sway
        detach Chimney push=down delay=0.4
    }
    action BreakLantern { trigger Lantern hits=1  detach Lantern }
}

thing Yard {
    part Ground { shape = box(width=300, height=2, depth=160), material = Stone }
    part A { thing = Hut }
    part B { thing = Hut(w=50) }
    relation {
        A.Floor.bottom on Ground.top shift=(-70, 0)
        B.Floor.bottom on Ground.top shift=(70, 0)
    }
}

print Yard
```
