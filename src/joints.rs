//! The joint tree a solved thing already has.
//!
//! Every mate `A.a on B.b …` is solved (frame_resolver::solve_one) as
//!
//! ```text
//! F_A = W · slide · lean · twist · pitch · flip · a⁻¹
//! ```
//!
//! with W = B's socket in the world. So `F_A · a` — A's own anchor frame,
//! after solving — is the socket after the slide and the qualifier
//! rotations: a JOINT frame, sitting exactly on the mate point. Each part is
//! placed at most once, so the relations form a tree: the object of A's
//! mate is A's parent.
//!
//! This module only READS solved frames; it never solves. It is the
//! substrate for `moxi gltf` (one node per part, hierarchy = this tree,
//! node origin = the joint frame) and, later, for poses
//! (DOC-20261004-living-models-design).
//!
//! Rules:
//! - `Align`: parent = object part (if it is a real part of this thing),
//!   joint = `F_A · a_subj`. For an orientation-free mate (`center`,
//!   `free=1`) `a_subj` is a pure translation, so the joint is the mate
//!   point carrying the part's own rotation.
//! - `Mirror` (`symmetric_across`): the image's joint is the mirror image of
//!   its source's joint (a proper frame: M·J·M), and its parent is the
//!   image of the source's parent when that parent was mirrored too
//!   (`LeftArm.Forearm` hangs from `LeftArm.Humerus`), else the source's
//!   parent itself (`LeftArm.Humerus` hangs from `Ribcage`, like its source).
//! - Unplaced (the root): no parent, joint = its frame.
//!
//! Invariant every consumer relies on: the joint frame is a rigid frame of
//! the thing's space, so `joint⁻¹ · F_A` maps shape-local geometry into
//! joint space and `joint · (joint⁻¹ · F_A) = F_A` — the world geometry is
//! unchanged by construction, whatever the hierarchy.

use std::collections::HashMap;

use crate::anchors::resolve_anchor;
use crate::ast::{Placement, ShapeExpr};
use crate::frame::{Frame, Mat3, Vec3};

#[derive(Debug, Clone, PartialEq)]
pub struct Joint {
    /// The part this one hangs from; `None` for a root.
    pub parent: Option<String>,
    /// The joint frame in the thing's space: origin at the mate point,
    /// axes = the socket's after shift/gap/lean/twist/pitch/flip.
    pub frame:  Frame,
}

/// Joint for every part in `frames`.
///
/// `parts` are the UNWRAPPED shapes (before `symmetric_across` images get
/// their `mirror` wrapper): subject anchors of `Align` mates are read from
/// them, and a mirrored part is never the subject of an `Align`.
///
/// `reflects` is the solver's local reflection normal per mirror image.
pub fn joint_tree(
    parts:      &[(String, ShapeExpr)],
    placements: &[Placement],
    frames:     &HashMap<String, Frame>,
    reflects:   &HashMap<String, Vec3>,
) -> HashMap<String, Joint> {
    let shape_of: HashMap<&str, &ShapeExpr> = parts.iter().map(|(n, s)| (n.as_str(), s)).collect();
    let placement_of: HashMap<&str, &Placement> =
        placements.iter().map(|p| (p.subject_name(), p)).collect();

    // Pass 1: every `Align`-placed or unplaced part.
    let mut out: HashMap<String, Joint> = HashMap::new();
    for (name, frame) in frames {
        let joint = match placement_of.get(name.as_str()) {
            Some(Placement::Align { subject, object, .. }) => {
                let parent = frames.contains_key(&object.part).then(|| object.part.clone());
                // The solver already resolved this anchor successfully; if it
                // could not be resolved here we fall back to the part's frame
                // (still world-correct, only the pivot moves).
                let frame = shape_of.get(subject.part.as_str())
                    .and_then(|s| resolve_anchor(s, &subject.anchor, &subject.args).ok())
                    .map(|a| frame.compose(&a.frame))
                    .unwrap_or(*frame);
                Joint { parent, frame }
            }
            Some(Placement::Mirror { .. }) => continue,
            None => Joint { parent: None, frame: *frame },
        };
        out.insert(name.clone(), joint);
    }

    // Pass 2: mirror images (the solver forbids mirroring an image, so every
    // source is already in `out`).
    let image_of: HashMap<&str, &str> = placements.iter().filter_map(|p| match p {
        Placement::Mirror { subject, source, .. } => Some((source.as_str(), subject.as_str())),
        _ => None,
    }).collect();
    for p in placements {
        let Placement::Mirror { subject, source, .. } = p else { continue };
        let Some(&f_img) = frames.get(subject) else { continue };
        let joint = match (out.get(source), frames.get(source)) {
            (Some(js), Some(f_src)) => {
                let parent = js.parent.as_ref().map(|q| {
                    image_of.get(q.as_str()).map(|s| s.to_string()).unwrap_or_else(|| q.clone())
                });
                let frame = match reflects.get(subject) {
                    Some(n) => {
                        let m = Frame::from_rot(reflection(*n));
                        let t = f_src.inverse().compose(&js.frame); // joint in source-local
                        f_img.compose(&m.compose(&t).compose(&m))
                    }
                    None => f_img,
                };
                Joint { parent, frame }
            }
            _ => Joint { parent: None, frame: f_img },
        };
        out.insert(subject.clone(), joint);
    }
    out
}

/// I − 2nnᵀ for the unit `n` (normalized here).
fn reflection(n: Vec3) -> Mat3 {
    let n = n.normalize().unwrap_or(Vec3::X);
    let v = [n.x, n.y, n.z];
    let mut m = [[0.0; 3]; 3];
    for (r, row) in m.iter_mut().enumerate() {
        for (c, x) in row.iter_mut().enumerate() {
            *x = if r == c { 1.0 } else { 0.0 } - 2.0 * v[r] * v[c];
        }
    }
    Mat3(m)
}
