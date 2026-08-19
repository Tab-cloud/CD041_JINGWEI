//! Native deterministic schematic ERC with explicit rule coverage.

use std::collections::BTreeMap;

use jingwei_components::{Finding, Severity};
use jingwei_jdp::{ValidationFinding, ValidationOutput};
use jingwei_jir::EntityId;
use jingwei_schematic::SchematicGraph;
use serde::{Deserialize, Serialize};

mod rules;

pub use rules::rule_manifest;

/// Library binding status supplied by the verified library domain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LibraryBindingStatus {
    VerifiedExact,
    Missing,
    MpnMismatch,
    PackageMismatch,
    PinPadInvalid,
    Unverified,
}

/// Inputs in addition to the logical graph.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErcContext {
    pub library_bindings: BTreeMap<EntityId, LibraryBindingStatus>,
    pub project_risk_profile: String,
}

/// One declared deterministic rule or explicit coverage gap.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleCoverage {
    pub rule_id: String,
    pub version: String,
    pub title: String,
    pub implemented: bool,
    pub checked_aspects: Vec<String>,
    pub gaps: Vec<String>,
}

/// Native ERC result bound to a graph revision supplied by the caller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErcRun {
    pub provider: String,
    pub provider_version: String,
    pub findings: Vec<Finding>,
    pub coverage: Vec<RuleCoverage>,
    pub error_count: usize,
    pub warning_count: usize,
    pub passed: bool,
}

impl ErcRun {
    /// Converts detailed domain findings to the current core JDP summary shape.
    #[must_use]
    pub fn to_jdp_validation(&self) -> ValidationOutput {
        ValidationOutput {
            valid: self.passed,
            findings: self
                .findings
                .iter()
                .map(|finding| ValidationFinding {
                    code: finding.rule_id.clone(),
                    severity: match finding.severity {
                        Severity::Blocking => "blocking",
                        Severity::Error => "error",
                        Severity::Warning => "warning",
                        Severity::Info => "info",
                    }
                    .to_owned(),
                    summary: finding.message.clone(),
                    object_refs: finding.object_refs.clone(),
                    fingerprint: finding.fingerprint.clone(),
                })
                .collect(),
            providers_run: vec![format!("{}@{}", self.provider, self.provider_version)],
        }
    }
}

/// Runs every implemented rule; error-free does not erase declared coverage gaps.
#[must_use]
pub fn run_native_erc(graph: &SchematicGraph, context: &ErcContext) -> ErcRun {
    let mut findings = rules::run_all(graph, context);
    findings.sort_by(|left, right| {
        left.rule_id
            .cmp(&right.rule_id)
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
    let error_count = findings
        .iter()
        .filter(|finding| matches!(finding.severity, Severity::Error | Severity::Blocking))
        .count();
    let warning_count = findings
        .iter()
        .filter(|finding| finding.severity == Severity::Warning)
        .count();
    ErcRun {
        provider: "jingwei-native-erc".to_owned(),
        provider_version: env!("CARGO_PKG_VERSION").to_owned(),
        findings,
        coverage: rule_manifest(),
        error_count,
        warning_count,
        passed: error_count == 0,
    }
}
