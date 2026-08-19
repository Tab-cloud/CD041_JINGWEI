//! Common display, hit-test, cross-probe, and semantic edit envelopes.

use jingwei_jir::{EntityId, RevisionId, SessionId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewRequest {
    pub revision: RevisionId,
    pub viewport: Rect,
    pub layers: Vec<String>,
    pub level_of_detail: u8,
    pub selection: Vec<EntityId>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum DisplayPrimitive {
    Line {
        from: Point,
        to: Point,
        entity_id: EntityId,
    },
    Polyline {
        points: Vec<Point>,
        entity_id: EntityId,
    },
    Polygon {
        points: Vec<Point>,
        entity_id: EntityId,
    },
    Text {
        position: Point,
        text: String,
        entity_id: EntityId,
    },
    Glyph {
        position: Point,
        name: String,
        entity_id: EntityId,
    },
    Marker {
        position: Point,
        marker: String,
        entity_id: EntityId,
    },
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayList {
    pub revision: RevisionId,
    pub primitives: Vec<DisplayPrimitive>,
    pub artifact_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HitTestRequest {
    pub revision: RevisionId,
    pub point: Point,
    pub tolerance: f64,
    pub layers: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HitTestResult {
    pub hits: Vec<Hit>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub entity_id: EntityId,
    pub distance: f64,
    pub primitive_kind: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticEditCommand {
    pub kind: String,
    pub payload: Value,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditCommandGroup {
    pub edit_session_id: SessionId,
    pub base_draft_revision: RevisionId,
    pub commands: Vec<SemanticEditCommand>,
    pub expected_postconditions: Vec<String>,
    pub preview_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSessionOpenInput {
    pub base_revision: RevisionId,
    pub lease_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSessionInfo {
    pub edit_session_id: SessionId,
    pub base_project_revision: RevisionId,
    pub draft_head_revision: RevisionId,
    pub lease_expires_at: chrono::DateTime<chrono::Utc>,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPreviewOutput {
    pub preview_hash: String,
    pub validation: crate::ValidationOutput,
    pub display_delta: Vec<DisplayPrimitive>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditCommitOutput {
    pub draft_revision: RevisionId,
    pub validation: crate::ValidationOutput,
    pub diff: jingwei_jir::GraphDiff,
}

/// Input shared by undo, redo, checkpoint, and publish routes.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSessionRef {
    pub edit_session_id: SessionId,
}

/// Result of publishing a draft branch into the project candidate pointer.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPublishOutput {
    pub revision: RevisionId,
    pub diff: jingwei_jir::GraphDiff,
}

#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossProbeResult {
    pub source_entity: EntityId,
    pub related_entities: Vec<EntityId>,
    pub relation_kinds: Vec<String>,
}
