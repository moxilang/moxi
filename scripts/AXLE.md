# AXLE — union + spin showcase (requires Phase B2)

A whole wheel-and-axle assembly as ONE shape expression. `spin` aims
each cylinder's axis along X (impossible before B1 — cylinders only
pointed up), `at` places the wheels on the shaft ends, and `union`
fuses them. No relations needed: composition happened inside the shape.

```moxi
atom WOOD { color = brown }

material Timber { color = brown, voxel_atom = WOOD }

entity Axle {
    part Assembly {
        shape = union(
            spin(cylinder(height=14, radius=0.7), axis=z, degrees=-90),
            at(spin(cylinder(height=2, radius=4), axis=z, degrees=-90), x=-1),
            at(spin(cylinder(height=2, radius=4), axis=z, degrees=-90), x=13)
        ),
        material = Timber
    }

    resolve voxel_size = 1.0
}

print Axle detail=low
```