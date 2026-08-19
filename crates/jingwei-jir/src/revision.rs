//! Canonical immutable revision snapshot identity.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{JIR_SCHEMA_VERSION, JirGraph, ProjectId, RevisionId};

/// Canonical metadata included in the revision content identity.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionMetadata {
    pub id: RevisionId,
    pub project_id: ProjectId,
    pub parents: Vec<RevisionId>,
    pub schema_version: String,
    pub operation_journal_hash: String,
    pub graph_hash: String,
}

/// Complete immutable revision payload. Authorship and lifecycle live in separate records.
#[derive(Clone, Debug, Deserialize, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionSnapshot {
    pub metadata: RevisionMetadata,
    pub graph: JirGraph,
}

impl RevisionSnapshot {
    /// Constructs a canonical snapshot and derives its opaque content identity.
    pub fn build(
        project_id: ProjectId,
        parents: Vec<RevisionId>,
        graph: JirGraph,
        operation_journal_hash: impl Into<String>,
    ) -> Result<Self, crate::CanonicalError> {
        let graph_hash = graph.content_hash()?;
        let mut snapshot = Self {
            metadata: RevisionMetadata {
                id: RevisionId::new("revision-pending")
                    .expect("static pending revision ID is valid"),
                project_id,
                parents,
                schema_version: JIR_SCHEMA_VERSION.into(),
                operation_journal_hash: operation_journal_hash.into(),
                graph_hash,
            },
            graph,
        };
        snapshot.metadata.id = snapshot.expected_id()?;
        Ok(snapshot)
    }

    /// Re-identifies a not-yet-persisted snapshot after an explicit schema migration.
    pub fn reidentify(&mut self) -> Result<(), crate::CanonicalError> {
        self.metadata.graph_hash = self.graph.content_hash()?;
        self.metadata.id = self.expected_id()?;
        Ok(())
    }

    /// Revalidates graph integrity, graph hash, journal digest shape, and full revision identity.
    pub fn verify(&self) -> Result<(), RevisionVerificationError> {
        self.graph.validate()?;
        let graph_hash = self.graph.content_hash()?;
        if graph_hash != self.metadata.graph_hash {
            return Err(RevisionVerificationError::GraphHashMismatch {
                expected: self.metadata.graph_hash.clone(),
                actual: graph_hash,
            });
        }
        if !is_sha256(&self.metadata.operation_journal_hash) {
            return Err(RevisionVerificationError::InvalidJournalHash);
        }
        let expected = self.expected_id()?;
        if expected != self.metadata.id {
            return Err(RevisionVerificationError::IdentityMismatch {
                expected,
                actual: self.metadata.id.clone(),
            });
        }
        Ok(())
    }

    fn expected_id(&self) -> Result<RevisionId, crate::CanonicalError> {
        let seed = serde_json::json!({
            "projectId": self.metadata.project_id,
            "parents": self.metadata.parents,
            "schemaVersion": self.metadata.schema_version,
            "graphHash": self.metadata.graph_hash,
            "operationJournalHash": self.metadata.operation_journal_hash,
        });
        let digest = crate::canonical_sha256(&seed)?;
        let identity = if self.metadata.schema_version == "0.1" {
            &digest[..24]
        } else {
            &digest
        };
        Ok(RevisionId::new(format!("rev-{identity}")).expect("generated revision ID is valid"))
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Revision graph, journal, or identity verification error.
#[derive(Debug, thiserror::Error)]
pub enum RevisionVerificationError {
    #[error(transparent)]
    Graph(#[from] crate::GraphError),
    #[error(transparent)]
    Canonical(#[from] crate::CanonicalError),
    #[error("revision graph hash mismatch: expected {expected}, got {actual}")]
    GraphHashMismatch { expected: String, actual: String },
    #[error("revision operation journal hash is not a SHA-256 digest")]
    InvalidJournalHash,
    #[error("revision identity mismatch: expected {expected}, got {actual}")]
    IdentityMismatch {
        expected: RevisionId,
        actual: RevisionId,
    },
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn snapshot() -> RevisionSnapshot {
        RevisionSnapshot::build(
            ProjectId::new("project-revision-test").unwrap(),
            Vec::new(),
            JirGraph::default(),
            crate::canonical_sha256(&json!({"kind": "test"})).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn detects_metadata_substitution_and_bad_journal_hash() {
        let original = snapshot();
        original.verify().unwrap();

        let mut substituted = original.clone();
        substituted.metadata.project_id = ProjectId::new("other-project").unwrap();
        assert!(matches!(
            substituted.verify(),
            Err(RevisionVerificationError::IdentityMismatch { .. })
        ));

        let mut bad_journal = original;
        bad_journal.metadata.operation_journal_hash = "not-a-hash".into();
        assert!(matches!(
            bad_journal.verify(),
            Err(RevisionVerificationError::InvalidJournalHash)
        ));
    }
}
