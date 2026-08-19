use std::collections::BTreeMap;

use jingwei_components::{EvidenceRef, ExactMpn, PackageRef};
use serde::{Deserialize, Serialize};

/// Supported library asset kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssetKind {
    Symbol,
    Footprint,
    Model3d,
}

/// Ordered library provider class.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LibraryProvider {
    Project,
    ApprovedOrganization,
    LockedOfficial,
    ApprovedThirdParty,
    Manufacturer,
    Generated,
}

/// Complete asset origin and redistribution information.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetSource {
    pub provider: LibraryProvider,
    pub upstream_id: Option<String>,
    pub upstream_version: Option<String>,
    pub source_url: Option<String>,
    pub license_id: String,
    pub retrieved_at: String,
    pub generator_name: Option<String>,
    pub generator_version: Option<String>,
    pub input_evidence_refs: Vec<String>,
    pub redistribution_allowed: bool,
}

/// Promotion is separate from verification.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalState {
    Candidate,
    ProjectPrivate,
    ProjectApproved,
    OrganizationApproved,
    Rejected,
}

/// Latest deterministic verification state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationState {
    Unverified,
    Passed,
    Failed,
}

/// Symbol pin polarity or orientation marker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Polarity {
    None,
    ActiveLow,
    Positive,
    Negative,
}

/// Typed symbol pin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolPin {
    pub id: String,
    pub unit: String,
    pub number: String,
    pub name: String,
    pub electrical_type: String,
    pub direction: String,
    pub polarity: Polarity,
    pub hidden: bool,
    pub evidence_refs: Vec<String>,
}

/// Native logical symbol definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolDefinition {
    pub units: Vec<String>,
    pub pins: Vec<SymbolPin>,
    pub pin_one_marked: bool,
}

/// Supported pad primitive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PadShape {
    Rect,
    RoundRect,
    Circle,
    Oval,
}

/// Footprint pad geometry and fabrication metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PadDefinition {
    pub id: String,
    pub number: String,
    pub shape: PadShape,
    pub x_mm: f64,
    pub y_mm: f64,
    pub width_mm: f64,
    pub height_mm: f64,
    pub rotation_deg: f64,
    pub electrical: bool,
    pub polarity: Polarity,
    pub solder_mask_expansion_mm: f64,
    pub paste_expansion_mm: f64,
}

/// Package dimensions required for footprint validation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetGeometry {
    pub body_width_mm: f64,
    pub body_height_mm: f64,
    pub pitch_mm: f64,
    pub courtyard_width_mm: f64,
    pub courtyard_height_mm: f64,
    pub origin_x_mm: f64,
    pub origin_y_mm: f64,
    pub orientation_deg: f64,
    pub model3d_alignment: Option<[f64; 3]>,
}

/// Native footprint definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintDefinition {
    pub pads: Vec<PadDefinition>,
    pub geometry: AssetGeometry,
    pub pin_one_marked: bool,
    pub silkscreen_clearance_mm: f64,
}

/// Explicit symbol-pin to footprint-pad mapping.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinPadMapEntry {
    pub logical_pin_id: String,
    pub symbol_unit: String,
    pub symbol_pin_number: String,
    pub symbol_pin_name: String,
    pub electrical_type: String,
    pub footprint_pad_numbers: Vec<String>,
    pub package_pin_name: Option<String>,
    pub allow_multiple_pads: bool,
    pub evidence_refs: Vec<String>,
}

/// Versioned map between a symbol and footprint.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinPadMap {
    pub id: String,
    pub entries: Vec<PinPadMapEntry>,
}

/// Rendered asset produced for human review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewArtifact {
    pub id: String,
    pub media_type: String,
    pub content_hash: String,
    pub evidence_refs: Vec<String>,
}

/// Provenance and compatibility shared by every asset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAsset {
    pub id: String,
    pub kind: AssetKind,
    pub logical_name: String,
    pub format: String,
    pub content_hash: String,
    pub source: AssetSource,
    pub compatible_mpns: Vec<ExactMpn>,
    pub compatible_packages: Vec<PackageRef>,
    pub symbol: Option<SymbolDefinition>,
    pub footprint: Option<FootprintDefinition>,
    pub metadata: BTreeMap<String, String>,
    pub verification_state: VerificationState,
    pub approval_state: ApprovalState,
    pub evidence: Vec<EvidenceRef>,
    pub review_artifact: Option<ReviewArtifact>,
}

/// Resolver candidate containing a symbol/footprint pair and explicit mapping.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryAssetCandidate {
    pub id: String,
    pub symbol: LibraryAsset,
    pub footprint: LibraryAsset,
    pub model3d: Option<LibraryAsset>,
    pub pin_pad_map: PinPadMap,
}
