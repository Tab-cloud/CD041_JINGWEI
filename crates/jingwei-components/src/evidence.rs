use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A traceable external or generated fact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRef {
    pub id: String,
    pub kind: String,
    pub source_uri: Option<String>,
    pub publisher: String,
    pub version: String,
    pub retrieved_at: String,
    pub content_hash: String,
    pub locator: Option<String>,
    pub license: String,
    pub cache_policy: String,
}

impl EvidenceRef {
    /// Returns an error message for incomplete provenance.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("evidence id must not be empty".to_owned());
        }
        if self.publisher.trim().is_empty() || self.version.trim().is_empty() {
            return Err("evidence publisher and version are required".to_owned());
        }
        if !self.content_hash.starts_with("sha256:") || self.content_hash.len() != 71 {
            return Err("evidence contentHash must be sha256:<64 lowercase hex>".to_owned());
        }
        if !self.content_hash[7..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("evidence contentHash must use lowercase hexadecimal".to_owned());
        }
        if self.license.trim().is_empty() || self.cache_policy.trim().is_empty() {
            return Err("evidence license and cachePolicy are required".to_owned());
        }
        Ok(())
    }
}

/// Machine-readable finding severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Info,
    Warning,
    Error,
    Blocking,
}

/// Action that can resolve or investigate a finding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedAction {
    pub kind: String,
    pub description: String,
}

/// Declares what a finding rule actually checked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingCoverage {
    pub status: String,
    pub checked_aspects: Vec<String>,
    pub gaps: Vec<String>,
}

/// Stable, object-addressable validation result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub rule_id: String,
    pub rule_version: String,
    pub severity: Severity,
    pub object_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
    pub message: String,
    pub suggested_actions: Vec<SuggestedAction>,
    pub coverage: FindingCoverage,
    pub fingerprint: String,
}

impl Finding {
    /// Builds a deterministic finding fingerprint from rule, objects and message.
    #[must_use]
    pub fn new(
        rule_id: impl Into<String>,
        rule_version: impl Into<String>,
        severity: Severity,
        mut object_refs: Vec<String>,
        evidence_refs: Vec<String>,
        message: impl Into<String>,
        suggested_actions: Vec<SuggestedAction>,
        coverage: FindingCoverage,
    ) -> Self {
        let rule_id = rule_id.into();
        let rule_version = rule_version.into();
        let message = message.into();
        object_refs.sort();
        object_refs.dedup();
        let mut hasher = Sha256::new();
        for value in [&rule_id, &rule_version, &message] {
            hasher.update(value.as_bytes());
            hasher.update([0]);
        }
        for object_ref in &object_refs {
            hasher.update(object_ref.as_bytes());
            hasher.update([0]);
        }
        let fingerprint = format!("sha256:{:x}", hasher.finalize());
        Self {
            rule_id,
            rule_version,
            severity,
            object_refs,
            evidence_refs,
            message,
            suggested_actions,
            coverage,
            fingerprint,
        }
    }
}
