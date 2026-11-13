# Pitcher — a lathe and a tube

A pitcher is a turned silhouette and a bent handle. Before 0.8 the body was
a stack of cylinders and cones and the handle a torus cut by boxes, with
the numbers to keep them aligned worked out by hand.

**`lathe` turns a silhouette.** `profile` lists `[radius, height]` points
from the bottom up; the body is the smooth curve through them, spun about
the vertical and closed flat at the foot and the mouth.

**`tube` bends along a path.** `path` lists points in the part's own space;
the tube is the smooth curve through them. `radius` is a number, or a list
spread along the length.

**`origin` shares a frame.** `Handle.origin on Body.origin` puts the handle
in the body's own coordinates, so its path is written against the
silhouette: it leaves the shoulder at radius 3 and rejoins the belly at
radius 3.9. The glaze band sits in the same frame, at the belly.

```moxi
material Clay  { color = "#b6553a" }
material Glaze { color = "#e8dcc4", roughness = 0.35 }

thing Pitcher {
    part Body {
        shape = lathe(profile=[[2.6, 0], [3.6, 1.5], [3.9, 4], [3.1, 7], [2.5, 8.5], [3.0, 9.6]]),
        material = Clay
    }
    part Handle {
        shape = tube(path=[[2.7, 7.8, 0], [5.0, 7.6, 0], [5.7, 5.4, 0], [3.8, 2.8, 0]], radius=[0.5, 0.42, 0.55]),
        material = Clay
    }
    part Band {
        shape = at(torus(major_radius=3.86, minor_radius=0.2), y=3.4),
        material = Glaze
    }

    relation {
        Handle.origin on Body.origin
        Band.origin on Body.origin
    }

    resolve voxel_size = 0.25
}

print Pitcher
```
