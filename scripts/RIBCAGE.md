# Ribcage

Twelve pairs of ribs, sized by a formula, mirrored across the spine, with
ten vertebrae down its back. Nothing here is hand-copied: change `pairs`
and the cage rebuilds.

**`fn` is a formula with a name.** `taper(i, n)` rises from near 0 to 1
and back across `n` steps — ribs are widest mid-chest. It is one
expression, it cannot call itself, and it is expanded where it is used.

**`for` repeats everything inside its braces.** Each iteration gets its own
parts (`RibR[i]` becomes `RibR[0]` … `RibR[11]`), its own `let`s, and its
own relations. The range is half-open: `0..pairs` is 0 through
`pairs - 1`. Nothing after compilation knows the loop existed — the
solver sees 35 ordinary parts.

**A computed value passes into an instance.** `Rib(reach=reach)` hands
each rib its own size. The rib itself is a torus arc — half a ring, cut by
a `difference` — the same trick as the mug's handle — then `scale`d
front-to-back so the cage is a barrel, not a cylinder.

**A mirror image, not a copy.** `symmetric_across` reflects the right rib's
shape as well as its placement, so the left rib's half-ring opens the
other way. A half-ring is chiral — it has a handedness — which is why this
script is the one that needed it.

**A list is a table.** The twelve reaches are computed once, by a
comprehension over `taper`, and each rib reads its own entry by index:
`reaches[i]`. Change the formula — or replace the comprehension with a
literal table of measured values — and nothing else moves.

**Refer to one element from anywhere.** `RibR[0]` works outside the loop,
and index arithmetic chains elements: to stack vertebrae on each other,
place the first outside and loop from 1 with
`Vert[k].bottom on Vert[k-1].top`. Here they sit on the spine instead.

```moxi
material Bone { color = ivory }

fn taper(i, n) = sin(180 * (i + 0.5) / n)

# Half a ring: a torus with everything west of its centre cut away, then
# squashed front-to-back by `flat`. The `root` socket is the back tip of
# the arc, where it meets the spine; `scale` carries it along — the tip
# moves in by `flat`, and its normal still faces straight back.
thing Rib(reach=4, thick=0.3, flat=0.72) {
    part Bone {
        shape = scale(
            difference(
                torus(major_radius=reach, minor_radius=thick),
                at(box(width=2*reach + 2, height=2*thick + 2, depth=2*reach + 2*thick + 2),
                   x=0 - reach - 1.3)
            ),
            z=flat
        ),
        material = Bone
    }
    anchor root = Bone.surface(angle=270, phi=0)
    resolve voxel_size = 1.0
}

thing Ribcage(pairs=12, verts=10) {
    part Spine { shape = capsule(height=20, radius=0.8), material = Bone }

    # One table of reaches, widest mid-chest. A measured skeleton would
    # replace this with a literal: `[3.1, 4.4, 5.6, ...]`.
    let reaches = [for i in 0..pairs { 2.5 + 3.5 * taper(i, pairs) }]

    for i in 0..pairs {
        let t = 0.3 + 0.55 * i / pairs

        part RibR[i] { thing = Rib(reach=reaches[i]) }
        part RibL[i] { thing = Rib(reach=reaches[i]) }

        relation {
            RibR[i].root on Spine.side(t=t, angle=0)
            RibL[i] symmetric_across Spine from=RibR[i]
        }
    }

    for k in 0..verts {
        part Vert[k] { shape = box(width=1.6, height=1.1, depth=1.0, round=0.3), material = Bone }
        relation {
            Vert[k].south on Spine.side(t=0.05 + 0.9 * k / (verts - 1), angle=180)
        }
    }

    resolve voxel_size = 0.5
}

print Ribcage detail=low
```

The ribs are horizontal. Real ribs slope downward from the spine; that is
`twist=-90 pitch=…` on the rib mate — see the Placement section of
SKILL.md on why `pitch` alone would sweep them sideways instead.