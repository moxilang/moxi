# SPROUT — a smooth creature

Two spheres blended into one body: `union(…, blend=2.5)` fillets the join
instead of creasing it, so the neck flows. Eyes are two features on one
face via `shift`, sunk into the surface with a negative `gap`.

```moxi
material Skin { color = "#8fd18f" }
material Eye  { color = black }

thing Sprout {
    part Body {
        shape = union(
            sphere(radius=5),
            at(sphere(radius=3.2), y=6),
            blend=2.5
        ),
        material = Skin
    }
    part EyeL { shape = sphere(radius=0.7), material = Eye }
    part EyeR { shape = sphere(radius=0.7), material = Eye }

    relation {
        EyeL.south on Body.north shift=(4.1, -1.3) gap=-2.4
        EyeR.south on Body.north shift=(4.1,  1.3) gap=-2.4
    }

    resolve voxel_size = 1.0
}

print Sprout detail=low
```

The eye offsets are tuned by eye — `Body.north` sits at the front-center
of the WHOLE blended body (both spheres' combined extents), and `shift`
runs up the meridian to the head. When `surface(u, v)` lands on every
shape (P2) this becomes a single anchor call.