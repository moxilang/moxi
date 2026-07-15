// src/frame.rs
//
// Rigid transforms ("frames") for the placement solver.
//
// A Frame is (R, t): rotation + translation, in continuous world units (f64).
// The solver works entirely in frames — it never touches voxels. Voxelization
// (or, later, mesh/simplex realization) happens in a single realization step
// at the end of the pipeline. See `snap_axis_aligned` for the phase-1 bridge.
//
// Dependency-free by design, matching the rest of the codebase.

// ── Vec3 ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 { x: 0.0, y: 0.0, z: 0.0 };
    pub const X:    Vec3 = Vec3 { x: 1.0, y: 0.0, z: 0.0 };
    pub const Y:    Vec3 = Vec3 { x: 0.0, y: 1.0, z: 0.0 };
    pub const Z:    Vec3 = Vec3 { x: 0.0, y: 0.0, z: 1.0 };

    pub fn new(x: f64, y: f64, z: f64) -> Self { Self { x, y, z } }

    pub fn add(self, o: Vec3) -> Vec3 { Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z) }
    pub fn sub(self, o: Vec3) -> Vec3 { Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z) }
    pub fn scale(self, s: f64) -> Vec3 { Vec3::new(self.x * s, self.y * s, self.z * s) }
    pub fn neg(self) -> Vec3 { self.scale(-1.0) }

    pub fn dot(self, o: Vec3) -> f64 { self.x * o.x + self.y * o.y + self.z * o.z }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f64 { self.dot(self).sqrt() }

    /// Returns None for near-zero vectors instead of exploding.
    pub fn normalize(self) -> Option<Vec3> {
        let len = self.length();
        if len < 1e-12 { None } else { Some(self.scale(1.0 / len)) }
    }
}

// ── Mat3 (rotation matrices) ───────────────────────────────────────────────
//
// Column-major meaning: columns are the images of the basis vectors.
// Only proper rotations are stored here; reflections are applied via
// the sandwich M·R·M in the mirror placement (which yields a proper
// rotation again — det(M)² = 1).

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat3(pub [[f64; 3]; 3]); // row-major storage: self.0[row][col]

impl Mat3 {
    pub const IDENTITY: Mat3 = Mat3([
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ]);

    /// 180° about X — the mate flip. Maps +Y → −Y (normals oppose)
    /// while keeping +X (tangents align X-to-X at twist 0).
    pub const FLIP_X: Mat3 = Mat3([
        [1.0,  0.0,  0.0],
        [0.0, -1.0,  0.0],
        [0.0,  0.0, -1.0],
    ]);

    pub fn rot_x(a: f64) -> Mat3 {
        let (s, c) = a.sin_cos();
        Mat3([
            [1.0, 0.0, 0.0],
            [0.0,   c,  -s],
            [0.0,   s,   c],
        ])
    }

    pub fn rot_y(a: f64) -> Mat3 {
        let (s, c) = a.sin_cos();
        Mat3([
            [  c, 0.0,   s],
            [0.0, 1.0, 0.0],
            [ -s, 0.0,   c],
        ])
    }

    pub fn rot_z(a: f64) -> Mat3 {
        let (s, c) = a.sin_cos();
        Mat3([
            [  c,  -s, 0.0],
            [  s,   c, 0.0],
            [0.0, 0.0, 1.0],
        ])
    }

    /// Build a rotation from three orthonormal columns (x, y, z basis images).
    pub fn from_cols(x: Vec3, y: Vec3, z: Vec3) -> Mat3 {
        Mat3([
            [x.x, y.x, z.x],
            [x.y, y.y, z.y],
            [x.z, y.z, z.z],
        ])
    }

    pub fn col(&self, i: usize) -> Vec3 {
        Vec3::new(self.0[0][i], self.0[1][i], self.0[2][i])
    }

    pub fn mul(&self, o: &Mat3) -> Mat3 {
        let mut out = [[0.0f64; 3]; 3];
        for r in 0..3 {
            for c in 0..3 {
                out[r][c] = (0..3).map(|k| self.0[r][k] * o.0[k][c]).sum();
            }
        }
        Mat3(out)
    }

    pub fn apply(&self, v: Vec3) -> Vec3 {
        Vec3::new(
            self.0[0][0] * v.x + self.0[0][1] * v.y + self.0[0][2] * v.z,
            self.0[1][0] * v.x + self.0[1][1] * v.y + self.0[1][2] * v.z,
            self.0[2][0] * v.x + self.0[2][1] * v.y + self.0[2][2] * v.z,
        )
    }

    /// Transpose == inverse, for proper rotations.
    pub fn transpose(&self) -> Mat3 {
        let m = &self.0;
        Mat3([
            [m[0][0], m[1][0], m[2][0]],
            [m[0][1], m[1][1], m[2][1]],
            [m[0][2], m[1][2], m[2][2]],
        ])
    }
}

// ── Frame ──────────────────────────────────────────────────────────────────

/// A rigid transform: p_world = rot · p_local + pos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub rot: Mat3,
    pub pos: Vec3,
}

impl Frame {
    pub const IDENTITY: Frame = Frame { rot: Mat3::IDENTITY, pos: Vec3::ZERO };

    pub fn new(rot: Mat3, pos: Vec3) -> Self { Self { rot, pos } }
    pub fn from_rot(rot: Mat3) -> Self { Self { rot, pos: Vec3::ZERO } }
    pub fn from_pos(pos: Vec3) -> Self { Self { rot: Mat3::IDENTITY, pos } }

    /// self ∘ other — apply `other` first, then `self`.
    pub fn compose(&self, other: &Frame) -> Frame {
        Frame {
            rot: self.rot.mul(&other.rot),
            pos: self.rot.apply(other.pos).add(self.pos),
        }
    }

    pub fn inverse(&self) -> Frame {
        let rt = self.rot.transpose();
        Frame {
            rot: rt,
            pos: rt.apply(self.pos).neg(),
        }
    }

    pub fn apply_point(&self, p: Vec3) -> Vec3 { self.rot.apply(p).add(self.pos) }
    pub fn apply_dir(&self, d: Vec3) -> Vec3 { self.rot.apply(d) }
}

/// Build a frame at `pos` whose +Y is `normal` and whose +X is `x_hint`
/// projected onto the tangent plane. This is THE anchor-frame constructor:
/// every anchor in anchors.rs goes through it, so the convention
/// (+Y = outward normal, +X = tangent reference, +Z = X × Y) lives in
/// exactly one place.
///
/// Falls back to world +X, then +Z, if the hint is (near-)parallel to
/// the normal — e.g. at sphere poles.
pub fn frame_from_normal(pos: Vec3, normal: Vec3, x_hint: Vec3) -> Frame {
    let y = normal.normalize().unwrap_or(Vec3::Y);

    let project = |v: Vec3| -> Option<Vec3> {
        v.sub(y.scale(v.dot(y))).normalize()
    };

    let x = project(x_hint)
        .or_else(|| project(Vec3::X))
        .or_else(|| project(Vec3::Z))
        .unwrap_or(Vec3::X);

    let z = x.cross(y); // right-handed: Z = X × Y

    Frame::new(Mat3::from_cols(x, y, z), pos)
}

// ── Axis-aligned classifier (phase-1 realization bridge) ──────────────────
//
// The current stamp-then-offset voxel pipeline can realize exactly the 24
// proper axis-aligned orientations (signed permutation matrices with
// determinant +1). This classifier snaps a solved rotation to one of them,
// or reports that it can't — which the realizer turns into a clear
// "not yet supported by the voxel backend" error, naming the capability
// rather than silently resampling.
//
// When shapes become containment functions, the stamper tests
// contains(F⁻¹ · p) per voxel and this restriction is deleted, not patched.

/// Snap `m` to an exact signed permutation matrix if every entry is within
/// `eps` of {−1, 0, +1} and the result is a proper rotation. Returns None
/// otherwise.
pub fn snap_axis_aligned(m: &Mat3, eps: f64) -> Option<Mat3> {
    let mut snapped = [[0.0f64; 3]; 3];

    for r in 0..3 {
        for c in 0..3 {
            let v = m.0[r][c];
            snapped[r][c] = if (v - 1.0).abs() < eps {
                1.0
            } else if (v + 1.0).abs() < eps {
                -1.0
            } else if v.abs() < eps {
                0.0
            } else {
                return None;
            };
        }
    }

    let s = Mat3(snapped);

    // Verify: exactly one nonzero per row and column (signed permutation) …
    for i in 0..3 {
        let row_nz = (0..3).filter(|&c| s.0[i][c] != 0.0).count();
        let col_nz = (0..3).filter(|&r| s.0[r][i] != 0.0).count();
        if row_nz != 1 || col_nz != 1 {
            return None;
        }
    }

    // … and proper (det = +1). det of a signed permutation is ±1.
    let d = det3(&s);
    if (d - 1.0).abs() > 1e-9 {
        return None;
    }

    Some(s)
}

fn det3(m: &Mat3) -> f64 {
    let a = &m.0;
    a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn approx(a: Vec3, b: Vec3) -> bool {
        (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS && (a.z - b.z).abs() < EPS
    }

    #[test]
    fn compose_then_inverse_is_identity() {
        let f = Frame::new(Mat3::rot_y(0.7), Vec3::new(3.0, -2.0, 5.0));
        let g = f.compose(&f.inverse());
        assert!(approx(g.apply_point(Vec3::new(1.0, 2.0, 3.0)), Vec3::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn flip_x_opposes_y_keeps_x() {
        assert!(approx(Mat3::FLIP_X.apply(Vec3::Y), Vec3::Y.neg()));
        assert!(approx(Mat3::FLIP_X.apply(Vec3::X), Vec3::X));
    }

    #[test]
    fn frame_from_normal_is_right_handed() {
        let f = frame_from_normal(Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0), Vec3::Y);
        let x = f.rot.col(0);
        let y = f.rot.col(1);
        let z = f.rot.col(2);
        assert!(x.dot(y).abs() < EPS);
        assert!(approx(x.cross(y), z));
    }

    #[test]
    fn snap_accepts_quarter_turn_rejects_arbitrary() {
        assert!(snap_axis_aligned(&Mat3::rot_y(std::f64::consts::FRAC_PI_2), 1e-6).is_some());
        assert!(snap_axis_aligned(&Mat3::rot_y(0.3), 1e-6).is_none());
    }
}
