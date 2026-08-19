//! Canonical types, graph invariants, quantities, revisions, and migrations for JIR.

mod canonical;
mod graph;
mod ids;
mod lifecycle;
mod migration;
mod quantity;
mod revision;

pub use canonical::{CanonicalError, CanonicalValue, canonical_json_bytes, canonical_sha256};
pub use graph::{
    EditControl, EditPolicy, EntityKind, GraphDiff, GraphError, JirEdge, JirEntity, JirGraph,
    Provenance, SchemaDescriptor, SchemaRegistry,
};
pub use ids::{
    ArtifactId, AssessmentId, AttestationId, CommitId, DecisionId, EdgeId, EntityId, EvidenceId,
    ExecutionId, GateResultId, IdempotencyKey, InvalidId, InvocationId, ProjectId, ProposalId,
    ReleaseId, ReleaseRequestId, RequestId, RevisionId, RevokeId, SessionId, TaskId, WorkspaceId,
};
pub use lifecycle::{
    ApprovalAttestation, AuthenticatedActorRef, ChangeProposal, DependencyLocks,
    EffectiveReleaseStatus, GateResult, GateStatus, InvocationCorrelation, InvocationSourceKind,
    MaturityAssessment, MaturityLevel, ProposalAction, ProposalDecisionRecord, ReleaseRecord,
    ReleaseRequest, RevisionCommitRecord, RevokeRecord,
};
pub use migration::{MigrationError, MigrationRegistry, SchemaMigration};
pub use quantity::{Dimension, Quantity, QuantityError, Unit};
pub use revision::{RevisionMetadata, RevisionSnapshot, RevisionVerificationError};

/// Schema version of the canonical Phase 0 JIR model.
pub const JIR_SCHEMA_VERSION: &str = "0.2";
