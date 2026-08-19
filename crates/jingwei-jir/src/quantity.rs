//! Typed physical quantities and deterministic unit conversion.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Supported Phase 0 dimensions.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dimension {
    /// Unitless ratio or count.
    Dimensionless,
    /// Length.
    Length,
    /// Electric potential.
    Voltage,
    /// Electric current.
    Current,
    /// Temperature difference or absolute Celsius value in Phase 0.
    Temperature,
    /// Plane angle.
    Angle,
    /// Time.
    Time,
}

/// Units recognized by the core quantity type.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub enum Unit {
    #[serde(rename = "1")]
    One,
    #[serde(rename = "m")]
    Meter,
    #[serde(rename = "mm")]
    Millimeter,
    #[serde(rename = "um")]
    Micrometer,
    #[serde(rename = "V")]
    Volt,
    #[serde(rename = "mV")]
    Millivolt,
    #[serde(rename = "A")]
    Ampere,
    #[serde(rename = "mA")]
    Milliampere,
    #[serde(rename = "degC")]
    Celsius,
    #[serde(rename = "deg")]
    Degree,
    #[serde(rename = "rad")]
    Radian,
    #[serde(rename = "s")]
    Second,
    #[serde(rename = "ms")]
    Millisecond,
}

impl Unit {
    /// Returns the physical dimension associated with the unit.
    pub const fn dimension(self) -> Dimension {
        match self {
            Self::One => Dimension::Dimensionless,
            Self::Meter | Self::Millimeter | Self::Micrometer => Dimension::Length,
            Self::Volt | Self::Millivolt => Dimension::Voltage,
            Self::Ampere | Self::Milliampere => Dimension::Current,
            Self::Celsius => Dimension::Temperature,
            Self::Degree | Self::Radian => Dimension::Angle,
            Self::Second | Self::Millisecond => Dimension::Time,
        }
    }

    const fn scale_to_base(self) -> f64 {
        match self {
            Self::One
            | Self::Meter
            | Self::Volt
            | Self::Ampere
            | Self::Celsius
            | Self::Second
            | Self::Radian => 1.0,
            Self::Millimeter | Self::Millivolt | Self::Milliampere | Self::Millisecond => 0.001,
            Self::Micrometer => 0.000_001,
            Self::Degree => std::f64::consts::PI / 180.0,
        }
    }
}

/// A finite physical quantity with an explicit unit and dimension.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
pub struct Quantity {
    /// Numeric value in `unit`.
    pub value: f64,
    /// Explicit wire unit.
    pub unit: Unit,
    /// Redundant dimension used to reject malformed wire values.
    pub dimension: Dimension,
}

impl Quantity {
    /// Constructs a finite quantity.
    pub fn new(value: f64, unit: Unit) -> Result<Self, QuantityError> {
        if !value.is_finite() {
            return Err(QuantityError::NonFinite);
        }
        Ok(Self {
            value,
            unit,
            dimension: unit.dimension(),
        })
    }

    /// Converts the quantity to a compatible unit.
    pub fn convert_to(self, target: Unit) -> Result<Self, QuantityError> {
        if self.dimension != self.unit.dimension() || self.dimension != target.dimension() {
            return Err(QuantityError::DimensionMismatch {
                from: self.dimension,
                to: target.dimension(),
            });
        }
        let base = self.value * self.unit.scale_to_base();
        Self::new(base / target.scale_to_base(), target)
    }
}

/// Quantity construction or conversion error.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum QuantityError {
    /// NaN and infinity are not legal wire values.
    #[error("physical quantity must be finite")]
    NonFinite,
    /// Units from different dimensions cannot be converted.
    #[error("cannot convert {from:?} to {to:?}")]
    DimensionMismatch { from: Dimension, to: Dimension },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_millimeters_to_meters() {
        let converted = Quantity::new(2_500.0, Unit::Millimeter)
            .unwrap()
            .convert_to(Unit::Meter)
            .unwrap();
        assert_eq!(converted, Quantity::new(2.5, Unit::Meter).unwrap());
    }

    #[test]
    fn rejects_dimension_mismatch() {
        let error = Quantity::new(3.3, Unit::Volt)
            .unwrap()
            .convert_to(Unit::Millimeter)
            .unwrap_err();
        assert!(matches!(error, QuantityError::DimensionMismatch { .. }));
    }
}
