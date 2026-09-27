# Moxi — SKILL.md

Moxi describes a 3D object as **parts** and **how they attach**; the compiler
works out where everything goes. Code lives inside ` ```moxi ` fences —
anything outside a fence is prose and is ignored.

## A complete file

```moxi
material Iron  { color = "#333333" }
material Glass { color = yellow }

thing Lamp(height=300, reach=90) {
    part Base { shape = cylinder(height=12, radius=45), material = Iron }
    part Post { shape = cylinder(height=height, radius=6), material = Iron }
    part Arm  { shape = capsule(height=reach, radius=4), material = Iron }
    part Bulb { shape = sphere(radius=18), material = Glass }

    relation {
        Post.bottom on Base.top
        Arm.bottom  on Post.side(t=0.92, angle=0) lean=(20, 0)
        Bulb.center on Arm.top
    }

    resolve voxel_size = 3
}

print Lamp detail=low
```

- Declare in this order: `material`, then `fn`, then `thing` (a thing before
  any thing that uses it), then one `print`.
- Every `thing` ends with `resolve voxel_size = N`. Pick N near 1/100 of the
  object's largest dimension; smaller is finer and slower.
- Numbers are in whatever unit the task uses. Do not convert.
- Code is ASCII. `#` at the start of a line is a comment.

## Materials

`material Name { color = red }` — named colors: `red orange yellow green blue
purple white black gray brown ivory maroon peach`, or a hex string `"#c96f4a"`.

## Parts and shapes

A part is a shape (`shape = …, material = …`) or an instance of another thing
(`thing = Wing(span=54)`). The shape table is in the reference below.
`sphere`, `box`, `ellipsoid` and `torus` are centered; `cylinder`, `cone` and
`capsule` stand on their base with their axis up.

Combine shapes into one solid with `union(a, b, blend=k)` (blend rounds the
seam), `difference(base, cut, …)` and `intersect(a, b)`. Wrap any shape with
`at(s, x=, y=, z=)`, `spin(s, axis=y, degrees=)`, `mirror(s, axis=x)` or
`scale(s, x=, y=, z=)`. A cut must reach past the faces it opens: a bore
through a 10-tall disc is `at(cylinder(height=12, radius=5), y=-1)`.

## Which way is which

**+Y is up, +Z is the front, +X is right** as seen from the front. On every
shape, `top`/`bottom` face up and down, `north` is the front face, `south` the
back, `east` the right, `west` the left. A character's **own** right side is
`west`: name parts for the character.

## Placing parts

`Part.anchor on Other.anchor` makes the two anchors meet with their normals
opposed: the part points straight out of the anchor it is placed on.
Anchors are the compass faces above plus shape-specific ones —
`side(t, angle)` on cylinders, cones and capsules (t from 0 at the base to 1
at the top, angle 0 = front), `surface(yaw, pitch)` on spheres and
ellipsoids — listed per shape in the reference. `center` has no direction:
the part keeps its own orientation.

Qualifiers, all optional:

- `gap=g` — move along the normal; negative sinks the part in.
- `shift=(a, b)` — slide across the face:

  | Face | `a` | `b` |
  |---|---|---|
  | `north`, `south` | up | right |
  | `east`, `west` | up | front |
  | `top`, `bottom` | right | front |
  | `side`, `surface` | up | around |

- `lean=(a, b)` — tip the part toward those same directions, in degrees.
  On a post's `side`, `lean=(20, 0)` is 20° upward. Use `lean` to angle
  arms, branches and struts.
- `twist=d` — spin the part about the normal.

Shorthand for separate objects: `A above B`, `below`, `left_of`, `right_of`,
`in_front_of`, `behind`. For features **on** a surface (eyes, buttons,
handles), use an explicit mate with `shift`, not `left_of`.

`L symmetric_across Body from=R` makes L the mirror image of R across Body.

**Each part is placed at most once.** A part with no placement sits at the
origin; everything else should hang off it through relations.

## A reusable thing, mirrored

```moxi
material Shell { color = "#2a6f4a" }
material Film  { color = "#cfe8f0" }
material Black { color = black }

thing Wing(span=60, chord=14) {
    part Blade { shape = ellipsoid(rx=span / 2, ry=1, rz=chord / 2), material = Film }
    anchor root = Blade.west
    resolve voxel_size = 1
}

thing Dragonfly {
    part Thorax  { shape = ellipsoid(rx=8, ry=8, rz=14), material = Shell }
    part Tail    { shape = capsule(height=70, radius=3), material = Shell }
    part Head    { shape = sphere(radius=7), material = Shell }
    part EyeL    { shape = sphere(radius=3), material = Black }
    part EyeR    { shape = sphere(radius=3), material = Black }
    part WingFR  { thing = Wing }
    part WingFL  { thing = Wing }
    part WingBR  { thing = Wing(span=54) }
    part WingBL  { thing = Wing(span=54) }

    relation {
        Head.south on Thorax.north gap=-2
        Tail.top   on Thorax.south
        EyeR.center on Head.surface(yaw=-40, pitch=20)
        EyeL symmetric_across Head from=EyeR
        WingFR.root on Thorax.west shift=(4, 5)  lean=(8, 0)
        WingBR.root on Thorax.west shift=(4, -5) lean=(4, 0)
        WingFL symmetric_across Thorax from=WingFR
        WingBL symmetric_across Thorax from=WingBR
    }

    resolve voxel_size = 1
}

print Dragonfly detail=low
```

`anchor root = Blade.west` exports a socket so the instance can be placed by
it; an instance also answers `top`, `north` etc. for its whole assembly.
Instance parts are named `WingFR.Blade`.

## Values, lists and functions

```moxi
material Wood  { color = brown }
material Metal { color = gray }

fn bar_length(i) = 120 - 11 * i

thing Xylophone(n=8, spacing=14) {
    let lengths = [for i in 0..n { bar_length(i) }]
    let longest = if n > 0 { lengths[0] } else { 0 }

    part Frame { shape = box(width=n * spacing, height=6, depth=longest), material = Wood }
    for i in 0..n {
        part Bar[i] { shape = box(width=10, height=4, depth=lengths[i]), material = Metal }
        relation { Bar[i].bottom on Frame.top shift=((i - (n - 1) / 2) * spacing, 0) }
    }
    resolve voxel_size = 1
}

print Xylophone detail=low
```

A thing's parameters need defaults; an instance overrides them with any
expression of the caller's values: `thing = Xylophone(n=count * 2)`. `let`
names a value; `if` is an expression with a mandatory `else`; `fn` is a pure
one-expression function. Lists are `[a, b]` or `[for i in a..b { expr }]`,
indexed from 0 with `xs[i]`, sized with `len(xs)`; a point is `[x, y, z]`.
Math: `sin cos tan sqrt abs floor round pow min max clamp lerp` (angles in
degrees). `-x` works on any expression.

## Repetition

```moxi
material Hull { color = white }
material Trim { color = red }

thing Rocket(fins=4, length=200, radius=20) {
    part Body { shape = capsule(height=length, radius=radius), material = Hull }
    part Nose { shape = cone(height=60, radius=radius), material = Trim }
    relation { Nose.bottom on Body.top gap=-radius }

    for i in 0..fins {
        part Fin[i] { shape = box(width=4, height=50, depth=35), material = Trim }
        relation { Fin[i].south on Body.side(t=0.12, angle=360 / fins * i) }
    }

    resolve voxel_size = 2
}

print Rocket detail=low
```

`for i in a..b { … }` repeats the parts, `relation { … }` blocks and `let`s
inside it for i = a … b−1. `Fin[i]` names each copy; refer to one anywhere as
`Fin[0]`. A ring of copies is `side(angle=360 / n * i)`; a row is
`shift=(i * spacing, 0)`. To chain copies, place the first outside the loop
and loop from 1: `Seg[k].bottom on Seg[k-1].top`. Nested loops make grids.

## Scenes and terrain

A scene is just a thing whose parts are other things, placed by relation
like anything else. `heightfield(seed=, radius=, noise=, max_height=)` is
terrain; a `generator` scatters a thing over the printed scene's surface:

```text
generator Forest {
    scatter Tree
    count = 40   min_spacing = 5   seed = 7
    where = elevation > 3 and slope < 0.5
}
```

## Checks

`constraint A above B` (also `below`, `inside`, `surrounds`) fails the
compile with the measured numbers if the solved geometry disagrees.

## When the compiler says no

Errors name the stage, the location, and the valid choices — an unknown
anchor lists every anchor that shape has. The fix is almost always in the
message.
