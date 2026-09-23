# Mug — a difference, twice

A hollow body is a difference, not a special shape: the cup is a cylinder
minus a narrower one floated up by the wall thickness, which leaves a
floor. And the handle is a difference too — a torus stood on edge, with
the half nearest the mug cut away.

**Anchors pass through CSG.** `Body.side(t, angle)` still works on a
difference: it resolves on the base cylinder inside it, so a hollowed body
keeps its whole anchor vocabulary. The handle shows the same principle
from the other side — `Handle.center` is the centre of the full ring even
though half of it has been cut away, because a difference delegates its
anchors to its base.

**`center` needs no tuning.** It is orientation-free: the mate makes the
two points coincide and the handle inherits the body's rotation. The
ring's centre lands on the mug's surface, and the cut is placed just past
it so the handle's two ends sink slightly into the wall instead of
touching it at a seam.

**Parameters, derived.** Wall thickness sets both the floor and the bore;
the handle's tube scales with the wall, and every cutting box is sized
from the ring it cuts. Change `grip` and the cut follows.

```moxi
material Ceramic { color = "#c96f4a" }

thing Mug(height=10, radius=5, wall=1, grip=2.6) {
    let bore  = radius - wall
    let tube  = wall * 0.6
    let reach = grip + tube
    # The cut keeps everything east of x = -0.4 in the handle's own frame:
    # a sliver past the ring's centre, so the ends embed in the wall.
    let cut   = 0 - reach - 0.4

    part Body {
        shape = difference(
            cylinder(height=height, radius=radius),
            at(cylinder(height=height, radius=bore), y=wall)
        ),
        material = Ceramic
    }

    # A torus lies in the XZ plane; spinning it 90 degrees about X stands
    # it up in the XY plane, which contains the mug's axis and its +X side.
    part Handle {
        shape = difference(
            spin(torus(major_radius=grip, minor_radius=tube), axis=x, degrees=90),
            at(box(width=2*reach, height=2*reach + 2, depth=2*tube + 2), x=cut)
        ),
        material = Ceramic
    }

    relation {
        # The ring's centre on the body's east side. `side` resolves on the
        # outer cylinder inside the difference; `center` resolves on the
        # full ring inside the other one.
        Handle.center on Body.side(t=0.55, angle=90)
    }

    resolve voxel_size = 1.0
}

print Mug detail=low
```