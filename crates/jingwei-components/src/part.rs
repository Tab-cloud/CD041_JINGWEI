use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{EvidenceRef, Finding, FindingCoverage, Severity, SuggestedAction};

/// Exact, normalized manufacturer part number.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExactMpn(String);

impl ExactMpn {
    /// Creates an exact MPN. Wildcards and surrounding whitespace are rejected.
    pub fn parse(value: impl Into<String>) -> Result<Self, PartSearchError> {
        let value = value.into();
        if value.is_empty()
            || value.trim() != value
            || value.contains('*')
            || value.contains('?')
            || value.len() > 128
        {
            return Err(PartSearchError::InvalidMpn(value));
        }
        Ok(Self(value))
    }

    /// Returns the normalized exact value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Unit-bearing parameter value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ParameterValue {
    Number { value: f64, unit: String },
    Text { value: String },
    Boolean { value: bool },
}

/// Hard-constraint comparison operator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintOperator {
    Equals,
    GreaterThanOrEqual,
    LessThanOrEqual,
    InSet,
}

/// Hard candidate filter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Constraint {
    pub id: String,
    pub field: String,
    pub operator: ConstraintOperator,
    pub expected: Vec<ParameterValue>,
    pub evidence_refs: Vec<String>,
}

/// Weighted candidate preference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightedPreference {
    pub id: String,
    pub field: String,
    pub preferred: ParameterValue,
    pub weight: f64,
    pub lower_is_better: bool,
}

/// Package identity used during part and library matching.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRef {
    pub name: String,
    pub pin_count: u16,
}

/// Component lifecycle fact from a source snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lifecycle {
    Active,
    Nrnd,
    Eol,
    Unknown,
}

/// Price and stock at a particular time and quantity.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupplierOffer {
    pub supplier: String,
    pub supplier_sku: String,
    pub region: String,
    pub quantity: u32,
    pub unit_price: f64,
    pub currency: String,
    pub stock: u64,
    pub lead_time_days: Option<u32>,
    pub captured_at: String,
    pub ttl_hours: u32,
}

/// Immutable supplier/catalog evidence envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartSourceSnapshot {
    pub id: String,
    pub source_name: String,
    pub source_version: String,
    pub captured_at: String,
    pub age_hours: u32,
    pub license: String,
    pub cache_policy: String,
    pub content_hash: String,
    pub evidence: EvidenceRef,
}

/// Normalized catalog record returned by a source adapter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartCandidate {
    pub id: String,
    pub mpn: ExactMpn,
    pub manufacturer: String,
    pub parameters: BTreeMap<String, ParameterValue>,
    pub package: PackageRef,
    pub lifecycle: Lifecycle,
    pub source_snapshot: PartSourceSnapshot,
    pub offers: Vec<SupplierOffer>,
    pub evidence_refs: Vec<String>,
    pub hard_failures: Vec<Finding>,
    pub score_breakdown: Vec<ScoreComponent>,
}

/// Requested component function and selection policy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartRequirement {
    pub id: String,
    pub function: String,
    pub hard_constraints: Vec<Constraint>,
    pub soft_preferences: Vec<WeightedPreference>,
    pub quantity_tiers: Vec<u32>,
    pub allowed_packages: Vec<PackageRef>,
    pub forbidden_mpns: Vec<ExactMpn>,
    pub maximum_snapshot_age_hours: u32,
}

/// Adapter boundary for authorized sources and deterministic local fixtures.
pub trait PartSourceAdapter {
    /// Adapter identifier recorded in provenance.
    fn adapter_id(&self) -> &str;

    /// Returns normalized catalog candidates without performing selection.
    fn search(&self, requirement: &PartRequirement) -> Result<Vec<PartCandidate>, PartSearchError>;
}

/// Reproducible, offline catalog adapter.
#[derive(Clone, Debug)]
pub struct LocalCatalogAdapter {
    adapter_id: String,
    candidates: Vec<PartCandidate>,
}

impl LocalCatalogAdapter {
    /// Constructs an offline fixture adapter.
    #[must_use]
    pub fn new(adapter_id: impl Into<String>, candidates: Vec<PartCandidate>) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            candidates,
        }
    }
}

impl PartSourceAdapter for LocalCatalogAdapter {
    fn adapter_id(&self) -> &str {
        &self.adapter_id
    }

    fn search(&self, requirement: &PartRequirement) -> Result<Vec<PartCandidate>, PartSearchError> {
        if requirement.quantity_tiers.is_empty() {
            return Err(PartSearchError::MissingQuantityTier);
        }
        Ok(self.candidates.clone())
    }
}

/// Per-preference score contribution.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreComponent {
    pub preference_id: String,
    pub field: String,
    pub normalized_score: f64,
    pub weight: f64,
    pub weighted_score: f64,
    pub explanation: String,
}

/// Evaluated candidate with hard-filter status and total soft score.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateEvaluation {
    pub candidate: PartCandidate,
    pub eligible: bool,
    pub total_score: f64,
    pub risks: Vec<String>,
}

/// Auditable component selection candidate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartDecision {
    pub id: String,
    pub requirement_id: String,
    pub selected: PartCandidate,
    pub alternates: Vec<PartCandidate>,
    pub rejected: Vec<CandidateEvaluation>,
    pub reasons: Vec<String>,
    pub invalidation_triggers: Vec<String>,
    pub candidate_operation: String,
}

/// Validated substitution relationship.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovedSubstitution {
    pub primary_mpn: ExactMpn,
    pub substitute_mpn: ExactMpn,
    pub pinout_exact: bool,
    pub footprint_compatible: bool,
    pub electrical_findings: Vec<Finding>,
    pub firmware_impact: String,
    pub assembly_variant_allowed: bool,
}

/// Part-source error.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PartSearchError {
    #[error("invalid exact MPN: {0}")]
    InvalidMpn(String),
    #[error("part requirement must include at least one quantity tier")]
    MissingQuantityTier,
    #[error("part source is unavailable without configured authorization: {0}")]
    AuthorizationRequired(String),
}

/// Part-selection error.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PartSelectionError {
    #[error("no candidate satisfies every hard constraint")]
    NoEligibleCandidate,
    #[error("candidate set is empty")]
    EmptyCandidateSet,
}

/// Applies hard filters first and then computes auditable soft scores.
#[must_use]
pub fn compare_candidates(
    requirement: &PartRequirement,
    candidates: Vec<PartCandidate>,
) -> Vec<CandidateEvaluation> {
    let forbidden: BTreeSet<&ExactMpn> = requirement.forbidden_mpns.iter().collect();
    let mut evaluations = Vec::with_capacity(candidates.len());
    for mut candidate in candidates {
        let mut failures = Vec::new();
        if forbidden.contains(&candidate.mpn) {
            failures.push(hard_failure(
                "CMP.FORBIDDEN_MPN",
                &candidate,
                "the exact MPN is forbidden by the DesignIntent",
            ));
        }
        if candidate.source_snapshot.age_hours > requirement.maximum_snapshot_age_hours {
            failures.push(hard_failure(
                "CMP.STALE_SNAPSHOT",
                &candidate,
                "the supplier snapshot is older than the permitted TTL",
            ));
        }
        if !requirement.allowed_packages.is_empty()
            && !requirement.allowed_packages.contains(&candidate.package)
        {
            failures.push(hard_failure(
                "CMP.PACKAGE_MISMATCH",
                &candidate,
                "the package or pin count is not allowed",
            ));
        }
        for constraint in &requirement.hard_constraints {
            let actual = candidate.parameters.get(&constraint.field);
            if !constraint_matches(actual, constraint) {
                failures.push(hard_failure(
                    "CMP.HARD_CONSTRAINT",
                    &candidate,
                    &format!("hard constraint {} failed", constraint.id),
                ));
            }
        }
        candidate.hard_failures = failures;
        let score_breakdown = requirement
            .soft_preferences
            .iter()
            .map(|preference| score_preference(&candidate, preference))
            .collect::<Vec<_>>();
        let total_score = score_breakdown
            .iter()
            .map(|component| component.weighted_score)
            .sum();
        candidate.score_breakdown = score_breakdown;
        let mut risks = Vec::new();
        match candidate.lifecycle {
            Lifecycle::Nrnd => risks.push("NRND lifecycle".to_owned()),
            Lifecycle::Eol => risks.push("EOL lifecycle".to_owned()),
            Lifecycle::Unknown => risks.push("unknown lifecycle".to_owned()),
            Lifecycle::Active => {}
        }
        if candidate.offers.len() < 2 {
            risks.push("single-source supply snapshot".to_owned());
        }
        if candidate
            .offers
            .iter()
            .filter_map(|offer| offer.lead_time_days)
            .any(|days| days > 90)
        {
            risks.push("lead time exceeds 90 days".to_owned());
        }
        evaluations.push(CandidateEvaluation {
            eligible: candidate.hard_failures.is_empty(),
            candidate,
            total_score,
            risks,
        });
    }
    evaluations.sort_by(|left, right| {
        right
            .eligible
            .cmp(&left.eligible)
            .then_with(|| {
                right
                    .total_score
                    .partial_cmp(&left.total_score)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| left.candidate.mpn.cmp(&right.candidate.mpn))
    });
    evaluations
}

/// Selects the best eligible exact MPN without allowing score to override a hard failure.
pub fn select_part(
    requirement: &PartRequirement,
    evaluations: Vec<CandidateEvaluation>,
) -> Result<PartDecision, PartSelectionError> {
    if evaluations.is_empty() {
        return Err(PartSelectionError::EmptyCandidateSet);
    }
    let mut eligible = evaluations
        .iter()
        .filter(|evaluation| evaluation.eligible)
        .cloned()
        .collect::<Vec<_>>();
    if eligible.is_empty() {
        return Err(PartSelectionError::NoEligibleCandidate);
    }
    eligible.sort_by(|left, right| {
        right
            .total_score
            .partial_cmp(&left.total_score)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.candidate.mpn.cmp(&right.candidate.mpn))
    });
    let selected_evaluation = eligible.remove(0);
    let reasons = vec![
        "all hard constraints passed before soft scoring".to_owned(),
        format!(
            "selected highest eligible soft score {:.6}",
            selected_evaluation.total_score
        ),
        format!(
            "source snapshot {} at {}",
            selected_evaluation.candidate.source_snapshot.source_name,
            selected_evaluation.candidate.source_snapshot.captured_at
        ),
    ];
    let rejected = evaluations
        .into_iter()
        .filter(|evaluation| !evaluation.eligible)
        .collect();
    Ok(PartDecision {
        id: format!(
            "part-decision:{}:{}",
            requirement.id,
            selected_evaluation.candidate.mpn.as_str()
        ),
        requirement_id: requirement.id.clone(),
        selected: selected_evaluation.candidate,
        alternates: eligible
            .into_iter()
            .map(|evaluation| evaluation.candidate)
            .collect(),
        rejected,
        reasons,
        invalidation_triggers: vec![
            "hard constraint changed".to_owned(),
            "package or pin map changed".to_owned(),
            "source snapshot TTL expired".to_owned(),
            "lifecycle changed".to_owned(),
        ],
        candidate_operation: "logical.part.select".to_owned(),
    })
}

/// Re-runs package, pinout and hard electrical equivalence for a substitute.
#[must_use]
pub fn validate_substitution(
    requirement: &PartRequirement,
    primary: &PartCandidate,
    substitute: &PartCandidate,
    pinout_exact: bool,
) -> ApprovedSubstitution {
    let evaluation = compare_candidates(requirement, vec![substitute.clone()]);
    let hard_findings = evaluation
        .first()
        .map_or_else(Vec::new, |value| value.candidate.hard_failures.clone());
    let footprint_compatible = primary.package == substitute.package;
    let assembly_variant_allowed = pinout_exact && footprint_compatible && hard_findings.is_empty();
    ApprovedSubstitution {
        primary_mpn: primary.mpn.clone(),
        substitute_mpn: substitute.mpn.clone(),
        pinout_exact,
        footprint_compatible,
        electrical_findings: hard_findings,
        firmware_impact: if pinout_exact {
            "none".to_owned()
        } else {
            "unknown".to_owned()
        },
        assembly_variant_allowed,
    }
}

fn hard_failure(rule_id: &str, candidate: &PartCandidate, message: &str) -> Finding {
    Finding::new(
        rule_id,
        "1.0.0",
        Severity::Blocking,
        vec![candidate.id.clone()],
        candidate.evidence_refs.clone(),
        message,
        vec![SuggestedAction {
            kind: "use-alternative".to_owned(),
            description: "Select an exact-MPN candidate that satisfies the hard constraint."
                .to_owned(),
        }],
        FindingCoverage {
            status: "implemented".to_owned(),
            checked_aspects: vec!["exact MPN normalized catalog fields".to_owned()],
            gaps: Vec::new(),
        },
    )
}

fn constraint_matches(actual: Option<&ParameterValue>, constraint: &Constraint) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    match constraint.operator {
        ConstraintOperator::Equals | ConstraintOperator::InSet => constraint
            .expected
            .iter()
            .any(|expected| values_equal(actual, expected)),
        ConstraintOperator::GreaterThanOrEqual => constraint
            .expected
            .first()
            .is_some_and(|expected| numeric_compare(actual, expected, |a, b| a >= b)),
        ConstraintOperator::LessThanOrEqual => constraint
            .expected
            .first()
            .is_some_and(|expected| numeric_compare(actual, expected, |a, b| a <= b)),
    }
}

fn values_equal(left: &ParameterValue, right: &ParameterValue) -> bool {
    match (left, right) {
        (
            ParameterValue::Number {
                value: left_value,
                unit: left_unit,
            },
            ParameterValue::Number {
                value: right_value,
                unit: right_unit,
            },
        ) => left_unit == right_unit && (left_value - right_value).abs() < f64::EPSILON,
        (ParameterValue::Text { value: left }, ParameterValue::Text { value: right }) => {
            left.eq_ignore_ascii_case(right)
        }
        (ParameterValue::Boolean { value: left }, ParameterValue::Boolean { value: right }) => {
            left == right
        }
        _ => false,
    }
}

fn numeric_compare(
    left: &ParameterValue,
    right: &ParameterValue,
    predicate: impl FnOnce(f64, f64) -> bool,
) -> bool {
    match (left, right) {
        (
            ParameterValue::Number {
                value: left_value,
                unit: left_unit,
            },
            ParameterValue::Number {
                value: right_value,
                unit: right_unit,
            },
        ) if left_unit == right_unit => predicate(*left_value, *right_value),
        _ => false,
    }
}

fn score_preference(candidate: &PartCandidate, preference: &WeightedPreference) -> ScoreComponent {
    let actual = candidate.parameters.get(&preference.field);
    let (normalized_score, explanation) = match (actual, &preference.preferred) {
        (
            Some(ParameterValue::Number {
                value: actual_value,
                unit: actual_unit,
            }),
            ParameterValue::Number {
                value: preferred_value,
                unit: preferred_unit,
            },
        ) if actual_unit == preferred_unit => {
            let scale = preferred_value.abs().max(1.0);
            let distance = (actual_value - preferred_value).abs() / scale;
            let score = if preference.lower_is_better {
                1.0 / (1.0 + actual_value.max(0.0) / scale)
            } else {
                1.0 / (1.0 + distance)
            };
            (
                score,
                format!("numeric distance {:.6} {}", distance, actual_unit),
            )
        }
        (Some(actual), preferred) if values_equal(actual, preferred) => {
            (1.0, "exact preference match".to_owned())
        }
        (Some(_), _) => (0.0, "preference did not match".to_owned()),
        (None, _) => (0.0, "candidate did not provide this field".to_owned()),
    };
    ScoreComponent {
        preference_id: preference.id.clone(),
        field: preference.field.clone(),
        normalized_score,
        weight: preference.weight,
        weighted_score: normalized_score * preference.weight,
        explanation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> PartSourceSnapshot {
        PartSourceSnapshot {
            id: "snapshot:catalog-v1".to_owned(),
            source_name: "public-synthetic-catalog".to_owned(),
            source_version: "1".to_owned(),
            captured_at: "2026-08-19T00:00:00Z".to_owned(),
            age_hours: 0,
            license: "CC0-1.0".to_owned(),
            cache_policy: "redistributable".to_owned(),
            content_hash: format!("sha256:{}", "0".repeat(64)),
            evidence: EvidenceRef {
                id: "evidence:catalog-v1".to_owned(),
                kind: "api-snapshot".to_owned(),
                source_uri: None,
                publisher: "Jingwei test fixtures".to_owned(),
                version: "1".to_owned(),
                retrieved_at: "2026-08-19T00:00:00Z".to_owned(),
                content_hash: format!("sha256:{}", "0".repeat(64)),
                locator: Some("record 1".to_owned()),
                license: "CC0-1.0".to_owned(),
                cache_policy: "redistributable".to_owned(),
            },
        }
    }

    fn candidate(mpn: &str, voltage: f64, cost: f64) -> PartCandidate {
        PartCandidate {
            id: format!("part:{mpn}"),
            mpn: ExactMpn::parse(mpn).expect("valid test MPN"),
            manufacturer: "Jingwei Synthetic Semiconductor".to_owned(),
            parameters: BTreeMap::from([
                (
                    "maximum_input_voltage".to_owned(),
                    ParameterValue::Number {
                        value: voltage,
                        unit: "V".to_owned(),
                    },
                ),
                (
                    "unit_cost".to_owned(),
                    ParameterValue::Number {
                        value: cost,
                        unit: "CNY".to_owned(),
                    },
                ),
            ]),
            package: PackageRef {
                name: "SOIC-8".to_owned(),
                pin_count: 8,
            },
            lifecycle: Lifecycle::Active,
            source_snapshot: snapshot(),
            offers: Vec::new(),
            evidence_refs: vec!["evidence:catalog-v1".to_owned()],
            hard_failures: Vec::new(),
            score_breakdown: Vec::new(),
        }
    }

    fn requirement() -> PartRequirement {
        PartRequirement {
            id: "part-requirement:transceiver".to_owned(),
            function: "RS-485 transceiver".to_owned(),
            hard_constraints: vec![Constraint {
                id: "constraint:vin".to_owned(),
                field: "maximum_input_voltage".to_owned(),
                operator: ConstraintOperator::GreaterThanOrEqual,
                expected: vec![ParameterValue::Number {
                    value: 24.0,
                    unit: "V".to_owned(),
                }],
                evidence_refs: Vec::new(),
            }],
            soft_preferences: vec![WeightedPreference {
                id: "preference:cost".to_owned(),
                field: "unit_cost".to_owned(),
                preferred: ParameterValue::Number {
                    value: 1.0,
                    unit: "CNY".to_owned(),
                },
                weight: 1.0,
                lower_is_better: true,
            }],
            quantity_tiers: vec![10, 5000],
            allowed_packages: vec![PackageRef {
                name: "SOIC-8".to_owned(),
                pin_count: 8,
            }],
            forbidden_mpns: Vec::new(),
            maximum_snapshot_age_hours: 24,
        }
    }

    #[test]
    fn hard_failure_cannot_be_outscored() {
        let evaluations = compare_candidates(
            &requirement(),
            vec![
                candidate("JW485-CHEAP", 12.0, 0.1),
                candidate("JW485-OK", 30.0, 2.0),
            ],
        );
        let decision = select_part(&requirement(), evaluations).expect("one eligible candidate");
        assert_eq!(decision.selected.mpn.as_str(), "JW485-OK");
        assert_eq!(decision.rejected.len(), 1);
    }

    #[test]
    fn exact_mpn_rejects_wildcards() {
        assert!(ExactMpn::parse("JW485-*").is_err());
    }
}
