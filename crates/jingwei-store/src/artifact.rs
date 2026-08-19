//! Content-addressed artifact storage.

use chrono::{DateTime, Utc};
use jingwei_jir::ArtifactId;
use serde::{Deserialize, Serialize};

/// Maximum single artifact accepted by the Phase 0 local API.
pub const MAX_ARTIFACT_BYTES: usize = 64 * 1024 * 1024;

/// Immutable artifact metadata.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRecord {
    pub id: ArtifactId,
    pub sha256: String,
    pub byte_length: u64,
    pub media_type: String,
    pub created_at: DateTime<Utc>,
}
