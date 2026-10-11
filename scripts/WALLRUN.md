# Wall run

A ruined wall: two runs of footing and blocks, the second laid on the first
by a path into the instance (`B.Footing.bottom on A.Footing.top`). Shows the
small syntax of DOC-20261011-closing-the-circle, Issue L2: end-of-line `#`
comments, `%` (floored modulo) for every-other-block, a positional `point`,
and a single joint range in parentheses.

```moxi
material Stone { color = "#8a8478" }
material Moss  { color = "#4f5a3a" }

thing Run(len=60, blocks=5) {
    part Footing { shape = box(width=len, height=4, depth=10), material = Stone }
    for i in 0..blocks {
        part Block[i] {
            shape = box(width=10, height=12 + 6 * (i % 2), depth=8)   # tall, short, tall…
            material = Stone
        }
        relation { Block[i].bottom on Footing.top shift=(-len / 2 + 6 + i * (len - 12) / (blocks - 1), 0) }
    }
}

thing Wall {
    part A   { thing = Run(len=80, blocks=6) }
    part B   { thing = Run(len=50, blocks=4) }
    part Tuft { shape = sphere(radius=3), material = Moss }
    part Gate { shape = box(width=2, height=14, depth=8), material = Stone }
    relation {
        B.Footing.bottom on A.Footing.top shift=(10, 0)    # a path into the instance
        Tuft.center on A.Footing.point(30, 2, 0)           # point by position: x, y, z
        Gate.bottom on A.Footing.top shift=(38, 0)
    }
    joint Gate { twist=(0..110) }                    # one range in parentheses
}

print Wall
```
