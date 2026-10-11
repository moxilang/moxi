# GAZE — a named point, a constraint that travels with its thing, `print A, B`

`let eye = [x, y, z]` names a point once; `aim=eye` (in a mate or a pose)
turns every part toward it. A `constraint` written inside `Lamp` is checked
under each instance's own arguments, so `Hall` below is checked twice.

```moxi
material Brass { color = "#b8893a" }
material Glass { color = "#cfe8ff", emissive = 0.5 }

thing Lamp(reach=20) {
    let eye = [reach, 60, 30]
    part Base { shape = cylinder(height=4, radius=8), material = Brass }
    part Neck { shape = capsule(height=24, radius=2), material = Brass }
    part Bulb { shape = sphere(radius=4), material = Glass }
    relation {
        Neck.bottom on Base.top aim=eye
        Bulb.center on Neck.top
    }
    pose Away { Neck aim=[0, 60, -30] }
    constraint Bulb above Base
}

thing Hall {
    part Floor { shape = box(width=120, height=2, depth=60), material = Brass }
    part Left  { thing = Lamp(reach=20) }
    part Right { thing = Lamp(reach=-20) }
    relation {
        Left.bottom  on Floor.top shift=(-30, 0)
        Right.bottom on Floor.top shift=(30, 0)
    }
}

print Hall
```
