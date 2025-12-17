// src/geom.rs
// Geometry + transform primitives.
// This file is the single authority for axis parsing + 90° rotation math.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "x" | "X" => Some(Axis::X),
            "y" | "Y" => Some(Axis::Y),
            "z" | "Z" => Some(Axis::Z),
            _ => None,
        }
    }
}

/// Normalize quarter-turns into [0..=3]
#[inline]
pub fn norm_turns(turns: i32) -> i32 {
    ((turns % 4) + 4) % 4
}

/// Rotate a point by 90° increments around an axis.
/// This is the ONLY place the rotation math should exist.
#[inline]
pub fn rotate_point_90(mut x: i32, mut y: i32, mut z: i32, axis: Axis, turns: i32) -> (i32, i32, i32) {
    let turns = norm_turns(turns);

    for _ in 0..turns {
        match axis {
            Axis::X => {
                let ny = -z;
                let nz = y;
                y = ny;
                z = nz;
            }
            Axis::Y => {
                let nx = z;
                let nz = -x;
                x = nx;
                z = nz;
            }
            Axis::Z => {
                let nx = -y;
                let ny = x;
                x = nx;
                y = ny;
            }
        }
    }

    (x, y, z)
}
