# Gears — revolve

`revolve` turns mates forever at a rate, in degrees per second. At time t a
line's qualifier is its rest value plus rate·t: a revolve is poses at
keyframe times, so the solver sees nothing new. `moxi gltf` exports each
revolve as a looping clip whose period is the time after which every part has
made whole turns, with a keyframe at most every 90° so a player's
interpolation can only go the right way.

Two meshing gears on a plate: 12 and 24 teeth, so the small one turns twice
as fast and the other way (`twist` is about the plate's normal — each gear's
axle). The big gear starts half a tooth round (`twist=7.5`) so the teeth
interleave. A flywheel on a post turns on its own clip, about the post's
side normal (its axle).

(Not to be confused with the shape modifier `spin(s, axis=, degrees=)`,
which rotates a shape once, statically.)

```moxi
material Iron  { color = "#4a4a50" }
material Brass { color = "#b08d3c" }
material Steel { color = "#9aa0a8" }

thing Gears(ta=12, tb=24, r=10) {
    part Plate { shape = box(width=120, height=4, depth=70), material = Iron }
    part GearA { shape = cylinder(height=4, radius=r), material = Brass }
    part GearB { shape = cylinder(height=4, radius=r * 2), material = Brass }
    part Post  { shape = cylinder(height=40, radius=2), material = Iron }
    part Fly   { shape = cylinder(height=3, radius=12), material = Steel }
    part Spoke { shape = box(width=22, height=2, depth=3), material = Iron }
    part MarkA { shape = box(width=r * 1.6, height=1, depth=2), material = Iron }
    part MarkB { shape = box(width=r * 3.6, height=1, depth=3), material = Iron }

    relation {
        GearA.bottom on Plate.top shift=(-30, 0)
        GearB.bottom on Plate.top shift=(3.2, 0) twist=7.5
        Post.bottom  on Plate.top shift=(42, 10)
        Fly.bottom   on Post.side(t=0.85, angle=0) gap=-1
        Spoke.center on Fly.top
        MarkA.bottom on GearA.top
        MarkB.bottom on GearB.top
    }

    for k in 0..ta {
        part ToothA[k] { shape = box(width=2.4, height=4, depth=3), material = Brass }
        relation { ToothA[k].bottom on GearA.side(t=0.5, angle=k * 360 / ta) gap=-0.6 }
    }
    for k in 0..tb {
        part ToothB[k] { shape = box(width=2.4, height=4, depth=3), material = Brass }
        relation { ToothB[k].bottom on GearB.side(t=0.5, angle=k * 360 / tb) gap=-0.6 }
    }

    # the small gear twice as fast as the big one, the other way round
    revolve Mesh { GearA twist=90  GearB twist=-90 * ta / tb }
    revolve Roll { Fly twist=120 }

    resolve voxel_size = 0.5
}

print Gears detail=low
```
