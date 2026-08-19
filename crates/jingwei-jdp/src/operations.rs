//! Core operation input/output types.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use jingwei_jir::{
    ArtifactId, CanonicalValue, EdgeId, EntityId, JirEdge, JirEntity, ProjectId, ProposalId,
    RevisionId, SessionId, TaskId,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// All core operation names exposed by the Phase 0 daemon.
pub mod operation {
    pub const PROJECT_CREATE: &str = "project.create";
    pub const PROJECT_GET: &str = "project.get";
    pub const DESIGN_COMPILE_JDL: &str = "design.compile_jdl";
    pub const DESIGN_QUERY: &str = "design.query";
    pub const DESIGN_EXPLAIN: &str = "design.explain";
    pub const DESIGN_PATCH_PROPOSE: &str = "design.patch.propose";
    pub const DESIGN_PATCH_APPLY: &str = "design.patch.apply";
    pub const DESIGN_DIFF: &str = "design.diff";
    pub const DESIGN_VALIDATE: &str = "design.validate";
    pub const EDIT_SESSION_OPEN: &str = "edit_session.open";
    pub const EDIT_COMMAND_PREVIEW: &str = "edit_command.preview";
    pub const EDIT_COMMAND_COMMIT: &str = "edit_command.commit";
    pub const EDIT_UNDO: &str = "edit.undo";
    pub const EDIT_REDO: &str = "edit.redo";
    pub const EDIT_CHECKPOINT: &str = "edit.checkpoint";
    pub const EDIT_PUBLISH: &str = "edit.publish";
    pub const CHANGE_PROPOSAL_DECIDE: &str = "change_proposal.decide";
    pub const TASK_GET: &str = "task.get";
    pub const TASK_CANCEL: &str = "task.cancel";
    pub const TASK_EVENTS: &str = "task.events";
    pub const ARTIFACT_GET: &str = "artifact.get";
    pub const WORKSPACE_CAPABILITIES: &str = "workspace.capabilities";
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCreateInput {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub project_id: ProjectId,
    pub name: String,
    pub current_revision: RevisionId,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDecisionInput {
    pub proposal_id: ProposalId,
    pub action: jingwei_jir::ProposalAction,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDecisionOutput {
    pub proposal_id: ProposalId,
    pub action: jingwei_jir::ProposalAction,
    pub project_head: RevisionId,
    pub idempotent_replay: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileJdlInput {
    pub source_name: String,
    pub source: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileJdlOutput {
    pub revision: RevisionId,
    pub proposal_id: ProposalId,
    pub entity_count: usize,
    pub edge_count: usize,
    pub formatted_source: String,
    pub diagnostics: Vec<ProtocolDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolDiagnostic {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub source_name: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryInput {
    pub revision: Option<RevisionId>,
    pub kind: Option<String>,
    pub ids: Vec<EntityId>,
    pub limit: usize,
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryOutput {
    pub revision: RevisionId,
    pub entities: Vec<JirEntity>,
    pub edges: Vec<JirEdge>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainInput {
    pub revision: Option<RevisionId>,
    pub entity_id: EntityId,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainOutput {
    pub entity: JirEntity,
    pub incoming: Vec<JirEdge>,
    pub outgoing: Vec<JirEdge>,
    pub explanation: String,
}

/// Atomic graph patch operation.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(tag = "op", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum PatchOperation {
    AddEntity {
        entity: JirEntity,
    },
    RemoveEntity {
        entity_id: EntityId,
    },
    SetProperty {
        entity_id: EntityId,
        key: String,
        value: CanonicalValue,
    },
    RemoveProperty {
        entity_id: EntityId,
        key: String,
    },
    SetEditPolicy {
        entity_id: EntityId,
        policy: jingwei_jir::EditPolicy,
    },
    AddEdge {
        edge: JirEdge,
    },
    RemoveEdge {
        edge_id: EdgeId,
    },
}

/// Preconditions evaluated before a patch changes the isolated graph.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Precondition {
    EntityExists {
        entity_id: EntityId,
    },
    EntityAbsent {
        entity_id: EntityId,
    },
    PropertyEquals {
        entity_id: EntityId,
        key: String,
        expected: Value,
    },
    ObjectEditable {
        entity_id: EntityId,
    },
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchInput {
    pub operations: Vec<PatchOperation>,
    #[serde(default)]
    pub preconditions: Vec<Precondition>,
    #[serde(default)]
    pub expected_postconditions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchProposal {
    pub proposal_id: String,
    pub base_revision: RevisionId,
    pub normalized_input_hash: String,
    pub diff: jingwei_jir::GraphDiff,
    pub validation: ValidationOutput,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchApplyInput {
    pub proposal_id: Option<String>,
    #[serde(flatten)]
    pub patch: PatchInput,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchOutput {
    pub revision: RevisionId,
    pub proposal_id: ProposalId,
    pub diff: jingwei_jir::GraphDiff,
    pub validation: ValidationOutput,
    pub idempotent_replay: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffInput {
    pub left: RevisionId,
    pub right: RevisionId,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffOutput {
    pub left: RevisionId,
    pub right: RevisionId,
    pub diff: jingwei_jir::GraphDiff,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateInput {
    pub revision: Option<RevisionId>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationOutput {
    pub valid: bool,
    pub findings: Vec<ValidationFinding>,
    pub providers_run: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationFinding {
    pub code: String,
    pub severity: String,
    pub summary: String,
    pub object_refs: Vec<String>,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCapabilities {
    pub protocol_versions: Vec<String>,
    pub schema_versions: Vec<String>,
    pub operations: BTreeMap<String, CapabilityState>,
    pub read_write: bool,
    pub transport: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityState {
    Available,
    CapabilityUnavailable,
}

/// Durable aggregate event.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DurableEvent<T> {
    pub schema_version: String,
    pub aggregate_id: String,
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub session_id: Option<SessionId>,
    pub task_id: Option<TaskId>,
    pub trace_id: Option<String>,
    #[serde(rename = "type")]
    pub event_type: String,
    pub data: T,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventsInput {
    pub after_sequence: u64,
    pub limit: usize,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRef {
    pub task_id: TaskId,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskState {
    pub task_id: TaskId,
    pub state: String,
    pub cancellable: bool,
    pub terminal: bool,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactGetInput {
    pub artifact_id: ArtifactId,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactGetOutput {
    pub artifact_id: ArtifactId,
    pub media_type: String,
    pub sha256: String,
    pub bytes_hex: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeRequest {
    pub protocol_versions: Vec<String>,
    pub workspace_id: jingwei_jir::WorkspaceId,
    pub client_name: String,
    pub client_version: String,
    pub session_token: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeResponse {
    pub protocol_version: String,
    pub daemon_build: String,
    pub daemon_instance: String,
    pub daemon_epoch: u64,
    pub workspace_id: jingwei_jir::WorkspaceId,
    pub read_write: bool,
    pub capabilities: Vec<String>,
}
