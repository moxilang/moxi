# Axle — a whole assembly in one shape expression

A wheel-and-axle as ONE shape. `spin` aims each cylinder's axis along X,
`at` puts the wheels on the shaft ends, and `union` fuses them. There are
no relations: the composition happens inside the shape, which is the point
of this script.

**The numbers explain themselves.** An earlier version placed the wheels
at `x=-1` and `x=13` — correct only for a 14-unit shaft with 2-unit
treads, and silently wrong the moment either changed. Those positions are
now `let` bindings derived from the parameters: each wheel is centred on a
shaft end, whatever the shaft's length.

**The assembly answers the compass as a whole.** A union's compass anchors
come from its own combined extents, so `Assembly.top` is the top of the
wheels (y = 4) and `Assembly.east` is the far wheel's outer face — what
you would mount a cart bed or a second axle against. Before that rule,
`Assembly.top` delegated to the first operand, the spun shaft, whose `top`
had been rotated to point along +X: a "top" facing sideways.

```moxi
material Timber { color = brown }

thing Axle(length=14, shaft=0.7, wheel=4, tread=2) {
    # A spun cylinder's base sits at x=0 and runs along +X, so a wheel
    # centred on a shaft end starts half a tread before it.
    let near = 0 - tread / 2
    let far  = length - tread / 2

    part Assembly {
        shape = union(
            spin(cylinder(height=length, radius=shaft), axis=z, degrees=-90),
            at(spin(cylinder(height=tread, radius=wheel), axis=z, degrees=-90), x=near),
            at(spin(cylinder(height=tread, radius=wheel), axis=z, degrees=-90), x=far)
        ),
        material = Timber
    }

    resolve voxel_size = 1.0
}

print Axle detail=low
```