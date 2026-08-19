use crate::{Length, Point};
use serde::{Deserialize, Serialize};

/// Axis-aligned board-space bounds.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BoundingBox {
    /// Minimum point.
    pub min: Point,
    /// Maximum point.
    pub max: Point,
}

impl BoundingBox {
    /// Builds bounds and normalizes axis order.
    pub fn new(first: Point, second: Point) -> Self {
        Self {
            min: Point::from_micrometres(
                first.x.as_micrometres().min(second.x.as_micrometres()),
                first.y.as_micrometres().min(second.y.as_micrometres()),
            ),
            max: Point::from_micrometres(
                first.x.as_micrometres().max(second.x.as_micrometres()),
                first.y.as_micrometres().max(second.y.as_micrometres()),
            ),
        }
    }

    /// Returns whether the point is inside or on the bounds.
    pub fn contains(self, point: Point) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    /// Returns whether two bounds overlap or touch.
    pub fn intersects(self, other: Self) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
    }

    /// Returns bounds expanded by a typed margin.
    #[must_use]
    pub fn expanded(self, margin: Length) -> Self {
        Self {
            min: Point {
                x: self.min.x - margin,
                y: self.min.y - margin,
            },
            max: Point {
                x: self.max.x + margin,
                y: self.max.y + margin,
            },
        }
    }

    /// Squared distance between two disjoint bounds, or zero when touching.
    pub fn squared_distance(self, other: Self) -> i128 {
        let dx = if self.max.x < other.min.x {
            other.min.x.as_micrometres() - self.max.x.as_micrometres()
        } else if other.max.x < self.min.x {
            self.min.x.as_micrometres() - other.max.x.as_micrometres()
        } else {
            0
        };
        let dy = if self.max.y < other.min.y {
            other.min.y.as_micrometres() - self.max.y.as_micrometres()
        } else if other.max.y < self.min.y {
            self.min.y.as_micrometres() - other.max.y.as_micrometres()
        } else {
            0
        };
        i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy)
    }
}

/// One stable-ID entry in a spatial index.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpatialEntry {
    /// JIR entity ID.
    pub entity_id: String,
    /// Entity bounds.
    pub bounds: BoundingBox,
}

/// Deterministic linear spatial index suitable for Phase 0 and viewport culling.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct SpatialIndex {
    entries: Vec<SpatialEntry>,
}

impl SpatialIndex {
    /// Builds an index sorted by entity ID so query order is reproducible.
    pub fn new(mut entries: Vec<SpatialEntry>) -> Self {
        entries.sort_by(|left, right| left.entity_id.cmp(&right.entity_id));
        Self { entries }
    }

    /// Queries entries intersecting a viewport.
    pub fn query(&self, viewport: BoundingBox) -> Vec<&SpatialEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.bounds.intersects(viewport))
            .collect()
    }

    /// Returns the closest entries containing the point, in stable ID order.
    pub fn hit_test(&self, point: Point) -> Vec<&SpatialEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.bounds.contains(point))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_query_order_is_stable() {
        let bounds = BoundingBox::new(Point::from_millimetres(0, 0), Point::from_millimetres(1, 1));
        let index = SpatialIndex::new(vec![
            SpatialEntry {
                entity_id: "z".to_owned(),
                bounds,
            },
            SpatialEntry {
                entity_id: "a".to_owned(),
                bounds,
            },
        ]);
        let ids: Vec<_> = index
            .query(bounds)
            .into_iter()
            .map(|entry| entry.entity_id.as_str())
            .collect();
        assert_eq!(ids, ["a", "z"]);
    }
}
