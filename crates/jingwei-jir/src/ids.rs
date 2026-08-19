//! Opaque wire identifiers. Consumers must not depend on their internal format.

use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! opaque_id {
    ($name:ident, $prefix:literal) => {
        #[doc = concat!("Opaque `", stringify!($name), "` wire identifier.")]
        #[derive(
            Clone,
            Debug,
            Default,
            Deserialize,
            Eq,
            Hash,
            JsonSchema,
            Ord,
            PartialEq,
            PartialOrd,
            Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Constructs an identifier after checking that it is non-empty and bounded.
            pub fn new(value: impl Into<String>) -> Result<Self, InvalidId> {
                let value = value.into();
                if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
                    return Err(InvalidId(stringify!($name)));
                }
                Ok(Self(value))
            }

            /// Generates a locally unique identifier. Its concrete format is not a wire contract.
            pub fn generate() -> Self {
                Self(format!("{}-{}", $prefix, Uuid::new_v4()))
            }

            /// Returns the opaque string value.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = InvalidId;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

/// Error returned for an empty, overlong, or control-character-containing identifier.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("invalid {0}: IDs must contain 1..=255 non-control UTF-8 bytes")]
pub struct InvalidId(&'static str);

opaque_id!(WorkspaceId, "ws");
opaque_id!(ProjectId, "project");
opaque_id!(RevisionId, "rev");
opaque_id!(EntityId, "entity");
opaque_id!(EdgeId, "edge");
opaque_id!(RequestId, "request");
opaque_id!(IdempotencyKey, "idem");
opaque_id!(TaskId, "task");
opaque_id!(ExecutionId, "execution");
opaque_id!(SessionId, "session");
opaque_id!(ArtifactId, "artifact");
opaque_id!(EvidenceId, "evidence");
