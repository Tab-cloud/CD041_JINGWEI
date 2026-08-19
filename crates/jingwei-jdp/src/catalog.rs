//! Tool catalog generated from the Rust-owned operation list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::operation;

/// Permission required to invoke an operation.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Permission {
    Read,
    CandidateWrite,
    ReviewDecision,
    ExternalProcess,
    Waiver,
    ReleaseCandidate,
    ProductionRelease,
}

/// Declared operation side effect.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Effect {
    ReadOnly,
    RevisionWrite,
    DraftWrite,
    TaskControl,
    ArtifactRead,
    ArtifactWrite,
}

/// Generated descriptor consumed by Agent runtime and policy layers.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub permission: Permission,
    pub effect: Effect,
    pub resource_claim_template: Vec<String>,
    pub idempotent: bool,
    pub requires_base_revision: bool,
    pub cancellable: bool,
    pub default_timeout_ms: u64,
    pub protocol_version: String,
    pub schema_version: String,
}

/// Returns the stable Phase 0 catalog.
pub fn tool_catalog() -> Vec<ToolDescriptor> {
    let rows = [
        (
            operation::PROJECT_CREATE,
            "Create an immutable empty project",
            Permission::CandidateWrite,
            Effect::RevisionWrite,
        ),
        (
            operation::PROJECT_GET,
            "Read project metadata and current revision",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            operation::DESIGN_COMPILE_JDL,
            "Compile bounded declarative JDL into a JIR revision",
            Permission::CandidateWrite,
            Effect::RevisionWrite,
        ),
        (
            operation::DESIGN_QUERY,
            "Query a bounded subset of a JIR graph",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            operation::DESIGN_EXPLAIN,
            "Explain one entity using provenance and graph relations",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            operation::DESIGN_PATCH_PROPOSE,
            "Validate and preview an atomic JIR patch",
            Permission::CandidateWrite,
            Effect::ReadOnly,
        ),
        (
            operation::DESIGN_PATCH_APPLY,
            "Atomically apply a preconditioned JIR patch",
            Permission::CandidateWrite,
            Effect::RevisionWrite,
        ),
        (
            operation::DESIGN_DIFF,
            "Compute a semantic JIR graph diff",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            operation::DESIGN_VALIDATE,
            "Run core and registered validation providers",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            operation::DESIGN_VIEW,
            "Render a bounded semantic display list from an immutable JIR revision",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            operation::DESIGN_HIT_TEST,
            "Hit-test semantic display geometry using stable JIR identities",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            operation::DESIGN_PROJECT,
            "Materialize a deterministic external projection into content-addressed artifacts",
            Permission::CandidateWrite,
            Effect::ArtifactWrite,
        ),
        (
            operation::EDIT_SESSION_OPEN,
            "Open a leased draft branch",
            Permission::CandidateWrite,
            Effect::DraftWrite,
        ),
        (
            operation::EDIT_COMMAND_PREVIEW,
            "Preview semantic edit commands without writing JIR",
            Permission::CandidateWrite,
            Effect::ReadOnly,
        ),
        (
            operation::EDIT_COMMAND_COMMIT,
            "Commit a semantic command group to a draft revision",
            Permission::CandidateWrite,
            Effect::DraftWrite,
        ),
        (
            operation::EDIT_UNDO,
            "Create an inverse draft revision",
            Permission::CandidateWrite,
            Effect::DraftWrite,
        ),
        (
            operation::EDIT_REDO,
            "Create a reapply draft revision",
            Permission::CandidateWrite,
            Effect::DraftWrite,
        ),
        (
            operation::EDIT_CHECKPOINT,
            "Persist draft session metadata",
            Permission::CandidateWrite,
            Effect::DraftWrite,
        ),
        (
            operation::EDIT_PUBLISH,
            "Publish a draft as an immutable change proposal without moving project head",
            Permission::CandidateWrite,
            Effect::RevisionWrite,
        ),
        (
            operation::CHANGE_PROPOSAL_DECIDE,
            "Append an authenticated proposal decision; accept atomically moves project head",
            Permission::ReviewDecision,
            Effect::RevisionWrite,
        ),
        (
            operation::EXECUTION_RECONCILE,
            "Read a terminal execution result after a response is lost",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            operation::TASK_GET,
            "Read durable task state",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            operation::TASK_CANCEL,
            "Request durable task cancellation",
            Permission::CandidateWrite,
            Effect::TaskControl,
        ),
        (
            operation::TASK_WAIT,
            "Wait for a durable task to reach a terminal state",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            operation::TASK_EVENTS,
            "Replay durable task events",
            Permission::Read,
            Effect::ReadOnly,
        ),
        (
            operation::ARTIFACT_GET,
            "Read a content-addressed artifact",
            Permission::Read,
            Effect::ArtifactRead,
        ),
        (
            operation::WORKSPACE_CAPABILITIES,
            "Negotiate workspace and operation capabilities",
            Permission::Read,
            Effect::ReadOnly,
        ),
    ];
    rows.into_iter()
        .map(|(name, description, permission, effect)| ToolDescriptor {
            name: name.into(),
            description: description.into(),
            input_schema: json!({"type": "object"}),
            output_schema: json!({"type": "object"}),
            permission,
            effect,
            resource_claim_template: if matches!(
                effect,
                Effect::RevisionWrite | Effect::DraftWrite | Effect::ArtifactWrite
            ) {
                vec![
                    "workspace:{workspaceId}".into(),
                    "project:{projectId}".into(),
                ]
            } else {
                vec!["workspace:{workspaceId}".into()]
            },
            idempotent: true,
            requires_base_revision: matches!(
                name,
                operation::DESIGN_COMPILE_JDL
                    | operation::DESIGN_PATCH_PROPOSE
                    | operation::DESIGN_PATCH_APPLY
            ),
            cancellable: matches!(effect, Effect::TaskControl),
            default_timeout_ms: 30_000,
            protocol_version: crate::JDP_PROTOCOL_VERSION.into(),
            schema_version: jingwei_jir::JIR_SCHEMA_VERSION.into(),
        })
        .collect()
}
