//! Immutable lifecycle records kept separate from canonical design snapshots.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AssessmentId, AttestationId, CommitId, DecisionId, GateResultId, InvocationId, ProjectId,
    ProposalId, ReleaseId, ReleaseRequestId, RequestId, RevisionId, RevokeId,
};

/// Authenticated human or service identity. Request payloads cannot self-assert this value.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatedActorRef {
    pub subject: String,
    pub issuer: String,
    pub authentication_method: String,
    #[serde(default)]
    pub role_claims: BTreeSet<String>,
}

/// Trusted origin of a design-changing invocation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum InvocationSourceKind {
    AgentTool,
    UiCommand,
    CliCommand,
    SystemMigration,
}

/// Correlation attached by Runtime or daemon after authenticating the caller.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvocationCorrelation {
    pub invocation_id: InvocationId,
    pub source_kind: InvocationSourceKind,
    pub source_id: String,
    pub request_id: RequestId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_epoch: Option<u64>,
}

/// Append-only authorship and invocation proof for one canonical revision snapshot.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevisionCommitRecord {
    pub id: CommitId,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub actor: AuthenticatedActorRef,
    pub issued_at: DateTime<Utc>,
    pub correlation: InvocationCorrelation,
}

/// Immutable proposal to move a project head from one exact base to one revision.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeProposal {
    pub id: ProposalId,
    pub project_id: ProjectId,
    pub base_project_head: RevisionId,
    pub proposed_revision: RevisionId,
    pub created_by_commit: CommitId,
    pub issued_at: DateTime<Utc>,
}

/// Append-only decision applied to a change proposal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProposalAction {
    Accept,
    Reject,
    Withdraw,
}

/// Authenticated proposal decision. Accept is atomically coupled to project-head CAS.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDecisionRecord {
    pub id: DecisionId,
    pub proposal_id: ProposalId,
    pub action: ProposalAction,
    pub actor: AuthenticatedActorRef,
    pub issued_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Engineering maturity. Production release (L5) is deliberately not a member.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "kebab-case")]
pub enum MaturityLevel {
    L0Draft,
    L1EcadCandidate,
    L2EngineeringVerified,
    L3ManufacturingCandidate,
    L4PrototypeVerified,
}

/// Independent dependency locks that make an assessment reproducible.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DependencyLocks {
    pub intent_revision: String,
    pub toolchain_lock: String,
    pub rule_set_lock: String,
    pub manufacturer_profile_lock: String,
    pub evidence_set_hash: String,
    pub waiver_set_hash: String,
}

/// Deterministic gate status.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateStatus {
    Passed,
    Blocked,
    Stale,
    Invalid,
}

/// One immutable gate result bound to the full assessment key.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GateResult {
    pub id: GateResultId,
    pub gate: String,
    pub status: GateStatus,
    pub finding_refs: Vec<String>,
    pub provider: String,
}

/// Recomputable engineering maturity statement for one exact revision and dependency set.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaturityAssessment {
    pub id: AssessmentId,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub locks: DependencyLocks,
    pub highest_level: MaturityLevel,
    pub gates: Vec<GateResult>,
    pub issued_at: DateTime<Utc>,
}

/// Role-bound, authenticated human attestation.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalAttestation {
    pub id: AttestationId,
    pub role: String,
    pub actor: AuthenticatedActorRef,
    pub assessment_id: AssessmentId,
    pub issued_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Immutable request sent to the independent organization release signer.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRequest {
    pub id: ReleaseRequestId,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub assessment_id: AssessmentId,
    pub artifact_manifest_hash: String,
    pub attestation_ids: Vec<AttestationId>,
    pub issued_at: DateTime<Utc>,
}

/// Organization-signed production release. Revocation never mutates this record.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseRecord {
    pub id: ReleaseId,
    pub request: ReleaseRequest,
    pub signer_key_id: String,
    pub signature: String,
    pub issued_at: DateTime<Utc>,
}

/// Append-only organization-signed revocation.
#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeRecord {
    pub id: RevokeId,
    pub release_id: ReleaseId,
    pub reason: String,
    pub signer_key_id: String,
    pub signature: String,
    pub issued_at: DateTime<Utc>,
}

/// Verifier projection over immutable release, revoke, assessment, and artifact bytes.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EffectiveReleaseStatus {
    Active,
    Revoked,
    Stale,
    Invalid,
}
