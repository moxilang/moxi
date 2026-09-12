# abstract-001 — a tall spiral staircase winding around a central pole

No comprehensions yet (Phase E), so this is ten hand-placed treads around
a central pole, each positioned via the universal `point(x=,y=,z=,free=1)`
escape hatch at hand-computed helix coordinates, and `spin`-rotated so its
long axis points radially outward at that angle. Verbose, but mechanical,
not blocked — unlike `mech-003`/`arch-003`, this case carries no "expected
failure" note in the corpus.

```moxi
material Metal { color = gray }
material Wood  { color = "#8b4513" }

thing Staircase {
    part Pole { shape = cylinder(height=17, radius=0.5), material = Metal }

    part Step0 { shape = spin(box(width=3, height=0.4, depth=1.2), axis=y, degrees=0),   material = Wood }
    part Step1 { shape = spin(box(width=3, height=0.4, depth=1.2), axis=y, degrees=36),  material = Wood }
    part Step2 { shape = spin(box(width=3, height=0.4, depth=1.2), axis=y, degrees=72),  material = Wood }
    part Step3 { shape = spin(box(width=3, height=0.4, depth=1.2), axis=y, degrees=108), material = Wood }
    part Step4 { shape = spin(box(width=3, height=0.4, depth=1.2), axis=y, degrees=144), material = Wood }
    part Step5 { shape = spin(box(width=3, height=0.4, depth=1.2), axis=y, degrees=180), material = Wood }
    part Step6 { shape = spin(box(width=3, height=0.4, depth=1.2), axis=y, degrees=216), material = Wood }
    part Step7 { shape = spin(box(width=3, height=0.4, depth=1.2), axis=y, degrees=252), material = Wood }
    part Step8 { shape = spin(box(width=3, height=0.4, depth=1.2), axis=y, degrees=288), material = Wood }
    part Step9 { shape = spin(box(width=3, height=0.4, depth=1.2), axis=y, degrees=324), material = Wood }

    relation {
        Step0.center on Pole.point(x=4.0,   y=0.0,  z=0.0,   free=1)
        Step1.center on Pole.point(x=3.236, y=1.6,  z=2.351, free=1)
        Step2.center on Pole.point(x=1.236, y=3.2,  z=3.804, free=1)
        Step3.center on Pole.point(x=-1.236,y=4.8,  z=3.804, free=1)
        Step4.center on Pole.point(x=-3.236,y=6.4,  z=2.351, free=1)
        Step5.center on Pole.point(x=-4.0,  y=8.0,  z=0.0,   free=1)
        Step6.center on Pole.point(x=-3.236,y=9.6,  z=-2.351,free=1)
        Step7.center on Pole.point(x=-1.236,y=11.2, z=-3.804,free=1)
        Step8.center on Pole.point(x=1.236, y=12.8, z=-3.804,free=1)
        Step9.center on Pole.point(x=3.236, y=14.4, z=-2.351,free=1)
    }

    resolve voxel_size = 0.5
}

print Staircase detail=low
```
