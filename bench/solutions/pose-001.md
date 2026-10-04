material Iron   { color = "#2b2b2b" }
material Enamel { color = "#c8442c" }
material Glow   { color = "#fff2b0", emissive = 1.5 }

thing DeskLamp(upper=60, fore=55) {
    part Base  { shape = cylinder(height=6, radius=26), material = Iron }
    part Upper { shape = capsule(height=upper, radius=2.5), material = Iron }
    part Fore  { shape = capsule(height=fore, radius=2.2), material = Iron }
    part Shade { shape = cone(height=22, radius=16), material = Enamel }
    part Bulb  { shape = sphere(radius=6), material = Glow }

    relation {
        Upper.bottom on Base.top lean=(15, 0)
        Fore.bottom  on Upper.top lean=(-80, 0)
        Shade.top    on Fore.top lean=(-60, 0)
        Bulb.center  on Shade.bottom gap=3
    }

    anchor foot = Base.bottom

    joint Upper { lean=(0..45, 0..0) }
    joint Fore  { lean=(-160..-15, 0..0) }
    joint Shade { lean=(-110..0, 0..0) }

    pose Reading { Upper lean=(25, 0)  Fore lean=(-50, 0)  Shade lean=(-80, 0) }
    pose Folded  { Upper lean=(5, 0)   Fore lean=(-130, 0) Shade lean=(-20, 0) }
    pose Craning { Upper lean=(40, 0)  Fore lean=(-20, 0)  Shade lean=(-100, 0) }

    constraint Shade above Base

    resolve voxel_size = 1
}

print DeskLamp detail=low
