# mech-001 — a hollow cylindrical pipe with an open bore through it

Reference solution for the `mech-001` bench case. Pure CSG: the bore
cylinder is taller than the pipe and offset past both ends, so it
perforates cleanly through rather than leaving a cap at either end.

```moxi
material Metal { color = gray }

thing Pipe {
    part Body {
        shape = difference(
            cylinder(height=20, radius=5),
            at(cylinder(height=22, radius=3.5), y=-1)
        ),
        material = Metal
    }
    resolve voxel_size = 1.0
}

print Pipe detail=low
```
