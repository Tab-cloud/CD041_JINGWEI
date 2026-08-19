use crate::{Angle, BoundingBox, Point, Vector};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Geometry validation error.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GeometryError {
    /// A polyline has fewer than two points.
    #[error("a polyline requires at least two points")]
    TooFewPolylinePoints,
    /// A polygon has fewer than three distinct vertices.
    #[error("a polygon requires at least three distinct vertices")]
    TooFewPolygonVertices,
}

/// A finite line segment.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LineSegment {
    /// Start point.
    pub start: Point,
    /// End point.
    pub end: Point,
}

impl LineSegment {
    /// Returns segment bounds.
    pub fn bounds(self) -> BoundingBox {
        BoundingBox::new(self.start, self.end)
    }

    /// Returns whether this segment intersects another, including endpoints.
    pub fn intersects(self, other: Self) -> bool {
        fn orientation(a: Point, b: Point, c: Point) -> i128 {
            let edge_x = i128::from(b.x.as_micrometres() - a.x.as_micrometres());
            let edge_y = i128::from(b.y.as_micrometres() - a.y.as_micrometres());
            let point_x = i128::from(c.x.as_micrometres() - a.x.as_micrometres());
            let point_y = i128::from(c.y.as_micrometres() - a.y.as_micrometres());
            edge_x * point_y - edge_y * point_x
        }

        fn on_segment(a: Point, b: Point, point: Point) -> bool {
            BoundingBox::new(a, b).contains(point)
        }

        let o1 = orientation(self.start, self.end, other.start);
        let o2 = orientation(self.start, self.end, other.end);
        let o3 = orientation(other.start, other.end, self.start);
        let o4 = orientation(other.start, other.end, self.end);
        if ((o1 > 0 && o2 < 0) || (o1 < 0 && o2 > 0)) && ((o3 > 0 && o4 < 0) || (o3 < 0 && o4 > 0))
        {
            return true;
        }
        (o1 == 0 && on_segment(self.start, self.end, other.start))
            || (o2 == 0 && on_segment(self.start, self.end, other.end))
            || (o3 == 0 && on_segment(other.start, other.end, self.start))
            || (o4 == 0 && on_segment(other.start, other.end, self.end))
    }
}

/// A connected sequence of points.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Polyline {
    /// Ordered vertices.
    pub points: Vec<Point>,
}

impl Polyline {
    /// Creates a validated polyline.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::TooFewPolylinePoints`] when fewer than two
    /// points are supplied.
    pub fn new(points: Vec<Point>) -> Result<Self, GeometryError> {
        if points.len() < 2 {
            return Err(GeometryError::TooFewPolylinePoints);
        }
        Ok(Self { points })
    }

    /// Returns component line segments.
    pub fn segments(&self) -> impl Iterator<Item = LineSegment> + '_ {
        self.points.windows(2).map(|pair| LineSegment {
            start: pair[0],
            end: pair[1],
        })
    }

    /// Returns bounds for the complete polyline.
    pub fn bounds(&self) -> BoundingBox {
        bounds_for_points(&self.points)
    }

    /// Returns Manhattan length in micrometres.
    pub fn manhattan_length_micrometres(&self) -> i64 {
        self.segments()
            .map(|segment| {
                (segment.end.x - segment.start.x).abs().as_micrometres()
                    + (segment.end.y - segment.start.y).abs().as_micrometres()
            })
            .sum()
    }
}

/// A closed polygon represented without a duplicate terminal vertex.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Polygon {
    /// Ordered vertices.
    pub vertices: Vec<Point>,
}

/// Basic deterministic polygon validation result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PolygonValidity {
    /// Whether the polygon can be used as a board boundary.
    pub valid: bool,
    /// Machine-readable issue codes.
    pub issues: Vec<String>,
}

impl Polygon {
    /// Creates a polygon with at least three distinct vertices.
    ///
    /// # Errors
    ///
    /// Returns [`GeometryError::TooFewPolygonVertices`] when fewer than three
    /// distinct points are supplied.
    pub fn new(vertices: Vec<Point>) -> Result<Self, GeometryError> {
        let mut distinct = vertices.clone();
        distinct.sort_by_key(|point| (point.x, point.y));
        distinct.dedup();
        if distinct.len() < 3 {
            return Err(GeometryError::TooFewPolygonVertices);
        }
        Ok(Self { vertices })
    }

    /// Creates an axis-aligned rectangle.
    pub fn rectangle(min: Point, max: Point) -> Self {
        Self {
            vertices: vec![
                min,
                Point { x: max.x, y: min.y },
                max,
                Point { x: min.x, y: max.y },
            ],
        }
    }

    /// Returns polygon edges including the closing edge.
    pub fn edges(&self) -> impl Iterator<Item = LineSegment> + '_ {
        self.vertices
            .iter()
            .copied()
            .zip(self.vertices.iter().copied().cycle().skip(1))
            .take(self.vertices.len())
            .map(|(start, end)| LineSegment { start, end })
    }

    /// Returns polygon bounds.
    pub fn bounds(&self) -> BoundingBox {
        bounds_for_points(&self.vertices)
    }

    /// Performs a ray-crossing containment test with integer arithmetic.
    pub fn contains(&self, point: Point) -> bool {
        if self.edges().any(|edge| {
            let area = (i128::from(edge.end.x.as_micrometres() - edge.start.x.as_micrometres())
                * i128::from(point.y.as_micrometres() - edge.start.y.as_micrometres()))
                - (i128::from(edge.end.y.as_micrometres() - edge.start.y.as_micrometres())
                    * i128::from(point.x.as_micrometres() - edge.start.x.as_micrometres()));
            area == 0 && edge.bounds().contains(point)
        }) {
            return true;
        }

        let px = point.x.as_micrometres();
        let py = point.y.as_micrometres();
        let mut inside = false;
        for edge in self.edges() {
            let x1 = edge.start.x.as_micrometres();
            let y1 = edge.start.y.as_micrometres();
            let x2 = edge.end.x.as_micrometres();
            let y2 = edge.end.y.as_micrometres();
            if (y1 > py) != (y2 > py) {
                let lhs = i128::from(px - x1) * i128::from(y2 - y1);
                let rhs = i128::from(x2 - x1) * i128::from(py - y1);
                if (y2 > y1 && lhs < rhs) || (y2 < y1 && lhs > rhs) {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Checks repeated vertices, zero-length edges and self-intersections.
    pub fn validity(&self) -> PolygonValidity {
        let mut issues = Vec::new();
        if self.vertices.len() < 3 {
            issues.push("too-few-vertices".to_owned());
        }
        if self.edges().any(|edge| edge.start == edge.end) {
            issues.push("zero-length-edge".to_owned());
        }
        let edges: Vec<_> = self.edges().collect();
        for left in 0..edges.len() {
            for right in (left + 1)..edges.len() {
                let adjacent = right == left + 1 || (left == 0 && right + 1 == edges.len());
                if !adjacent && edges[left].intersects(edges[right]) {
                    issues.push("self-intersection".to_owned());
                    break;
                }
            }
        }
        PolygonValidity {
            valid: issues.is_empty(),
            issues,
        }
    }
}

/// Circular arc with typed center, radius and angles.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Arc {
    /// Circle center.
    pub center: Point,
    /// Radius.
    pub radius: crate::Length,
    /// Start angle.
    pub start: Angle,
    /// Signed sweep.
    pub sweep: Angle,
}

/// Translation, rotation and optional mirror used for footprint geometry.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Transform {
    /// Translation.
    pub translation: Vector,
    /// Rotation.
    pub rotation: Angle,
    /// Whether the local X axis is mirrored.
    pub mirror_x: bool,
}

impl Transform {
    /// Applies exact transforms for orthogonal rotations used by Phase 0 PCB fixtures.
    pub fn apply(self, point: Point) -> Point {
        let mut x = point.x.as_micrometres();
        let y = point.y.as_micrometres();
        if self.mirror_x {
            x = -x;
        }
        let (x, y) = match self.rotation.normalized().as_microdegrees() {
            90_000_000 => (-y, x),
            180_000_000 => (-x, -y),
            270_000_000 => (y, -x),
            _ => (x, y),
        };
        Point::from_micrometres(
            x + self.translation.dx.as_micrometres(),
            y + self.translation.dy.as_micrometres(),
        )
    }
}

fn bounds_for_points(points: &[Point]) -> BoundingBox {
    let first = points.first().copied().unwrap_or_default();
    points
        .iter()
        .copied()
        .fold(BoundingBox::new(first, first), |bounds, point| {
            BoundingBox::new(
                Point::from_micrometres(
                    bounds.min.x.as_micrometres().min(point.x.as_micrometres()),
                    bounds.min.y.as_micrometres().min(point.y.as_micrometres()),
                ),
                Point::from_micrometres(
                    bounds.max.x.as_micrometres().max(point.x.as_micrometres()),
                    bounds.max.y.as_micrometres().max(point.y.as_micrometres()),
                ),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_contains_edge_and_interior() {
        let rectangle = Polygon::rectangle(
            Point::from_millimetres(0, 0),
            Point::from_millimetres(60, 40),
        );
        assert!(rectangle.contains(Point::from_millimetres(30, 20)));
        assert!(rectangle.contains(Point::from_millimetres(0, 20)));
        assert!(!rectangle.contains(Point::from_millimetres(61, 20)));
        assert!(rectangle.validity().valid);
    }

    #[test]
    fn bow_tie_polygon_is_rejected() {
        let polygon = Polygon::new(vec![
            Point::from_millimetres(0, 0),
            Point::from_millimetres(10, 10),
            Point::from_millimetres(0, 10),
            Point::from_millimetres(10, 0),
        ])
        .expect("four distinct points");
        assert_eq!(polygon.validity().issues, ["self-intersection"]);
    }
}
