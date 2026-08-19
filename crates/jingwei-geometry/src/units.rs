use serde::{Deserialize, Serialize};

/// A signed length stored as integer micrometres.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(transparent)]
pub struct Length(i64);

impl Length {
    /// Zero length.
    pub const ZERO: Self = Self(0);

    /// Creates a length from micrometres.
    pub const fn from_micrometres(value: i64) -> Self {
        Self(value)
    }

    /// Creates a length from whole millimetres.
    pub const fn from_millimetres(value: i64) -> Self {
        Self(value * 1_000)
    }

    /// Returns the integer micrometre representation.
    pub const fn as_micrometres(self) -> i64 {
        self.0
    }

    /// Returns the absolute value.
    #[must_use]
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }
}

impl std::ops::Add for Length {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl std::ops::Sub for Length {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0.saturating_sub(rhs.0))
    }
}

/// A board-space point.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Point {
    /// X coordinate.
    pub x: Length,
    /// Y coordinate.
    pub y: Length,
}

impl Point {
    /// Creates a point from micrometre coordinates.
    pub const fn from_micrometres(x: i64, y: i64) -> Self {
        Self {
            x: Length::from_micrometres(x),
            y: Length::from_micrometres(y),
        }
    }

    /// Creates a point from whole-millimetre coordinates.
    pub const fn from_millimetres(x: i64, y: i64) -> Self {
        Self {
            x: Length::from_millimetres(x),
            y: Length::from_millimetres(y),
        }
    }

    /// Squared Euclidean distance in square micrometres.
    pub fn squared_distance(self, other: Self) -> i128 {
        let dx = i128::from(self.x.0) - i128::from(other.x.0);
        let dy = i128::from(self.y.0) - i128::from(other.y.0);
        dx * dx + dy * dy
    }
}

impl std::ops::Add<Vector> for Point {
    type Output = Self;

    fn add(self, rhs: Vector) -> Self::Output {
        Self {
            x: self.x + rhs.dx,
            y: self.y + rhs.dy,
        }
    }
}

impl std::ops::Sub for Point {
    type Output = Vector;

    fn sub(self, rhs: Self) -> Self::Output {
        Vector {
            dx: self.x - rhs.x,
            dy: self.y - rhs.y,
        }
    }
}

/// A board-space displacement.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Vector {
    /// X displacement.
    pub dx: Length,
    /// Y displacement.
    pub dy: Length,
}

/// An angle stored as integer microdegrees.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(transparent)]
pub struct Angle(i64);

impl Angle {
    /// Zero degrees.
    pub const ZERO: Self = Self(0);

    /// Creates an angle from whole degrees.
    pub const fn from_degrees(value: i64) -> Self {
        Self(value * 1_000_000)
    }

    /// Returns microdegrees.
    pub const fn as_microdegrees(self) -> i64 {
        self.0
    }

    /// Returns a normalized angle in the half-open interval [0, 360 degrees).
    #[must_use]
    pub fn normalized(self) -> Self {
        Self(self.0.rem_euclid(360_000_000))
    }
}

/// Versioned snapping and comparison tolerances.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct QuantizationPolicy {
    /// Stable policy identifier recorded in artifacts.
    pub version: String,
    /// Coordinate snap grid.
    pub coordinate_step: Length,
    /// Tolerance used by deterministic geometry comparisons.
    pub comparison_tolerance: Length,
}

impl Default for QuantizationPolicy {
    fn default() -> Self {
        Self {
            version: "quantization-100um-v1".to_owned(),
            coordinate_step: Length::from_micrometres(100),
            comparison_tolerance: Length::from_micrometres(1),
        }
    }
}

impl QuantizationPolicy {
    /// Snaps a point to the nearest grid intersection using integer arithmetic.
    pub fn snap(&self, point: Point) -> Point {
        fn snap_axis(value: i64, step: i64) -> i64 {
            if step <= 0 {
                return value;
            }
            let quotient = value.div_euclid(step);
            let remainder = value.rem_euclid(step);
            if remainder * 2 >= step {
                (quotient + 1) * step
            } else {
                quotient * step
            }
        }

        let step = self.coordinate_step.as_micrometres();
        Point::from_micrometres(
            snap_axis(point.x.as_micrometres(), step),
            snap_axis(point.y.as_micrometres(), step),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_is_deterministic_for_negative_coordinates() {
        let policy = QuantizationPolicy::default();
        assert_eq!(
            policy.snap(Point::from_micrometres(-149, 151)),
            Point::from_micrometres(-100, 200)
        );
    }

    #[test]
    fn angle_normalization_is_stable() {
        assert_eq!(
            Angle::from_degrees(-90).normalized(),
            Angle::from_degrees(270)
        );
    }
}
