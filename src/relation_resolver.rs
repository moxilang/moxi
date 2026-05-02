// src/relation_resolver.rs
//
// Takes compiled parts (each stamped at origin) and the relation statements
// from the AST, then computes a (dx, dy, dz) world-space offset for each part.
//
// This runs AFTER geometry compilation and BEFORE OBJ export.
// The geometry backend stamps shapes at origin. We move them here.

use std::collections::HashMap;
use crate::ast::{RelationKind, RelationStmt};
use crate::geometry::CompiledPart;

// ── Public types ───────────────────────────────────────────────────────────

/// The resolved offset for a single part, in voxel units.
#[derive(Debug, Clone, Copy, Default)]
pub struct PartOffset {
    pub dx: i32,
    pub dy: i32,
    pub dz: i32,
}

/// Map from part name → its final world-space offset.
pub type OffsetMap = HashMap<String, PartOffset>;

// ── Entry point ────────────────────────────────────────────────────────────

/// Compute offsets for all parts in one entity given its relation statements.
///
/// `parts`     — compiled parts, each with a bounding box at origin
/// `relations` — the `RelationStmt` list from the resolved AST
///
/// Returns a map from part name → (dx, dy, dz) offset.
/// Parts not mentioned in any relation get offset (0,0,0).
pub fn resolve_offsets(
    parts:     &[CompiledPart],
    relations: &[RelationStmt],
) -> OffsetMap {
    // Build name → bounding box map for fast lookup
    let bbox_map: HashMap<&str, BBox> = parts
        .iter()
        .map(|p| (p.name.as_str(), BBox::from_part(p)))
        .collect();

    // Start every part at origin
    let mut offsets: OffsetMap = parts
        .iter()
        .map(|p| (p.name.clone(), PartOffset::default()))
        .collect();

    // Process each relation in order.
    // We do multiple passes to propagate chains:
    //   Skull above Spine, Spine above Pelvis
    // needs Spine's offset to be settled before Skull's is computed.
    for _ in 0..4 {
        for rel in relations {
            apply_relation(rel, &bbox_map, &mut offsets);
        }
    }

    offsets
}

// ── Relation application ───────────────────────────────────────────────────

fn apply_relation(
    rel:      &RelationStmt,
    bbox_map: &HashMap<&str, BBox>,
    offsets:  &mut OffsetMap,
) {
    let subj = rel.subject.name.as_str();
    let obj  = rel.object.name.as_str();

    let Some(sb) = bbox_map.get(subj) else { return };
    let Some(ob) = bbox_map.get(obj)  else { return };

    // Current offset of the object (the anchor part)
    let obj_off = offsets.get(obj).copied().unwrap_or_default();

    // Compute where the subject should go relative to the object
    let new_off = match rel.predicate {

        // ── Vertical ──────────────────────────────────────────────────────

        // Subject sits directly above object.
        // Subject's base (min_y + dy) = object's top (max_y + obj_off.dy)
        RelationKind::Above => PartOffset {
            dx: obj_off.dx,
            dy: obj_off.dy + ob.max_y + 1,
            dz: obj_off.dz,
        },

        // Subject sits directly below object.
        // Subject's top (max_y + dy) = object's base (min_y + obj_off.dy)
        RelationKind::Below => PartOffset {
            dx: obj_off.dx,
            dy: obj_off.dy + ob.min_y - sb.max_y - 1,
            dz: obj_off.dz,
        },

        // ── Containment ───────────────────────────────────────────────────

        // Subject's center aligns with object's center on all axes.
        RelationKind::Inside | RelationKind::Surrounds => PartOffset {
            dx: obj_off.dx + ob.center_x() - sb.center_x(),
            dy: obj_off.dy + ob.center_y() - sb.center_y(),
            dz: obj_off.dz + ob.center_z() - sb.center_z(),
        },

        // ── Lateral ───────────────────────────────────────────────────────

        RelationKind::LeftOf => PartOffset {
            dx: obj_off.dx - sb.width(),
            dy: obj_off.dy + ob.center_y() - sb.center_y(),
            dz: obj_off.dz + ob.center_z() - sb.center_z(),
        },

        RelationKind::RightOf => PartOffset {
            dx: obj_off.dx + ob.width(),
            dy: obj_off.dy + ob.center_y() - sb.center_y(),
            dz: obj_off.dz + ob.center_z() - sb.center_z(),
        },

        RelationKind::InFrontOf => PartOffset {
            dx: obj_off.dx + ob.center_x() - sb.center_x(),
            dy: obj_off.dy + ob.center_y() - sb.center_y(),
            dz: obj_off.dz - sb.depth(),
        },

        RelationKind::Behind => PartOffset {
            dx: obj_off.dx + ob.center_x() - sb.center_x(),
            dy: obj_off.dy + ob.center_y() - sb.center_y(),
            dz: obj_off.dz + ob.depth(),
        },

        // ── Contact ───────────────────────────────────────────────────────

        // Subject touches object — same as adjacent, centered
        RelationKind::Touch | RelationKind::AdjacentTo => PartOffset {
            dx: obj_off.dx + ob.center_x() - sb.center_x(),
            dy: obj_off.dy + ob.height(),
            dz: obj_off.dz + ob.center_z() - sb.center_z(),
        },

        // ── Symmetric ─────────────────────────────────────────────────────

        // Mirror the subject across the midline of the object.
        // For now: mirror on X axis (bilateral symmetry).
        RelationKind::SymmetricAcross => {
            let current = offsets.get(subj).copied().unwrap_or_default();
            PartOffset {
                dx: obj_off.dx + ob.center_x() - (current.dx - ob.center_x()),
                dy: current.dy,
                dz: current.dz,
            }
        }

        // ── Outside ───────────────────────────────────────────────────────

        // Place subject just outside the object's bounding box on X+
        RelationKind::Outside => PartOffset {
            dx: obj_off.dx + ob.width(),
            dy: obj_off.dy + ob.center_y() - sb.center_y(),
            dz: obj_off.dz + ob.center_z() - sb.center_z(),
        },

        RelationKind::AttachedTo => {
            // Same as above — geometric attachment handled by geometry pass later
            PartOffset {
                dx: obj_off.dx + ob.center_x() - sb.center_x(),
                dy: obj_off.dy + ob.height(),
                dz: obj_off.dz + ob.center_z() - sb.center_z(),
            }
        }
    };

    offsets.insert(subj.to_string(), new_off);
}

// ── Bounding box ───────────────────────────────────────────────────────────

/// Axis-aligned bounding box of a compiled part, in voxel units.
#[derive(Debug, Clone, Copy)]
pub struct BBox {
    pub min_x: i32, pub max_x: i32,
    pub min_y: i32, pub max_y: i32,
    pub min_z: i32, pub max_z: i32,
}

impl BBox {
    pub fn from_part(part: &CompiledPart) -> Self {
        let mut min_x = i32::MAX; let mut max_x = i32::MIN;
        let mut min_y = i32::MAX; let mut max_y = i32::MIN;
        let mut min_z = i32::MAX; let mut max_z = i32::MIN;

        for (x, y, z, _) in part.grid.iter_filled() {
            let (x, y, z) = (x as i32, y as i32, z as i32);
            if x < min_x { min_x = x; } if x > max_x { max_x = x; }
            if y < min_y { min_y = y; } if y > max_y { max_y = y; }
            if z < min_z { min_z = z; } if z > max_z { max_z = z; }
        }

        // Empty grid fallback
        if min_x == i32::MAX {
            return Self { min_x:0, max_x:0, min_y:0, max_y:0, min_z:0, max_z:0 };
        }

        // Normalize to shape-local space: subtract the grid center so that
        // the shape center sits at (0,0,0). This makes all relation math
        // relative to shape centers, not grid cell origins.
        let cx = (min_x + max_x) / 2;
        let cy = (min_y + max_y) / 2;
        let cz = (min_z + max_z) / 2;

        Self {
            min_x: min_x - cx, max_x: max_x - cx,
            min_y: min_y - cy, max_y: max_y - cy,
            min_z: min_z - cz, max_z: max_z - cz,
        }
    }

    pub fn width(&self)  -> i32 { self.max_x - self.min_x + 1 }
    pub fn height(&self) -> i32 { self.max_y - self.min_y + 1 }
    pub fn depth(&self)  -> i32 { self.max_z - self.min_z + 1 }

    pub fn center_x(&self) -> i32 { (self.min_x + self.max_x) / 2 }
    pub fn center_y(&self) -> i32 { (self.min_y + self.max_y) / 2 }
    pub fn center_z(&self) -> i32 { (self.min_z + self.max_z) / 2 }
}