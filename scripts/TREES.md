# Trees — one definition, three trees

One PalmTree definition, three different trees. Parameters have defaults
(`thing PalmTree(height=6, crown=3)`), instances override them
(`thing = PalmTree(height=10, crown=4)`), and every derived value is
computed from them rather than typed.

**Derived values stay derived.** The trunk's girth is a `let`, and so is
how far the crown sinks onto it: `gap=0 - crown / 3` rather than a fixed
`-1`. A fixed sink half-buries a small crown and barely seats a large one;
a proportional sink makes three sizes look like one species at three ages.
Qualifiers take expressions, so this is one line.

**The compass reads each tree's actual size.** `Tall.west on Mid.east`
uses each instance's solved bounding box — the tall tree's west face is
where ITS crown ends, not where a default tree's would.

**The grove has a parameter too.** `spacing` drives the `gap` between
trees, so the whole row respaces from one number.

```moxi
material Bark  { color = brown }
material Leafy { color = green }

thing PalmTree(height=6, crown=3) {
    let girth = crown * 0.2

    part Trunk { shape = cylinder(height=height, radius=girth), material = Bark }
    part Crown { shape = blob(radius=crown, roughness=0.4),    material = Leafy }

    relation {
        Crown.bottom on Trunk.top gap=0 - crown / 3
    }

    resolve voxel_size = 1.0
}

thing Grove(spacing=1) {
    part Small { thing = PalmTree(height=4, crown=2) }
    part Mid   { thing = PalmTree }
    part Tall  { thing = PalmTree(height=10, crown=4) }

    relation {
        Mid.west  on Small.east gap=spacing
        Tall.west on Mid.east   gap=spacing
    }

    resolve voxel_size = 1.0
}

print Grove detail=low
```