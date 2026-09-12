# char-001 — a simple humanoid face with two eyes, a nose and a mouth

THE headline bench case (see `bench/cases.yaml`'s note on it): before P1,
every mate landed dead-center on its socket, so two eyes could only be
placed via layout sugar (landing on opposite temples — `clusters` passes,
`coplanar` fails) or both on the identical socket point (`coplanar` passes,
`clusters` fails). `shift=` gives every anchor a tangent plane, so both
eyes sit on the SAME face and at the SAME depth, satisfying both at once.

The eye material intentionally uses a reserved bench-only hex
(`"#ff00ff"`), not a real Moxi palette name — this is the `clusters`/
`coplanar` assertions' color tag (`color: eye` in `cases.yaml`), matching
the alias table in `src/bench.rs::COLOR_ALIASES`. It is a bench-harness
convention, not a design choice; do not change it to a named color.

```moxi
material Skin  { color = ivory }
material Eye   { color = "#ff00ff" }
material Nose  { color = brown }
material Mouth { color = black }

thing Face {
    part Head     { shape = box(width=9, height=9, depth=6.5, round=2.0), material = Skin }
    part LeftEye  { shape = sphere(radius=0.8), material = Eye }
    part RightEye { shape = sphere(radius=0.8), material = Eye }
    part Nose     { shape = cone(height=1.5, radius=0.8), material = Nose }
    part Mouth    { shape = ellipsoid(rx=1.6, ry=0.6, rz=0.5), material = Mouth }

    relation {
        LeftEye.south  on Head.north shift=(1.0, -2.2) gap=-0.6
        RightEye.south on Head.north shift=(1.0,  2.2) gap=-0.6
        Nose.south     on Head.north shift=(-1.0, 0.0) gap=-0.3
        Mouth.south    on Head.north shift=(-2.8, 0.0) gap=-0.5
    }

    resolve voxel_size = 0.5
}

print Face detail=low
```
