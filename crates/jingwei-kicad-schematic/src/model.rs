use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable mapping from JIR identity to an external KiCad-like UUID.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KicadObjectMapEntry {
    pub jir_entity_id: String,
    pub kicad_uuid: String,
    pub object_kind: String,
}

/// Projected pin instance with its logical-net membership retained.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KicadPinProjection {
    pub uuid: String,
    pub jir_entity_id: String,
    pub number: String,
    pub name: String,
    pub electrical_type: String,
    pub required_connection: bool,
    pub net_uuid: Option<String>,
}

/// Projected symbol instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KicadSymbolProjection {
    pub uuid: String,
    pub jir_entity_id: String,
    pub sheet_uuid: String,
    pub library_id: String,
    pub reference: String,
    pub value: String,
    pub footprint: Option<String>,
    pub x_mm: f64,
    pub y_mm: f64,
    pub rotation_deg: f64,
    pub mirrored: bool,
    pub pins: Vec<KicadPinProjection>,
}

/// Projected logical net and pin membership.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KicadNetProjection {
    pub uuid: String,
    pub jir_entity_id: String,
    pub name: String,
    pub pin_uuids: Vec<String>,
}

/// Projection capability and explicit omissions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionCoverage {
    pub projected_features: Vec<String>,
    pub coverage_gaps: Vec<String>,
    pub round_trip_model_verified: bool,
    pub kicad_cli_verified: bool,
}

/// Versioned, parseable reference projection model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KicadProjectionModel {
    pub schema_version: String,
    pub target_kicad_version: String,
    pub generator: String,
    pub generator_version: String,
    pub source_graph_id: String,
    pub root_uuid: String,
    pub sheets: BTreeMap<String, String>,
    pub symbols: Vec<KicadSymbolProjection>,
    pub nets: Vec<KicadNetProjection>,
    pub object_map: Vec<KicadObjectMapEntry>,
    pub coverage: ProjectionCoverage,
}

/// Metadata extracted by the native KiCad S-expression parser.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KicadNativeSummary {
    pub format_version: String,
    pub generator: String,
    pub root_uuid: String,
    pub library_symbol_count: usize,
    pub symbol_count: usize,
    pub pin_count: usize,
    pub label_count: usize,
    pub no_connect_count: usize,
}

/// Complete native KiCad export plus its traceability sidecar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KicadNativeArtifact {
    pub schematic: Vec<u8>,
    pub object_map: Vec<u8>,
    pub symbol_library: Vec<u8>,
    pub symbol_library_table: Vec<u8>,
    pub summary: KicadNativeSummary,
}

/// Typed projection failure.
#[derive(Debug, Error)]
pub enum ProjectionError {
    #[error("schematic graph is invalid: {0}")]
    InvalidGraph(String),
    #[error("projection serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("projection model version is unsupported: {0}")]
    UnsupportedVersion(String),
    #[error("projection object map is inconsistent: {0}")]
    InvalidObjectMap(String),
    #[error("native KiCad schematic data is invalid: {0}")]
    InvalidNativeData(String),
    #[error("native KiCad schematic syntax is invalid: {0}")]
    InvalidNativeSyntax(String),
}
