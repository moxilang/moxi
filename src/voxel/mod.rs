/// The flat 3D grid — the floor of the entire pipeline.
///
/// Every cell holds a u16 atom ID.  0 = empty air.
/// Coordinates are integer (x, y, z) with y = up.
#[derive(Debug, Clone)]
pub struct VoxelGrid {
    pub width:  u32,
    pub height: u32,
    pub depth:  u32,
    data: Vec<u16>,
}

impl VoxelGrid {
    pub fn new(width: u32, height: u32, depth: u32) -> Self {
        let size = (width * height * depth) as usize;
        Self { width, height, depth, data: vec![0u16; size] }
    }

    #[inline]
    pub fn in_bounds(&self, x: i32, y: i32, z: i32) -> bool {
        x >= 0 && y >= 0 && z >= 0
            && x < self.width  as i32
            && y < self.height as i32
            && z < self.depth  as i32
    }

    #[inline]
    fn index(&self, x: u32, y: u32, z: u32) -> usize {
        (x + y * self.width + z * self.width * self.height) as usize
    }

    /// Set a voxel.  Silent no-op if out of bounds.
    pub fn set(&mut self, x: i32, y: i32, z: i32, atom_id: u16) {
        if self.in_bounds(x, y, z) {
            let idx = self.index(x as u32, y as u32, z as u32);
            self.data[idx] = atom_id;
        }
    }

    /// Get a voxel.  Returns 0 (air) if out of bounds.
    pub fn get(&self, x: i32, y: i32, z: i32) -> u16 {
        if self.in_bounds(x, y, z) {
            let idx = self.index(x as u32, y as u32, z as u32);
            self.data[idx]
        } else {
            0
        }
    }

    /// Count non-empty cells.
    pub fn filled_count(&self) -> usize {
        self.data.iter().filter(|&&v| v != 0).count()
    }

    /// Iterator over all non-empty voxels: (x, y, z, atom_id)
    pub fn iter_filled(&self) -> impl Iterator<Item = (u32, u32, u32, u16)> + '_ {
        let w = self.width;
        let h = self.height;
        self.data.iter().enumerate().filter_map(move |(i, &v)| {
            if v == 0 { return None; }
            let i   = i as u32;
            let x   = i % w;
            let y   = (i / w) % h;
            let z   = i / (w * h);
            Some((x, y, z, v))
        })
    }

    /// Dimensions as a tuple.
    pub fn dims(&self) -> (u32, u32, u32) {
        (self.width, self.height, self.depth)
    }
}