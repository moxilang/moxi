# char-002 — a snowman

Three stacked spheres, a carrot nose, and two coal buttons on the middle
sphere. The buttons are two features on one curved host surface — the
same `shift=` mechanism as the eyes in `char-001`, in a shape a model will
happily attempt.

```moxi
material Snow   { color = white }
material Carrot { color = orange }
material Coal   { color = black }

thing Snowman {
    part Bottom  { shape = sphere(radius=4), material = Snow }
    part Middle  { shape = sphere(radius=3), material = Snow }
    part Head    { shape = sphere(radius=2), material = Snow }
    part Nose    { shape = cone(height=1.2, radius=0.4), material = Carrot }
    part ButtonA { shape = sphere(radius=0.4), material = Coal }
    part ButtonB { shape = sphere(radius=0.4), material = Coal }

    relation {
        Middle.bottom on Bottom.top gap=-1.5
        Head.bottom   on Middle.top gap=-1
        Nose.south    on Head.north   shift=(0.0, 0.0)  gap=-0.3
        ButtonA.south on Middle.north shift=(1.0, 0.0)  gap=-0.5
        ButtonB.south on Middle.north shift=(-1.0, 0.0) gap=-0.5
    }

    resolve voxel_size = 0.5
}

print Snowman detail=low
```
