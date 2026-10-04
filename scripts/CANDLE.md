# Candle

A candle in a brass dish. The flame is three see-through cones, one inside
the other: a dim red outer flame, an orange body, a pale hot core, all
`blend = additive`, so where they overlap they get brighter, the way fire
does. Above it a wisp of smoke, see-through but not additive (smoke darkens).

Fire has to leap and shrink, and a pose can only move parts rigidly, so the
flame is built in stacked segments that slide along each other: `Flicker`
pulls the upper cones out and lets them fall back, and tips the tip, so the
flame stretches without any part changing size. `state Out` fades it to
almost nothing and leaves the smoke.

```moxi
material Wax   { color = ivory }
material Brass { color = "#b08a3a" }
material Wick  { color = "#1a1612" }
material Outer { color = "#c2321a", emissive = 1.2, opacity = 0.35, blend = additive }
material Body  { color = "#ff8a26", emissive = 1.8, opacity = 0.55, blend = additive }
material Core  { color = "#fff2c0", emissive = 2.5, opacity = 0.7,  blend = additive }
material Smoke { color = "#5a5a5a", opacity = 0.25 }

thing Candle(h=60, r=9) {
    part Dish  { shape = cylinder(height=3, radius=r * 2.2), material = Brass }
    part Stick { shape = cylinder(height=h, radius=r), material = Wax }
    part Wick  { shape = cylinder(height=4, radius=0.7), material = Wick }
    relation {
        Stick.bottom on Dish.top
        Wick.bottom on Stick.top gap=-1
    }

    # the flame: a base and a tip per layer, the tip sliding on the base
    part OuterBase { shape = cone(height=12, radius=4.2), material = Outer }
    part OuterTip  { shape = cone(height=12, radius=2.6), material = Outer }
    part BodyBase  { shape = cone(height=9, radius=2.8), material = Body }
    part BodyTip   { shape = cone(height=9, radius=1.7), material = Body }
    part CoreBase  { shape = cone(height=6, radius=1.4), material = Core }
    part Puff      { shape = sphere(radius=3), material = Smoke }
    relation {
        OuterBase.bottom on Wick.top gap=-3
        OuterTip.bottom  on OuterBase.top gap=-7
        BodyBase.bottom  on Wick.top gap=-2
        BodyTip.bottom   on BodyBase.top gap=-5
        CoreBase.bottom  on Wick.top gap=-1.5
        Puff.bottom      on OuterTip.top gap=6
    }

    joint OuterTip { gap=-9..-2  lean=(-20..20, -20..20) }
    joint BodyTip  { gap=-7..-1  lean=(-20..20, -20..20) }
    joint Puff     { gap=2..18 }

    # licking: stretch, lean, fall back
    pose Tall { OuterTip gap=-3 lean=(6, 0)  BodyTip gap=-2 lean=(4, 2)  Puff gap=12 }
    pose Low  { OuterTip gap=-8 lean=(-5, 3)  BodyTip gap=-6 lean=(-3, -2)  Puff gap=4 }
    animate Flicker loop { 0 rest  0.18 Tall  0.31 Low  0.5 Tall  0.7 rest }

    state Out { Outer.opacity = 0.03  Body.opacity = 0.05  Core.opacity = 0.08  Smoke.opacity = 0.45 }

    resolve voxel_size = 0.5
}

print Candle detail=low
```
