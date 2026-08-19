use std::collections::{BTreeMap, BTreeSet};

use jingwei_components::{EvidenceRef, ExactMpn, PackageRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    ApprovalState, AssetGeometry, AssetKind, AssetSource, FootprintDefinition, LibraryAsset,
    LibraryAssetCandidate, LibraryProvider, PadDefinition, PinPadMap, PinPadMapEntry, Polarity,
    ReviewArtifact, SymbolDefinition, SymbolPin, VerificationState,
};

/// Trusted structured pin-table row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredPinInput {
    pub number: String,
    pub name: String,
    pub unit: String,
    pub electrical_type: String,
    pub direction: String,
    pub polarity: Polarity,
    pub pad_numbers: Vec<String>,
    pub allow_multiple_pads: bool,
    pub evidence_refs: Vec<String>,
}

/// Trusted structured package and land-pattern table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredFootprintInput {
    pub logical_name: String,
    pub package: PackageRef,
    pub manufacturer: String,
    pub exact_mpn: ExactMpn,
    pub pins: Vec<StructuredPinInput>,
    pub pads: Vec<PadDefinition>,
    pub geometry: AssetGeometry,
    pub pin_one_marked: bool,
    pub silkscreen_clearance_mm: f64,
    pub source_evidence: Vec<EvidenceRef>,
    pub generator_version: String,
}

/// Structured generation failure.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum GenerationError {
    #[error("structured generation requires at least one traceable evidence reference")]
    MissingEvidence,
    #[error("structured generation input has duplicate pin number: {0}")]
    DuplicatePin(String),
    #[error("structured generation input has duplicate pad identifier: {0}")]
    DuplicatePad(String),
    #[error("pin {0} has no explicit pad mapping")]
    MissingPadMap(String),
    #[error("pin {pin} references unknown pad {pad}")]
    UnknownPad { pin: String, pad: String },
}

/// Generates project-private candidates only from controlled structured tables.
pub fn generate_assets(
    input: &StructuredFootprintInput,
) -> Result<LibraryAssetCandidate, GenerationError> {
    if input.source_evidence.is_empty() {
        return Err(GenerationError::MissingEvidence);
    }
    let mut pin_numbers = BTreeSet::new();
    let mut pad_ids = BTreeSet::new();
    let pad_numbers = input
        .pads
        .iter()
        .map(|pad| pad.number.clone())
        .collect::<BTreeSet<_>>();
    for pad in &input.pads {
        if !pad_ids.insert(&pad.id) {
            return Err(GenerationError::DuplicatePad(pad.id.clone()));
        }
    }
    for pin in &input.pins {
        if !pin_numbers.insert(&pin.number) {
            return Err(GenerationError::DuplicatePin(pin.number.clone()));
        }
        if pin.pad_numbers.is_empty() {
            return Err(GenerationError::MissingPadMap(pin.number.clone()));
        }
        for pad in &pin.pad_numbers {
            if !pad_numbers.contains(pad) {
                return Err(GenerationError::UnknownPad {
                    pin: pin.number.clone(),
                    pad: pad.clone(),
                });
            }
        }
    }

    let evidence_refs = input
        .source_evidence
        .iter()
        .map(|evidence| evidence.id.clone())
        .collect::<Vec<_>>();
    let source = AssetSource {
        provider: LibraryProvider::Generated,
        upstream_id: None,
        upstream_version: None,
        source_url: None,
        license_id: "project-private-generated".to_owned(),
        retrieved_at: input.source_evidence[0].retrieved_at.clone(),
        generator_name: Some("jingwei-structured-library-generator".to_owned()),
        generator_version: Some(input.generator_version.clone()),
        input_evidence_refs: evidence_refs.clone(),
        redistribution_allowed: false,
    };
    let symbol_definition = SymbolDefinition {
        units: input
            .pins
            .iter()
            .map(|pin| pin.unit.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        pins: input
            .pins
            .iter()
            .map(|pin| SymbolPin {
                id: format!("pin:{}:{}", input.exact_mpn.as_str(), pin.number),
                unit: pin.unit.clone(),
                number: pin.number.clone(),
                name: pin.name.clone(),
                electrical_type: pin.electrical_type.clone(),
                direction: pin.direction.clone(),
                polarity: pin.polarity.clone(),
                hidden: false,
                evidence_refs: pin.evidence_refs.clone(),
            })
            .collect(),
        pin_one_marked: input.pin_one_marked,
    };
    let footprint_definition = FootprintDefinition {
        pads: input.pads.clone(),
        geometry: input.geometry.clone(),
        pin_one_marked: input.pin_one_marked,
        silkscreen_clearance_mm: input.silkscreen_clearance_mm,
    };
    let map = PinPadMap {
        id: format!("pin-pad-map:{}", input.exact_mpn.as_str()),
        entries: input
            .pins
            .iter()
            .map(|pin| PinPadMapEntry {
                logical_pin_id: format!("pin:{}:{}", input.exact_mpn.as_str(), pin.number),
                symbol_unit: pin.unit.clone(),
                symbol_pin_number: pin.number.clone(),
                symbol_pin_name: pin.name.clone(),
                electrical_type: pin.electrical_type.clone(),
                footprint_pad_numbers: pin.pad_numbers.clone(),
                package_pin_name: None,
                allow_multiple_pads: pin.allow_multiple_pads,
                evidence_refs: pin.evidence_refs.clone(),
            })
            .collect(),
    };

    let symbol_hash = hash_debug(&symbol_definition);
    let footprint_hash = hash_debug(&footprint_definition);
    let review_hash = hash_debug(&(symbol_hash.as_str(), footprint_hash.as_str(), &map));
    let compatibility = vec![input.exact_mpn.clone()];
    let packages = vec![input.package.clone()];
    let metadata = BTreeMap::from([
        ("manufacturer".to_owned(), input.manufacturer.clone()),
        (
            "generationInput".to_owned(),
            "structured-pin-package-table".to_owned(),
        ),
        ("manualReviewRequired".to_owned(), "true".to_owned()),
    ]);
    let review = Some(ReviewArtifact {
        id: format!("review:{}", input.exact_mpn.as_str()),
        media_type: "application/vnd.jingwei.library-review+json".to_owned(),
        content_hash: review_hash,
        evidence_refs: evidence_refs.clone(),
    });
    Ok(LibraryAssetCandidate {
        id: format!("library-candidate:{}", input.exact_mpn.as_str()),
        symbol: LibraryAsset {
            id: format!("symbol:{}", input.exact_mpn.as_str()),
            kind: AssetKind::Symbol,
            logical_name: input.logical_name.clone(),
            format: "jingwei-symbol/0.1".to_owned(),
            content_hash: symbol_hash,
            source: source.clone(),
            compatible_mpns: compatibility.clone(),
            compatible_packages: packages.clone(),
            symbol: Some(symbol_definition),
            footprint: None,
            metadata: metadata.clone(),
            verification_state: VerificationState::Unverified,
            approval_state: ApprovalState::ProjectPrivate,
            evidence: input.source_evidence.clone(),
            review_artifact: review.clone(),
        },
        footprint: LibraryAsset {
            id: format!("footprint:{}", input.exact_mpn.as_str()),
            kind: AssetKind::Footprint,
            logical_name: input.package.name.clone(),
            format: "jingwei-footprint/0.1".to_owned(),
            content_hash: footprint_hash,
            source,
            compatible_mpns: compatibility,
            compatible_packages: packages,
            symbol: None,
            footprint: Some(footprint_definition),
            metadata,
            verification_state: VerificationState::Unverified,
            approval_state: ApprovalState::ProjectPrivate,
            evidence: input.source_evidence.clone(),
            review_artifact: review,
        },
        model3d: None,
        pin_pad_map: map,
    })
}

fn hash_debug(value: &impl std::fmt::Debug) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{value:#?}").as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}
