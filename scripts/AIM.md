# Aim — where a part points

`aim=` states where a part's axis points instead of working it out from
`lean`, `twist` and `pitch`. Three kinds of target, one of each here:

- **a part or anchor.** The watcher's eye is mated on top of its post and
  aims at the ball (`aim=Ball`, which means the ball's center). Pose `Roll`
  swings the ball's stick, and the eye follows, because every pose solves
  the whole thing again.
- **a direction.** Each spike sits on the dome where its `surface` anchor
  says, but `aim=up` stands it straight up rather than along the dome's
  normal.
- **a point.** The crane's boom leaves the side of its mast and aims at
  `[330, 210, 40]` in the thing's space.

The aim tips the axis the shortest way and never rolls the part. `lean`,
`twist` and `pitch` still work, as offsets from the aimed rest.

```moxi
material Iron  { color = "#333333" }
material Brass { color = "#b08d57" }
material Glass { color = yellow }
material Felt  { color = "#2e6b3a" }

thing Watcher(spikes=6) {
    part Table { shape = box(width=420, height=10, depth=220), material = Felt }
    part Post  { shape = cylinder(height=110, radius=5), material = Iron }
    part Eye   { shape = cone(height=40, radius=14), material = Brass }
    part Stick { shape = capsule(height=90, radius=3), material = Iron }
    part Ball  { shape = sphere(radius=12), material = Glass }
    part Dome  { shape = sphere(radius=36), material = Iron }
    part Mast  { shape = cylinder(height=160, radius=6), material = Iron }
    part Boom  { shape = capsule(height=110, radius=4), material = Brass }

    relation {
        Post.bottom  on Table.top shift=(-140, 40)
        Eye.bottom   on Post.top aim=Ball
        Stick.bottom on Table.top shift=(-20, 60) lean=(-25, 0)
        Ball.center  on Stick.top
        Dome.bottom  on Table.top shift=(60, -50)
        Mast.bottom  on Table.top shift=(160, 40)
        Boom.bottom  on Mast.side(t=0.75, angle=90) aim=[330, 210, 40]
    }

    for i in 0..spikes {
        part Spike[i] { shape = cone(height=26, radius=4), material = Brass }
        relation {
            Spike[i].bottom on Dome.surface(yaw=360 * i / spikes, pitch=35) aim=up
        }
    }

    pose Roll { Stick lean=(30, 0) }

    resolve voxel_size = 2
}

print Watcher
```
