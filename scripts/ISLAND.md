# Tropical Island World
> A procedural island with palm trees scattered across biome zones.
> Compile with:  moxi compile scripts/island.md

# Atoms
> Atoms are the lowest-level building blocks. Every material references one.

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
> Cylindrical trunk with organic blob crown.
> Crown is placed above trunk via the relation resolver.

entity PalmTree {
    part Trunk { shape = cylinder(height=6, radius=0.6), material = Bark }
    part Crown { shape = blob(radius=3, roughness=0.35),  material = Leaves }

    relation {
        Crown above Trunk
    }

    resolve voxel_size = 1.0
}

# Island Terrain
> Procedural heightfield. Change seed for a different island shape.

entity Island {
    part Terrain {
        shape    = heightfield(seed=42, radius=40, noise=0.35, max_height=18)
        material = Soil
    }

    resolve voxel_size = 1.0
}

# Generators
> ForestGen: dense coverage on mid-elevation slopes.
> BeachGen:  sparse palms along the shoreline.

generator ForestGen {
    scatter PalmTree
    count       = 60
    min_spacing = 5
    seed        = 7
    where       = elevation > 3 and elevation < 14
}

generator BeachGen {
    scatter PalmTree
    count       = 12
    min_spacing = 6
    seed        = 99
    where       = elevation > 1 and elevation < 4
}

# Output

print Island   detail=low
print PalmTree detail=low