//! Deterministic geometry for the physical-design domain.
//!
//! Coordinates use integer micrometres.  The representation is deliberately
//! independent from renderer pixels and avoids unitless floating-point values.

mod primitives;
mod spatial;
mod units;

pub use primitives::{
    Arc, GeometryError, LineSegment, Polygon, PolygonValidity, Polyline, Transform,
};
pub use spatial::{BoundingBox, SpatialEntry, SpatialIndex};
pub use units::{Angle, Length, Point, QuantizationPolicy, Vector};

/// Version of the coordinate and tolerance convention used by this crate.
pub const GEOMETRY_CONVENTION_VERSION: &str = "jingwei-geometry-micrometre-v1";
