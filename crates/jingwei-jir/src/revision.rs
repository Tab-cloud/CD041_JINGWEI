//! Immutable revision snapshot metadata.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{JIR_SCHEMA_VERSION, JirGraph, ProjectId, RevisionId};

/// Lifecycle status of an immutable design revision.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RevisionStatus {
    Draft,
    Candidate,
    Verified,
    Released,
    Superseded,
}

/// Metadata kept next to an immutable graph snapshot.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionMetadata {
    pub id: RevisionId,
    pub project_id: ProjectId,
    pub parents: Vec<RevisionId>,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub schema_version: String,
    pub operation_journal_hash: String,
    pub graph_hash: String,
    pub status: RevisionStatus,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

/// Complete immutable revision payload.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionSnapshot {
    pub metadata: RevisionMetadata,
    pub graph: JirGraph,
}

impl RevisionSnapshot {
    /// Constructs metadata and computes deterministic graph/revision IDs.
    pub fn build(
        project_id: ProjectId,
        parents: Vec<RevisionId>,
        graph: JirGraph,
        created_by: impl Into<String>,
        operation_journal_hash: impl Into<String>,
        status: RevisionStatus,
    ) -> Result<Self, crate::CanonicalError> {
        let graph_hash = graph.content_hash()?;
        let revision_seed = serde_json::json!({
            "projectId": project_id,
            "parents": parents,
            "schemaVersion": JIR_SCHEMA_VERSION,
            "graphHash": graph_hash,
            "operationJournalHash": operation_journal_hash.into(),
        });
        let revision_hash = crate::canonical_sha256(&revision_seed)?;
        let operation_journal_hash = revision_seed["operationJournalHash"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        Ok(Self {
            metadata: RevisionMetadata {
                id: RevisionId::new(format!("rev-{}", &revision_hash[..24]))
                    .expect("generated revision ID is valid"),
                project_id,
                parents,
                created_at: Utc::now(),
                created_by: created_by.into(),
                schema_version: JIR_SCHEMA_VERSION.into(),
                operation_journal_hash,
                graph_hash,
                status,
                artifact_refs: Vec::new(),
                evidence_refs: Vec::new(),
            },
            graph,
        })
    }

    /// Revalidates graph integrity and the stored graph hash.
    pub fn verify(&self) -> Result<(), RevisionVerificationError> {
        self.graph.validate()?;
        let actual = self.graph.content_hash()?;
        if actual != self.metadata.graph_hash {
            return Err(RevisionVerificationError::HashMismatch {
                expected: self.metadata.graph_hash.clone(),
                actual,
            });
        }
        Ok(())
    }
}

/// Revision verification error.
#[derive(Debug, thiserror::Error)]
pub enum RevisionVerificationError {
    #[error(transparent)]
    Graph(#[from] crate::GraphError),
    #[error(transparent)]
    Canonical(#[from] crate::CanonicalError),
    #[error("revision graph hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
}
