# Facing — which way the front turns

`facing=` swivels a part about its own axis until its front points as close
as possible to a target. A part's front is its own `north`. The exception is
a part mated by its `north` or `south`, whose front is its own `top`. The
targets are the ones `aim` takes: a direction word, a part, an anchor or a
point.

Three cases, one of each:

- **The choir.** Singers stand on the stage and hang from the gallery, which
  is mounted on the back wall. With a raw `twist`, the same angle turns the
  other way on a bottom face. `facing=north` means north on both.
- **The chest.** The lid sits on a `point` anchor. `facing=north` gives the
  same front whatever the anchor's normal, so no hidden datum decides which
  way it turns.
- **The audience.** Every head faces the angel (`facing=Angel`). Pose
  `Descend` swings the angel's pole, and every head turns to follow it.

`twist` still works, as an offset from the faced rest, and so do poses
and revolves on it.

```moxi
material Wood  { color = "#8a5a36" }
material Robe  { color = "#4b3a7a" }
material Brass { color = "#b08d57" }
material Halo  { color = "#fff2b0", emissive = 1.2 }

thing Hall(singers=4) {
    part Stage   { shape = box(width=400, height=10, depth=260), material = Wood }
    part Back    { shape = box(width=400, height=170, depth=10), material = Wood }
    part Gallery { shape = box(width=400, height=10, depth=60), material = Wood }
    part Chest   { shape = box(width=50, height=30, depth=30), material = Wood }
    part Lid     { shape = box(width=50, height=4, depth=30), material = Brass }
    part Pole    { shape = cylinder(height=150, radius=2), material = Brass }
    part Angel   { shape = sphere(radius=14), material = Halo }

    relation {
        Back.bottom    on Stage.top shift=(0, -125)
        Gallery.south  on Back.north shift=(70, 0)
        Chest.bottom   on Stage.top shift=(150, 90)
        Lid.bottom     on Chest.point(x=0, y=15, z=0, nx=0, ny=1, nz=0) facing=north
        Pole.bottom    on Stage.top shift=(0, 110) lean=(0, -25)
        Angel.center   on Pole.top
    }

    for i in 0..singers {
        part Singer[i] { shape = box(width=18, height=50, depth=6), material = Robe }
        part Hanger[i] { shape = box(width=18, height=40, depth=6), material = Robe }
        part Head[i]   { shape = box(width=12, height=14, depth=10), material = Robe }
        relation {
            Singer[i].bottom on Stage.top shift=(-120 + 60 * i, -40) facing=north
            Hanger[i].top    on Gallery.bottom shift=(-120 + 60 * i, 0) facing=north
            Head[i].bottom   on Singer[i].top facing=Angel
        }
    }

    pose Descend { Pole lean=(0, 25) }

    resolve voxel_size = 2
}

print Hall
```
