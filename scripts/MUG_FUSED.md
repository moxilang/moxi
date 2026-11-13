# Mug, fused — a span that grows into its body

`scripts/MUG.md` builds a mug's handle from a torus cut by boxes, with the
cutting boxes sized from the wall's thickness. Here the handle is a span
between two points on the cup's side, and `fuse` makes cup and handle one
surface with a fillet where they meet.

**`fuse Cup, Handle blend=0.6`** joins the two into one skin. They must wear
one material; the first part named keeps its frame and joint, and the
report lists the handle under it.

```moxi
material Clay { color = "#c46f4a", roughness = 0.4 }

thing Mug {
    part Cup    { shape = difference(cylinder(height=10, radius=4), at(cylinder(height=10, radius=3.4), y=0.6)), material = Clay }
    part Handle { shape = tube(radius=[0.7, 0.55, 0.7]), material = Clay }
    relation {
        Handle from Cup.side(t=0.82, angle=90) to Cup.side(t=0.22, angle=90) reach=0.55
    }
    fuse Cup, Handle blend=0.6
    resolve grain = 0.2
}

print Mug
```
