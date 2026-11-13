# Lantern — spans

A rope between two posts, a lantern hanging from it, and the lantern's
handle. Before 0.8 each curve was a list of world coordinates worked out
from where the posts stood; move a post and every number was wrong.

**A span is a tube placed between two ends.** `Rope from PostA.top to
PostB.top` names the ends and the solver works out the path: no coordinate
of either end is written down. Move a post and the rope follows.

**`sag` and `rise` bend it.** `reach=0` makes the rope run straight from end
to end; `sag=3` hangs its middle three units below that line. With the
default `reach`, a span leaves each end straight out along the surface, so
`Handle from Body.rim_top(angle=90) to Body.rim_top(angle=270)` arches up
off the lantern's rim and back down.

**Parts can hang on a span.** The rope's `bottom` is its lowest point, so
`Lamp.top on Rope.bottom` hangs the lantern there (an instance's `top`
is the top of the whole lantern, its handle and bail included).

```moxi
material Wood  { color = "#7a5a3c" }
material Rope  { color = "#c9b48a" }
material Iron  { color = "#3b3b3f", roughness = 0.5, metalness = 0.6 }
material Glass { color = "#ffcf6b", emissive = 1.5 }

thing Lantern {
    part Body   { shape = lathe(profile=[[1.2, 0], [1.5, 0.6], [1.5, 2.6], [0.9, 3.3], [0.9, 3.6]]), material = Glass }
    part Handle { shape = tube(radius=0.12), material = Iron }
    part Bail   { shape = sphere(radius=0.25), material = Iron }
    relation {
        Handle from Body.rim_top(angle=90) to Body.rim_top(angle=270) reach=0.9
        Bail.bottom on Handle.top
    }
    resolve grain = 0.1
}

thing Line {
    part Ground { shape = box(width=30, height=1, depth=8), material = Wood }
    part PostA  { shape = cylinder(height=12, radius=0.4), material = Wood }
    part PostB  { shape = cylinder(height=12, radius=0.4), material = Wood }
    part Rope   { shape = tube(radius=0.15), material = Rope }
    part Lamp   { thing = Lantern }
    relation {
        PostA.bottom on Ground.top shift=(-12, 0)
        PostB.bottom on Ground.top shift=(12, 0)
        Rope from PostA.top to PostB.top reach=0 sag=3
        Lamp.top on Rope.bottom
    }
    resolve grain = 0.25
}

print Line
```
