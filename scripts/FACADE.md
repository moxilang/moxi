# Facade — a module that cuts what it sits on

A window is a hole in a wall, a frame and a pane. Before 0.8 the hole had to
be written into the wall, at coordinates matching where the window was
placed; move the window and the hole stayed behind. Here the window brings
its own hole.

**`cuts = host`** makes a part that is not drawn: its shape is removed from
whatever the thing is mated onto (its host). The `Window` below never names
a wall, and the same `Window` cuts a thin wall and a thick one.

**`blend`** rounds the rim of the cut; here the arched door's opening.

```moxi
material Plaster { color = "#d9cfc0" }
material Stone   { color = "#8d8679" }
material Oak     { color = "#6b4a2f" }
material Glass   { color = "#9fc4d6", opacity = 0.5 }

thing Window(w=9, h=12) {
    part Hole  { shape = box(width=w, height=h, depth=60), cuts = host }
    part Frame { shape = difference(box(width=w + 1.2, height=h + 1.2, depth=1.4), box(width=w, height=h, depth=2)), material = Oak }
    part Pane  { shape = box(width=w, height=h, depth=0.2), material = Glass }
    anchor seat = Hole.point(0, 0, 0, 0, 0, -1)
    relation {
        Frame.origin on Hole.origin
        Pane.origin  on Hole.origin
    }
    resolve grain = 0.25
}

thing Door {
    part Hole { shape = union(box(width=10, height=16, depth=60), at(spin(cylinder(height=60, radius=5), axis=x, degrees=90), y=8, z=-30)), cuts = host, blend = 0.6 }
    anchor seat = Hole.point(0, -8, 0, 0, 0, -1)
    resolve grain = 0.25
}

thing Facade {
    part Wall  { shape = box(width=70, height=34, depth=2), material = Plaster }
    part Tower { shape = box(width=24, height=50, depth=8), material = Stone }
    part Entry { thing = Door }
    for k in 0..3 {
        part Win[k] { thing = Window }
        relation { Win[k].seat on Wall.north shift=(5, k * 17 - 8) }
    }
    part Slit { thing = Window(w=4, h=14) }
    relation {
        Tower.west on Wall.east
        Entry.seat on Wall.north shift=(-17, -24)
        Slit.seat on Tower.north shift=(10, 0)
    }
    resolve grain = 0.5
}

print Facade
```
