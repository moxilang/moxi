# mech-002 — a wheel and axle: two wheels on the ends of a horizontal shaft

Three separate parts (axle + two wheels), not one fused shape — `parts_min`
requires real decomposition, not a single union standing in for three
things. Both the axle and each wheel are `spin`-rotated so their own
+Y axis (the cylinder's natural axis) points along world +X, matching
`scripts/AXLE.md`'s "axis along X" idiom. `Axle.top`/`Axle.bottom` land
exactly on the shaft's two ends after the spin, with outward-pointing
normals — mating each wheel's own `top`/`bottom` cap there needs no extra
rotation beyond a twist (invisible on a rotationally-symmetric wheel), so
the flat wheel faces stay perpendicular to the shaft.

```moxi
material Metal { color = gray }

thing WheelAxle {
    part Axle   { shape = spin(cylinder(height=14, radius=0.6), axis=z, degrees=-90), material = Metal }
    part WheelR { shape = spin(cylinder(height=1.5, radius=4),  axis=z, degrees=-90), material = Metal }
    part WheelL { shape = spin(cylinder(height=1.5, radius=4),  axis=z, degrees=-90), material = Metal }

    relation {
        WheelR.bottom on Axle.top
        WheelL.top    on Axle.bottom
    }

    resolve voxel_size = 0.5
}

print WheelAxle detail=low
```
