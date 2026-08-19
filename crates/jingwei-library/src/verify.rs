use std::collections::{BTreeMap, BTreeSet};

use jingwei_components::{Finding, FindingCoverage, Severity, SuggestedAction};
use serde::{Deserialize, Serialize};

use crate::{LibraryAssetCandidate, Polarity};

/// Complete library validation result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryVerification {
    pub candidate_id: String,
    pub findings: Vec<Finding>,
    pub checked_rules: Vec<String>,
    pub blocking: bool,
    pub manual_review_required: bool,
}

/// Validates pin/pad mapping, geometry, polarity, mask/paste and review evidence.
#[must_use]
pub fn verify_library(candidate: &LibraryAssetCandidate) -> LibraryVerification {
    let mut findings = Vec::new();
    let checked_rules = vec![
        "LIB.PIN_COUNT",
        "LIB.PIN_FIELDS",
        "LIB.PAD_COUNT",
        "LIB.PIN_PAD_MAP",
        "LIB.PIN_ONE_POLARITY",
        "LIB.MECHANICAL_DIMENSIONS",
        "LIB.COURTYARD",
        "LIB.ORIGIN_ORIENTATION",
        "LIB.MODEL3D_ALIGNMENT",
        "LIB.MASK_PASTE",
        "LIB.SILKSCREEN",
        "LIB.RENDER_REVIEW",
        "LIB.PROVENANCE_LICENSE",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();

    let Some(symbol) = &candidate.symbol.symbol else {
        findings.push(blocking(
            "LIB.PIN_COUNT",
            vec![candidate.symbol.id.clone()],
            "symbol asset does not contain a symbol definition",
        ));
        return result(candidate, findings, checked_rules);
    };
    let Some(footprint) = &candidate.footprint.footprint else {
        findings.push(blocking(
            "LIB.PAD_COUNT",
            vec![candidate.footprint.id.clone()],
            "footprint asset does not contain a footprint definition",
        ));
        return result(candidate, findings, checked_rules);
    };

    let pin_numbers = symbol
        .pins
        .iter()
        .map(|pin| pin.number.clone())
        .collect::<Vec<_>>();
    let unique_pins = pin_numbers.iter().collect::<BTreeSet<_>>();
    if unique_pins.len() != pin_numbers.len() {
        findings.push(blocking(
            "LIB.PIN_COUNT",
            vec![candidate.symbol.id.clone()],
            "duplicate symbol pin number",
        ));
    }
    for pin in &symbol.pins {
        if pin.number.trim().is_empty()
            || pin.name.trim().is_empty()
            || pin.electrical_type.trim().is_empty()
            || pin.direction.trim().is_empty()
            || pin.unit.trim().is_empty()
        {
            findings.push(blocking(
                "LIB.PIN_FIELDS",
                vec![candidate.symbol.id.clone(), pin.id.clone()],
                "pin number, name, electrical type, direction and unit are required",
            ));
        }
    }

    let electrical_pads = footprint
        .pads
        .iter()
        .filter(|pad| pad.electrical)
        .map(|pad| pad.number.clone())
        .collect::<Vec<_>>();
    let pad_counts = electrical_pads
        .iter()
        .fold(BTreeMap::new(), |mut map, number| {
            *map.entry(number.clone()).or_insert(0_u32) += 1;
            map
        });
    for (number, count) in &pad_counts {
        if *count > 1 {
            findings.push(blocking(
                "LIB.PAD_COUNT",
                vec![candidate.footprint.id.clone()],
                &format!("duplicate electrical pad number {number}"),
            ));
        }
    }

    let mapped_pins = candidate
        .pin_pad_map
        .entries
        .iter()
        .map(|entry| entry.symbol_pin_number.clone())
        .collect::<Vec<_>>();
    for pin in &symbol.pins {
        let matches = candidate
            .pin_pad_map
            .entries
            .iter()
            .filter(|entry| entry.symbol_pin_number == pin.number)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            findings.push(blocking(
                "LIB.PIN_PAD_MAP",
                vec![candidate.symbol.id.clone(), pin.id.clone()],
                "each symbol pin must have exactly one explicit map entry",
            ));
            continue;
        }
        let entry = matches[0];
        if entry.footprint_pad_numbers.is_empty() {
            findings.push(blocking(
                "LIB.PIN_PAD_MAP",
                vec![pin.id.clone()],
                "pin-pad map entry has no pad",
            ));
        }
        if entry.footprint_pad_numbers.len() > 1 && !entry.allow_multiple_pads {
            findings.push(blocking(
                "LIB.PIN_PAD_MAP",
                vec![pin.id.clone()],
                "multiple pads require explicit allowMultiplePads semantics",
            ));
        }
        for pad_number in &entry.footprint_pad_numbers {
            if !pad_counts.contains_key(pad_number) {
                findings.push(blocking(
                    "LIB.PIN_PAD_MAP",
                    vec![pin.id.clone(), candidate.footprint.id.clone()],
                    &format!("pin references missing electrical pad {pad_number}"),
                ));
            }
        }
    }
    if mapped_pins.len() != mapped_pins.iter().collect::<BTreeSet<_>>().len() {
        findings.push(blocking(
            "LIB.PIN_PAD_MAP",
            vec![candidate.pin_pad_map.id.clone()],
            "duplicate symbol pin mapping",
        ));
    }

    let pin_one = symbol.pins.iter().find(|pin| pin.number == "1");
    let pad_one = footprint.pads.iter().find(|pad| pad.number == "1");
    if pin_one.is_none() || pad_one.is_none() || !symbol.pin_one_marked || !footprint.pin_one_marked
    {
        findings.push(blocking(
            "LIB.PIN_ONE_POLARITY",
            vec![candidate.symbol.id.clone(), candidate.footprint.id.clone()],
            "symbol and footprint must contain and visibly mark pin 1",
        ));
    }
    if let (Some(pin), Some(pad)) = (pin_one, pad_one)
        && pin.polarity != Polarity::None
        && pad.polarity != pin.polarity
    {
        findings.push(blocking(
            "LIB.PIN_ONE_POLARITY",
            vec![pin.id.clone(), pad.id.clone()],
            "pin 1 polarity metadata differs between symbol and footprint",
        ));
    }

    let geometry = &footprint.geometry;
    if geometry.body_width_mm <= 0.0 || geometry.body_height_mm <= 0.0 || geometry.pitch_mm <= 0.0 {
        findings.push(blocking(
            "LIB.MECHANICAL_DIMENSIONS",
            vec![candidate.footprint.id.clone()],
            "body width, body height and pitch must be positive",
        ));
    }
    if geometry.courtyard_width_mm <= geometry.body_width_mm
        || geometry.courtyard_height_mm <= geometry.body_height_mm
    {
        findings.push(blocking(
            "LIB.COURTYARD",
            vec![candidate.footprint.id.clone()],
            "courtyard must extend beyond the component body",
        ));
    }
    if !geometry.origin_x_mm.is_finite()
        || !geometry.origin_y_mm.is_finite()
        || !geometry.orientation_deg.is_finite()
    {
        findings.push(blocking(
            "LIB.ORIGIN_ORIENTATION",
            vec![candidate.footprint.id.clone()],
            "origin and orientation metadata must be finite",
        ));
    }
    if candidate.model3d.is_some() && geometry.model3d_alignment.is_none() {
        findings.push(blocking(
            "LIB.MODEL3D_ALIGNMENT",
            vec![candidate.footprint.id.clone()],
            "a resolved 3D model requires explicit alignment metadata",
        ));
    }
    for pad in &footprint.pads {
        if pad.width_mm <= 0.0 || pad.height_mm <= 0.0 {
            findings.push(blocking(
                "LIB.MECHANICAL_DIMENSIONS",
                vec![pad.id.clone()],
                "pad dimensions must be positive",
            ));
        }
        if pad.solder_mask_expansion_mm < -pad.width_mm / 2.0
            || pad.paste_expansion_mm < -pad.width_mm / 2.0
        {
            findings.push(blocking(
                "LIB.MASK_PASTE",
                vec![pad.id.clone()],
                "mask or paste contraction removes the complete pad opening",
            ));
        }
    }
    if footprint.silkscreen_clearance_mm < 0.1 {
        findings.push(warning(
            "LIB.SILKSCREEN",
            vec![candidate.footprint.id.clone()],
            "silkscreen clearance is below the project baseline of 0.1 mm",
        ));
    }
    if candidate.symbol.review_artifact.is_none() || candidate.footprint.review_artifact.is_none() {
        findings.push(blocking(
            "LIB.RENDER_REVIEW",
            vec![candidate.id.clone()],
            "symbol and footprint require a rendered review artifact",
        ));
    }
    for asset in [&candidate.symbol, &candidate.footprint] {
        if asset.content_hash.trim().is_empty()
            || asset.source.license_id.trim().is_empty()
            || asset.source.input_evidence_refs.is_empty()
        {
            findings.push(blocking(
                "LIB.PROVENANCE_LICENSE",
                vec![asset.id.clone()],
                "asset hash, license and input evidence are required",
            ));
        }
    }
    result(candidate, findings, checked_rules)
}

fn result(
    candidate: &LibraryAssetCandidate,
    findings: Vec<Finding>,
    checked_rules: Vec<String>,
) -> LibraryVerification {
    let blocking = findings
        .iter()
        .any(|finding| matches!(finding.severity, Severity::Blocking | Severity::Error));
    LibraryVerification {
        candidate_id: candidate.id.clone(),
        findings,
        checked_rules,
        blocking,
        manual_review_required: true,
    }
}

fn blocking(rule_id: &str, object_refs: Vec<String>, message: &str) -> Finding {
    finding(rule_id, Severity::Blocking, object_refs, message)
}

fn warning(rule_id: &str, object_refs: Vec<String>, message: &str) -> Finding {
    finding(rule_id, Severity::Warning, object_refs, message)
}

fn finding(rule_id: &str, severity: Severity, object_refs: Vec<String>, message: &str) -> Finding {
    Finding::new(
        rule_id,
        "1.0.0",
        severity,
        object_refs,
        Vec::new(),
        message,
        vec![SuggestedAction {
            kind: "repair".to_owned(),
            description: "Repair the structured asset and re-run independent verification."
                .to_owned(),
        }],
        FindingCoverage {
            status: "implemented".to_owned(),
            checked_aspects: vec!["structured symbol, footprint and pin-pad metadata".to_owned()],
            gaps: vec!["manual rendered review remains required".to_owned()],
        },
    )
}

#[cfg(test)]
mod tests {
    use jingwei_components::EvidenceRef;

    use super::*;
    use crate::{
        AssetGeometry, PadDefinition, PadShape, StructuredFootprintInput, StructuredPinInput,
        generate_assets,
    };
    use jingwei_components::{ExactMpn, PackageRef};

    fn evidence() -> EvidenceRef {
        EvidenceRef {
            id: "evidence:structured-package".to_owned(),
            kind: "datasheet".to_owned(),
            source_uri: None,
            publisher: "Jingwei fixture".to_owned(),
            version: "1".to_owned(),
            retrieved_at: "2026-08-19T00:00:00Z".to_owned(),
            content_hash: format!("sha256:{}", "1".repeat(64)),
            locator: Some("structured table".to_owned()),
            license: "CC0-1.0".to_owned(),
            cache_policy: "redistributable".to_owned(),
        }
    }

    fn input() -> StructuredFootprintInput {
        StructuredFootprintInput {
            logical_name: "JW-SYNTH-2".to_owned(),
            package: PackageRef {
                name: "SOT-23-2".to_owned(),
                pin_count: 2,
            },
            manufacturer: "Jingwei Fixtures".to_owned(),
            exact_mpn: ExactMpn::parse("JW-SYNTH-2").expect("valid MPN"),
            pins: vec![
                StructuredPinInput {
                    number: "1".to_owned(),
                    name: "A".to_owned(),
                    unit: "A".to_owned(),
                    electrical_type: "passive".to_owned(),
                    direction: "bidirectional".to_owned(),
                    polarity: Polarity::Positive,
                    pad_numbers: vec!["1".to_owned()],
                    allow_multiple_pads: false,
                    evidence_refs: vec!["evidence:structured-package".to_owned()],
                },
                StructuredPinInput {
                    number: "2".to_owned(),
                    name: "K".to_owned(),
                    unit: "A".to_owned(),
                    electrical_type: "passive".to_owned(),
                    direction: "bidirectional".to_owned(),
                    polarity: Polarity::Negative,
                    pad_numbers: vec!["2".to_owned()],
                    allow_multiple_pads: false,
                    evidence_refs: vec!["evidence:structured-package".to_owned()],
                },
            ],
            pads: vec![
                PadDefinition {
                    id: "pad:1".to_owned(),
                    number: "1".to_owned(),
                    shape: PadShape::Rect,
                    x_mm: -0.95,
                    y_mm: 0.0,
                    width_mm: 0.7,
                    height_mm: 1.0,
                    rotation_deg: 0.0,
                    electrical: true,
                    polarity: Polarity::Positive,
                    solder_mask_expansion_mm: 0.0,
                    paste_expansion_mm: 0.0,
                },
                PadDefinition {
                    id: "pad:2".to_owned(),
                    number: "2".to_owned(),
                    shape: PadShape::Rect,
                    x_mm: 0.95,
                    y_mm: 0.0,
                    width_mm: 0.7,
                    height_mm: 1.0,
                    rotation_deg: 0.0,
                    electrical: true,
                    polarity: Polarity::Negative,
                    solder_mask_expansion_mm: 0.0,
                    paste_expansion_mm: 0.0,
                },
            ],
            geometry: AssetGeometry {
                body_width_mm: 1.2,
                body_height_mm: 2.0,
                pitch_mm: 1.9,
                courtyard_width_mm: 3.0,
                courtyard_height_mm: 2.5,
                origin_x_mm: 0.0,
                origin_y_mm: 0.0,
                orientation_deg: 0.0,
                model3d_alignment: None,
            },
            pin_one_marked: true,
            silkscreen_clearance_mm: 0.15,
            source_evidence: vec![evidence()],
            generator_version: "0.1.0".to_owned(),
        }
    }

    #[test]
    fn generated_candidate_passes_structured_checks() {
        let candidate = generate_assets(&input()).expect("generate candidate");
        let verification = verify_library(&candidate);
        assert!(!verification.blocking, "{:?}", verification.findings);
        assert!(verification.manual_review_required);
    }

    #[test]
    fn wrong_pin_one_polarity_is_blocking() {
        let mut candidate = generate_assets(&input()).expect("generate candidate");
        candidate
            .footprint
            .footprint
            .as_mut()
            .expect("footprint")
            .pads[0]
            .polarity = Polarity::Negative;
        let verification = verify_library(&candidate);
        assert!(verification.blocking);
        assert!(
            verification
                .findings
                .iter()
                .any(|finding| finding.rule_id == "LIB.PIN_ONE_POLARITY")
        );
    }
}
