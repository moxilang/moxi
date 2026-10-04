# Chain — joints in a loop

A chain hanging from a hook: eight links made by a `for`, each one bounded by
its own `joint`, declared in the same loop. The bounds use the loop variable:
a link near the hook may swing only a little, the free end a lot, which is
how a real chain moves. The `Swing` animate stays inside every range (the
compiler checks each link's lean at rest and in every pose against its own
`Link[k]` joint); push a pose past one and the error names the link.

```moxi
material Iron  { color = "#4a4a50" }
material Brass { color = "#a67c2e" }

thing Chain(links=8, l=14) {
    part Beam { shape = box(width=80, height=8, depth=8), material = Iron }
    part Hook { shape = torus(major_radius=4, minor_radius=1.2), material = Brass }
    relation { Hook.top on Beam.bottom lean=(0, 90) gap=-1 }

    part Link[0] { shape = capsule(height=l, radius=1.6), material = Iron }
    relation { Link[0].top on Hook.bottom gap=-2 }
    for k in 1..links {
        let reach = 4 + 3 * k
        part Link[k] { shape = capsule(height=l, radius=1.6), material = Iron }
        relation { Link[k].top on Link[k - 1].bottom gap=-2 }
        joint Link[k] { lean=(-reach..reach, -reach..reach)  twist=-20..20 }
    }

    pose Left  { for k in 1..links { Link[k] lean=(2 + 2 * k, 0) } }
    pose Right { for k in 1..links { Link[k] lean=(-2 - 2 * k, 1) } }
    animate Swing loop { 0 Left  1.2 Right  2.4 Left }

    resolve voxel_size = 0.5
}

print Chain detail=low
```
