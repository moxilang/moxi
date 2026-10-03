# Lamp — the SKILL.md example, and the render smoke test

This is the complete file from the top of SKILL.md (`docs/skill_preamble.md`),
copied verbatim so it is compiled by CI like every other script. It is also
the fixture for `cargo make render-check`: a headless browser renders
`moxi web` output and the check asserts the yellow bulb is on screen. If you
change the example in the preamble, change it here too.

```moxi
material Iron  { color = "#333333" }
material Glass { color = yellow }

thing Lamp(height=300, reach=90) {
    part Base { shape = cylinder(height=12, radius=45), material = Iron }
    part Post { shape = cylinder(height=height, radius=6), material = Iron }
    part Arm  { shape = capsule(height=reach, radius=4), material = Iron }
    part Bulb { shape = sphere(radius=18), material = Glass }

    relation {
        Post.bottom on Base.top
        Arm.bottom  on Post.side(t=0.92, angle=0) lean=(20, 0)
        Bulb.center on Arm.top
    }

    resolve voxel_size = 3
}

print Lamp detail=low
```
