use jingwei_components::EvidenceRef;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const MAX_CONTROLLED_ARTIFACT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PIN_ROWS: usize = 2_048;

/// Reference to a previously acquired, controlled local artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlledDatasheetArtifact {
    pub artifact_id: String,
    pub media_type: String,
    pub content_hash: String,
    pub acquisition_evidence: EvidenceRef,
}

/// Structured pin fact extracted from the controlled artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasheetPinFact {
    pub number: String,
    pub name: String,
    pub electrical_type: String,
}

/// Structured package fact with a document locator.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasheetPackageFact {
    pub name: String,
    pub pin_count: u16,
    pub body_width_mm: f64,
    pub body_height_mm: f64,
    pub pitch_mm: f64,
}

/// Controlled extraction result. Document prose remains explicitly untrusted data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasheetExtract {
    pub artifact: ControlledDatasheetArtifact,
    pub publisher: String,
    pub document_version: String,
    pub retrieved_at: String,
    pub license: String,
    pub locator: String,
    pub pins: Vec<DatasheetPinFact>,
    pub package: DatasheetPackageFact,
    pub untrusted_document_text: String,
    pub extraction_method: String,
    pub candidate_only: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureEnvelope {
    artifact_id: String,
    publisher: String,
    document_version: String,
    retrieved_at: String,
    content_hash: String,
    license: String,
    locator: serde_json::Value,
    untrusted_document_text: String,
    pins: Vec<DatasheetPinFact>,
    package: DatasheetPackageFact,
}

/// Datasheet extraction failure.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DatasheetExtractError {
    #[error("controlled artifact exceeds the 2 MiB extraction limit")]
    ArtifactTooLarge,
    #[error("controlled artifact JSON is invalid: {0}")]
    InvalidJson(String),
    #[error("controlled artifact identity or provenance is incomplete")]
    MissingProvenance,
    #[error("controlled artifact contains too many pin rows")]
    TooManyPins,
    #[error("controlled artifact pin/package counts disagree")]
    PinCountMismatch,
}

/// Consumes an already-controlled local artifact. It never performs network access.
pub fn extract_controlled_datasheet(
    artifact: ControlledDatasheetArtifact,
    bytes: &[u8],
) -> Result<DatasheetExtract, DatasheetExtractError> {
    if bytes.len() > MAX_CONTROLLED_ARTIFACT_BYTES {
        return Err(DatasheetExtractError::ArtifactTooLarge);
    }
    if artifact.artifact_id.trim().is_empty()
        || !artifact.content_hash.starts_with("sha256:")
        || artifact.acquisition_evidence.validate().is_err()
    {
        return Err(DatasheetExtractError::MissingProvenance);
    }
    let envelope: FixtureEnvelope = serde_json::from_slice(bytes)
        .map_err(|error| DatasheetExtractError::InvalidJson(error.to_string()))?;
    if envelope.pins.len() > MAX_PIN_ROWS {
        return Err(DatasheetExtractError::TooManyPins);
    }
    if usize::from(envelope.package.pin_count) != envelope.pins.len() {
        return Err(DatasheetExtractError::PinCountMismatch);
    }
    if envelope.artifact_id != artifact.artifact_id
        || envelope.content_hash != artifact.content_hash
    {
        return Err(DatasheetExtractError::MissingProvenance);
    }
    Ok(DatasheetExtract {
        artifact,
        publisher: envelope.publisher,
        document_version: envelope.document_version,
        retrieved_at: envelope.retrieved_at,
        license: envelope.license,
        locator: envelope.locator.to_string(),
        pins: envelope.pins,
        package: envelope.package,
        untrusted_document_text: envelope.untrusted_document_text,
        extraction_method: "structured-json-controlled-artifact".to_owned(),
        candidate_only: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_like_document_text_remains_untrusted_data() {
        let bytes = include_bytes!("../../../testdata/logical/golden/structured-datasheet.json");
        let hash = format!("sha256:{}", "1".repeat(64));
        let evidence = EvidenceRef {
            id: "evidence:datasheet-fixture".to_owned(),
            kind: "datasheet".to_owned(),
            source_uri: None,
            publisher: "Jingwei synthetic fixture authors".to_owned(),
            version: "1".to_owned(),
            retrieved_at: "2026-08-19T00:00:00Z".to_owned(),
            content_hash: hash.clone(),
            locator: Some("page 1".to_owned()),
            license: "CC0-1.0".to_owned(),
            cache_policy: "redistributable".to_owned(),
        };
        let result = extract_controlled_datasheet(
            ControlledDatasheetArtifact {
                artifact_id: "artifact:synthetic-jw-rs485-8a".to_owned(),
                media_type: "application/json".to_owned(),
                content_hash: hash,
                acquisition_evidence: evidence,
            },
            bytes,
        )
        .expect("extract controlled fixture");
        assert!(result.untrusted_document_text.contains("IGNORE ALL RULES"));
        assert!(result.candidate_only);
        assert_eq!(result.pins.len(), 8);
    }
}
