# Worm

A stubby earthworm drawn mostly with marks: two ring bands and two eyes,
each eye with a shine and a glint, on a capsule. Hands and feet are small
spheres. `pose Shut` closes the eyes and hides the shines; `animate Blink`
plays it.

```moxi
material Skin   { color = "#d9a198", roughness = 0.55 }
material Band   { color = "#efcabd", roughness = 0.5 }
material Eye    { color = "#1b141d", roughness = 0.15 }
material Shine  { color = "#ffffff", roughness = 0.3 }

thing Worm(r=20, tall=26) {
    part Body  { shape = capsule(height=tall, radius=r), material = Skin }
    part BandA { mark = band(width=6), material = Band }
    part BandB { mark = band(width=6), material = Band }
    part EyeR  { mark = oval(width=8.6, height=10.8), material = Eye }
    part EyeL  { mark = oval(width=8.6, height=10.8), material = Eye }
    part ShineR  { mark = oval(width=2.6, height=3.0), material = Shine }
    part ShineL  { mark = oval(width=2.6, height=3.0), material = Shine }
    part GlintR  { mark = oval(width=1.1, height=1.1), material = Shine }
    part GlintL  { mark = oval(width=1.1, height=1.1), material = Shine }
    part HandR { shape = sphere(radius=5.6), material = Skin }
    part HandL { shape = sphere(radius=5.6), material = Skin }
    part FootR { shape = sphere(radius=5.8), material = Skin }
    part FootL { shape = sphere(radius=5.8), material = Skin }
    relation {
        BandA on Body.side(t=0.385)
        BandB on Body.side(t=0.06)
        EyeR on Body.side(t=0.86, angle=-24)
        EyeL symmetric_across Body from=EyeR
        ShineR on EyeR.center shift=(2.2, -1.4)
        ShineL on EyeL.center shift=(2.2, -1.4)
        GlintR on EyeR.center shift=(-2.4, 1.6)
        GlintL on EyeL.center shift=(-2.4, 1.6)
        HandR.center on Body.side(t=0.38, angle=-90) gap=2.5
        HandL symmetric_across Body from=HandR
        FootR.center on Body.point(x=-11, y=-17.5, z=3, nx=0, ny=-1, nz=0)
        FootL symmetric_across Body from=FootR
    }
    pose Shut     { EyeR height=0.8  ShineR height=0.01  GlintR height=0.01  ShineL height=0.01  GlintL height=0.01 }
    animate Blink { 0 rest  0.08 Shut  0.16 rest }
    resolve grain = 0.4
}
print Worm
```
