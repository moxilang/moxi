# Crag — one shape, one loop, a guide

A crag is a stack of slabs, each a little smaller than the one below and a
little turned. Before 0.8 that took a generator, or nine copied lines: a
`fn` can only return a number, and a `union` took a fixed list.

**`shape` names a shape.** `Slab(w, turn)` is a box rounded at the edges
and spun about the vertical; a call substitutes its arguments for the
parameters, as a `fn` does for a number.

**`for` among the operands.** `for k in 0..9 { … }` inside `union` adds one
operand per turn, so the nine slabs are one line, and `blend` softens the
joins into one rock. The bounds are constants, so the language still ends.

**`rand` makes it uneven, repeatably.** `rand(7, k)` is a number in 0..1
fixed by its arguments: the same crag every run, a different one when the
seed changes.

**`guide` and `origin`.** `Summit` is a guide: solved, so the flag can be
mated to it, and never drawn or exported. `Summit.origin on Body.origin`
puts it in the crag's own frame without a wrapper box, and the marker box
inside it, 28.7 up, is where the flag stands.

```moxi
material Rock { color = "#7d7a74" }
material Cloth { color = "#c0392b" }

shape Slab(w, turn) = spin(box(width=w, height=4, depth=w * 0.8, round=1.2), axis=y, degrees=turn)

thing Crag(seed=7) {
    part Body {
        shape = union(
            for k in 0..9 {
                at(Slab(30 - 2 * k - 3 * rand(seed, k), 23 * k + 40 * rand(seed, k, 1)),
                   x=2 * rand(seed, k, 2) - 1, y=3.4 * k)
            },
            blend = 1.5
        ),
        material = Rock
    }
    part Summit { shape = at(box(width=1, height=1, depth=1), y=28.7), guide }
    part Pole   { shape = cylinder(height=7, radius=0.4), material = Cloth }

    relation {
        Summit.origin on Body.origin
        Pole.bottom on Summit.top
    }

    resolve voxel_size = 1.0
}

print Crag detail=low
```
