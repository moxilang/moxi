# Tropical Island World
> A procedural island with palm trees, beach, rocky peaks, and ocean.
> Compile with:  moxi compile scripts/ISLAND.md

# Design notes
> DETERMINISM: Use cylinder for flat layers (ocean, sand). Cylinders are
> fully deterministic — same output every run. Heightfields with noise > 0
> produce ragged edges that vary with floating point rounding at grid
> boundaries, even with the same seed. Never use heightfield for layers
> that need a clean consistent boundary (ocean, beach).
>
> LAYER ORDER: Render bottom to top. Each entity overwrites voxels below it.
> Ocean first (widest), then sand, then soil, then rock (narrowest but
> tallest). Soil covers the center of the sand disc leaving a ring visible
> at the coastline. Rock renders last so it paints over soil at the peaks.
>
> BEACH RING WIDTH: sand_radius(55) - soil_radius(40) = 15 voxel wide ring.
> Widen beach by increasing sand_radius or decreasing soil_radius.
>
> ROCKY PEAKS: same seed as SoilTerrain so noise pattern aligns spatially.
> Larger radius (35) and max_height (20) makes rocks dominate the summit.
> Renders after soil so rock is always visible on top.
>
> OCEAN SIZE: radius=200 makes the ocean extend to the horizon in the viewer.
> Reduce to 74 for a tighter view that shows the full ocean disc.

# Tropical Island World - Moxi Code

# Atoms
> Atoms are the atomic unit. Every material maps to exactly one atom.
> Two atoms can share a color (TRUNK and SOIL both brown) but remain
> semantically distinct — useful for future material logic and gameplay.

atom SAND   { color = yellow }
atom SOIL   { color = brown }
atom ROCK   { color = gray }
atom WATER  { color = blue }
atom TRUNK  { color = brown }
atom LEAF   { color = green }

# Materials

material Sand   { color = yellow, voxel_atom = SAND }
material Soil   { color = brown,  voxel_atom = SOIL }
material Rock   { color = gray,   voxel_atom = ROCK }
material Ocean  { color = blue,   voxel_atom = WATER }
material Bark   { color = brown,  voxel_atom = TRUNK }
material Leaves { color = green,  voxel_atom = LEAF }

# Palm Tree
> Crown above Trunk places the blob canopy on top of the cylinder trunk.
> The relation resolver computes the exact y offset from the trunk height.

entity PalmTree {
    part Trunk { shape = cylinder(height=6, radius=0.6), material = Bark }
    part Crown { shape = blob(radius=3, roughness=0.35),  material = Leaves }
    relation {
        Crown above Trunk
    }
    resolve voxel_size = 1.0
}

# Ocean
> Flat cylinder. Fully deterministic. Height=1, radius=200.
> The compiler sinks non-heightfield entities so the top face sits at y=0.
> Radius 200 makes the ocean extend to the horizon in the viewer.
> Reduce to 74 if you want to see the full ocean disc from above.

entity Ocean {
    part Water {
        shape    = cylinder(height=1, radius=200)
        material = Ocean
    }
    resolve voxel_size = 1.0
}

# Sand
> Flat cylinder. Fully deterministic. Always a clean ring every run.
> Beach ring width = sand_radius(55) - soil_radius(40) = 15 voxels.
> DO NOT replace with heightfield — noise makes the ring width non-deterministic.

entity SandBase {
    part Shore {
        shape    = cylinder(height=1, radius=55)
        material = Sand
    }
    resolve voxel_size = 1.0
}

# Soil terrain
> Heightfield — the main island landmass. Noise gives organic coastline shape.
> Radius 40 sits inside sand radius 55, so the sand ring is always exposed.

entity SoilTerrain {
    part Body {
        shape    = heightfield(seed=42, radius=40, noise=0.35, max_height=18)
        material = Soil
    }
    resolve voxel_size = 1.0
}

# Rocky peaks
> Same seed as SoilTerrain so the noise pattern aligns with the terrain.
> Larger radius (35) and higher max_height (20) makes the summit rocky.
> Higher noise (0.6) gives jagged appearance compared to smooth soil (0.35).
> Renders last among terrain layers so rock is always visible on top.

entity RockyPeaks {
    part Peaks {
        shape    = heightfield(seed=42, radius=35, noise=0.6, max_height=20)
        material = Rock
    }
    resolve voxel_size = 1.0
}

# Generators
> Generators scatter entities over the primary terrain (SoilTerrain).
> The where condition samples elevation at each candidate position.
> min_spacing enforces a minimum distance between placed instances.
> Change seed for a different placement pattern with the same density.

generator ForestGen {
    scatter PalmTree
    count       = 60
    min_spacing = 5
    seed        = 7
    where       = elevation > 3 and elevation < 13
}

generator BeachGen {
    scatter PalmTree
    count       = 10
    min_spacing = 7
    seed        = 99
    where       = elevation > 1 and elevation < 3
}

# Output
> Bottom to top render order — ocean first, rocks last, trees over everything.

print Ocean       detail=low
print SandBase    detail=low
print SoilTerrain detail=low
print RockyPeaks  detail=low
print PalmTree    detail=low