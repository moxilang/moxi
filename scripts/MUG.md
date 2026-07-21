# MUG — CSG showcase (requires Phase B2)

> A hollow body is a difference, not a special shape: the cup is a
> cylinder minus a shorter cylinder floated up 1 unit (leaving a bottom).
> The handle is a C — a box minus a box shifted toward the mug. And the
> mate still works: `Body.side(…)` passes through the difference to its
> base cylinder, so CSG shapes keep their anchor vocabulary.

atom CLAY { color = "#c96f4a" }

material Ceramic { color = "#c96f4a", voxel_atom = CLAY }

entity Mug {
    part Body {
        shape = difference(
            cylinder(height=10, radius=5),
            at(cylinder(height=10, radius=4), y=1)
        ),
        material = Ceramic
    }

    part Handle {
        shape = difference(
            box(width=4, height=8, depth=2),
            at(box(width=4, height=4, depth=3), x=-1.5)
        ),
        material = Ceramic
    }

    relation {
        # The handle's west face mates the body's east side — the side()
        # anchor resolves on the OUTER cylinder inside the difference.
        Handle.west on Body.side(t=0.55, angle=90)
    }

    resolve voxel_size = 1.0
}

print Mug detail=low
