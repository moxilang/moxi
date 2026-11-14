# Grove first draft (fixture, Tighten T2): a `for` written inside relation { }

Two trunks of the alien forest, 78 m and 63 m tall: indigo, lumpy, leaning
columns, each one surface with its limbs and its root flare. An orange nerve
system joins them: a great arch and a second strand under it, running
from one trunk into the other, with thin endings that fork off each strand
into the trunk, and glowing buds at the joints.

Nothing here is placed by a world coordinate. The trunks stand on the
ground; every nerve runs between two points on the trunks or the nerve it
forks from, and the compiler works out its path.

Status: built 2026-11-14 to compare against `grove.md` (0.7). The trunks and
their limbs are better as fused tubes; the nerves are worse as spans than
the 0.7 torus arcs. See `assets/LOG.md`.

Units: meters. Origin: on the ground, between the trunks.

```moxi
material Trunk { color = "#3f22b0", roughness = 0.6 }
material Dusk  { color = "#2c1680", roughness = 0.7 }
material Nerve { color = "#d9861c", roughness = 0.5 }
material Bud   { color = "#f2a030", emissive = 0.5, roughness = 0.4 }
material Flesh { color = "#b0307e", roughness = 0.7 }

thing GroveTest {
    part Ground { shape = box(width=160, height=1, depth=80), guide }

    # trunk A: a lumpy column leaning a little, swelling and pinching
    part TrunkA { shape = tube(path=[[-34, 0, 0], [-32.5, 18, -1], [-36, 36, 1.5], [-31.5, 53, 0.5], [-33.5, 67, -1], [-35, 79, 0]],
                               radius=[10, 7.5, 8.5, 6, 7, 5, 5.5]), material = Trunk }
    part LimbA  { shape = tube(radius=[3.6, 3.0, 4.2]), material = Trunk }
    part FlareA { shape = ellipsoid(rx=12, ry=4, rz=11), material = Trunk }

    # trunk B: shorter and thicker, split in two near the top
    part TrunkB { shape = tube(path=[[34, 0, 6], [33, 17, 7], [35.5, 32, 5], [34, 42, 6]],
                               radius=[12, 9, 9.5, 7]), material = Dusk }
    part ForkB1 { shape = tube(radius=[5, 3.6, 4.6]), material = Dusk }
    part ForkB2 { shape = tube(radius=[5, 3.2, 4.2]), material = Dusk }
    part FlareB { shape = ellipsoid(rx=14, ry=4.5, rz=13), material = Dusk }

    part FleshA { shape = box(width=5, height=17, depth=11, round=2), material = Flesh }
    part FleshB { shape = box(width=12, height=13, depth=4.5, round=2), material = Flesh }

    # the nerve: two strands from trunk A into trunk B
    part NerveHigh { shape = tube(radius=[1.4, 1.9, 1.9, 1.4]), material = Nerve }
    part NerveLow  { shape = tube(radius=[1.1, 1.4, 1.1]), material = Nerve }
    # a strand that drops from the arch toward the lower one
    part Drop      { shape = tube(radius=[1.0, 0.7]), material = Nerve }
    # nerve endings gripping each trunk where a strand enters it
    for k in 0..3 {
        part EndA[k] { shape = tube(radius=[0.8, 0.45]), material = Nerve }
        part EndB[k] { shape = tube(radius=[0.8, 0.45]), material = Nerve }
    }

    part Bud1 { shape = sphere(radius=3.0), material = Bud }
    part Bud2 { shape = sphere(radius=2.0), material = Bud }
    part Bud3 { shape = sphere(radius=1.6), material = Bud }

    relation {
        # the trunks are drawn in the grove's own frame, so every point below
        # means the same thing: x along the grove, y up
        TrunkA.origin on Ground.origin
        TrunkB.origin on Ground.origin
        FlareA.center on Ground.point(x=-34, y=2, z=0, nx=0, ny=1, nz=0)
        FlareB.center on Ground.point(x=34, y=2, z=6, nx=0, ny=1, nz=0)
        # on a standing tube, side(angle=…) is 0 north, 90 east, 180 south, 270 west
        LimbA  from TrunkA.side(t=0.35, angle=90) to [-11.5, 55, 0] reach=0.3
        ForkB1 from TrunkB.end to [23, 63, 6] reach=0.25
        ForkB2 from TrunkB.end to [45.5, 58.5, 6] reach=0.25
        FleshA.west on TrunkA.side(t=0.1, angle=40) gap=-2
        FleshB.north on TrunkB.side(t=0.08, angle=330) gap=-2

        NerveHigh from TrunkA.side(t=0.72, angle=90) to ForkB1.side(t=0.55, angle=270) reach=0.12 via=[0, 78, 3]
        NerveLow  from TrunkA.side(t=0.45, angle=70) to TrunkB.side(t=0.55, angle=290) reach=0.12 via=[0, 54, 3]
        Drop      from NerveHigh.side(t=0.3, angle=0) to NerveLow.side(t=0.3, angle=0) reach=0.2

        Bud1.center on NerveHigh.side(t=0.5, angle=0)
        Bud2.center on NerveHigh.side(t=0.3, angle=0)
        Bud3.center on NerveLow.side(t=0.25, angle=0)
        for k in 0..3 {
            EndA[k] from NerveLow.side(t=0.08, angle=k * 120) to TrunkA.side(t=0.42 + 0.04 * k, angle=40 + 30 * k) reach=0.5
            EndB[k] from NerveHigh.side(t=0.93, angle=k * 120) to TrunkB.side(t=0.86 + 0.03 * k, angle=240 + 30 * k) reach=0.5
        }
    }

    fuse TrunkA, LimbA, FlareA blend=3
    fuse TrunkB, ForkB1, ForkB2, FlareB blend=3
    fuse NerveHigh, NerveLow, Drop, EndA, EndB blend=0.8

    resolve grain = 0.5
}
print GroveTest detail=low
```
