//! Deterministic JSON normalization and hashing.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// JSON value validated for deterministic canonical serialization.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(transparent)]
pub struct CanonicalValue(Value);

impl CanonicalValue {
    /// Validates and wraps a JSON value.
    pub fn new(value: Value) -> Result<Self, CanonicalError> {
        validate_depth_and_numbers(&value, 0)?;
        Ok(Self(value))
    }

    /// Returns the contained JSON value.
    pub fn as_value(&self) -> &Value {
        &self.0
    }

    /// Consumes the wrapper.
    pub fn into_value(self) -> Value {
        self.0
    }

    /// Serializes the value with object keys sorted recursively.
    pub fn to_canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        canonical_json_bytes(&self.0)
    }

    /// Returns the lowercase SHA-256 digest of canonical bytes.
    pub fn sha256(&self) -> Result<String, CanonicalError> {
        canonical_sha256(&self.0)
    }
}

impl TryFrom<Value> for CanonicalValue {
    type Error = CanonicalError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Canonical serialization failure.
#[derive(Debug, thiserror::Error)]
pub enum CanonicalError {
    /// Input exceeded the defensive nesting limit.
    #[error("canonical JSON exceeds maximum nesting depth of 64")]
    TooDeep,
    /// JSON serialization unexpectedly failed.
    #[error("canonical JSON serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Serializes arbitrary serializable data into canonical JSON bytes.
pub fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, CanonicalError> {
    let value = serde_json::to_value(value)?;
    validate_depth_and_numbers(&value, 0)?;
    serde_json::to_vec(&sort_value(value)).map_err(Into::into)
}

/// Hashes canonical JSON using SHA-256.
pub fn canonical_sha256<T: Serialize>(value: &T) -> Result<String, CanonicalError> {
    let bytes = canonical_json_bytes(value)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let sorted: BTreeMap<_, _> = object
                .into_iter()
                .map(|(key, value)| (key, sort_value(value)))
                .collect();
            Value::Object(sorted.into_iter().collect())
        }
        Value::Array(values) => Value::Array(values.into_iter().map(sort_value).collect()),
        scalar => scalar,
    }
}

fn validate_depth_and_numbers(value: &Value, depth: usize) -> Result<(), CanonicalError> {
    if depth > 64 {
        return Err(CanonicalError::TooDeep);
    }
    match value {
        Value::Array(values) => {
            for value in values {
                validate_depth_and_numbers(value, depth + 1)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                validate_depth_and_numbers(value, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn object_order_does_not_change_hash() {
        let left = json!({"b": 2, "a": {"z": 1, "x": 0}});
        let right = json!({"a": {"x": 0, "z": 1}, "b": 2});
        assert_eq!(
            canonical_sha256(&left).unwrap(),
            canonical_sha256(&right).unwrap()
        );
        assert_eq!(
            canonical_json_bytes(&left).unwrap(),
            br#"{"a":{"x":0,"z":1},"b":2}"#
        );
    }
}
