use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Human, agent or importer that created a baseline.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActorRef {
    pub id: String,
    pub kind: String,
}

/// Source from which an intent statement was derived.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRef {
    pub id: String,
    pub kind: String,
    pub evidence_ref: Option<String>,
}

/// Product purpose and deployment category.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductIntent {
    pub purpose: String,
    pub use_case: String,
    pub regions: Vec<String>,
}

/// Environmental operating envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentIntent {
    pub minimum_temperature_c: Option<f64>,
    pub maximum_temperature_c: Option<f64>,
    pub indoor_only: Option<bool>,
    pub condensation_allowed: Option<bool>,
}

/// Supported project risk levels.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RiskClass {
    LowVoltageGeneral,
    ElevatedReview,
    SafetyCriticalBlocked,
    Unknown,
}

/// Input power requirement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerInput {
    pub name: String,
    pub minimum_v: f64,
    pub nominal_v: f64,
    pub maximum_v: f64,
    pub reverse_polarity_protection: bool,
}

/// Generated rail requirement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerRail {
    pub name: String,
    pub nominal_v: f64,
    pub maximum_current_a: f64,
}

/// Complete power intent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerIntent {
    pub inputs: Vec<PowerInput>,
    pub rails: Vec<PowerRail>,
}

/// Required external or internal interface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceIntent {
    pub id: String,
    pub kind: String,
    pub count: u32,
    pub isolation: String,
    pub connector: Option<String>,
}

/// Mechanical size and placement bounds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MechanicalEnvelope {
    pub width_mm: Option<f64>,
    pub height_mm: Option<f64>,
    pub tolerance_mm: Option<f64>,
    pub mounting_holes: Option<u32>,
    pub maximum_component_height_mm: Option<f64>,
}

/// Target manufacturing and assembly process.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturingIntent {
    pub target_manufacturer: Option<String>,
    pub assembly_provider: Option<String>,
    pub layers: Option<u8>,
    pub material: Option<String>,
    pub assembly_sides: Option<String>,
    pub minimum_passive_package: Option<String>,
}

/// A hard/soft statement, assumption or preference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementStatement {
    pub id: String,
    pub statement: String,
    pub source_refs: Vec<String>,
    pub evidence_refs: Vec<String>,
}

/// An unresolved fact with an explicit deadline stage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnknownRequirement {
    pub id: String,
    pub field: String,
    pub why_it_matters: String,
    pub blocking_before_stage: String,
}

/// Contradictory intent facts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentConflict {
    pub id: String,
    pub field: String,
    pub alternatives: Vec<String>,
    pub source_refs: Vec<String>,
}

/// Audit metadata for an intent baseline candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineMetadata {
    pub actor: ActorRef,
    pub recorded_at: String,
    pub source_refs: Vec<SourceRef>,
    pub evidence_refs: Vec<String>,
}

/// Typed requirements contract. Natural language is not a production fact here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignIntent {
    pub id: String,
    pub schema_version: String,
    pub product: ProductIntent,
    pub environment: EnvironmentIntent,
    pub risk: RiskClass,
    pub power: PowerIntent,
    pub interfaces: Vec<InterfaceIntent>,
    pub mechanical: MechanicalEnvelope,
    pub manufacturing: ManufacturingIntent,
    pub component_preferences: Vec<RequirementStatement>,
    pub forbidden_parts: Vec<String>,
    pub hard_constraints: Vec<RequirementStatement>,
    pub soft_preferences: Vec<RequirementStatement>,
    pub assumptions: Vec<RequirementStatement>,
    pub unknowns: Vec<UnknownRequirement>,
    pub conflicts: Vec<IntentConflict>,
    pub baseline: BaselineMetadata,
}

/// A question that must be surfaced instead of inventing a default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentQuestion {
    pub id: String,
    pub field: String,
    pub question: String,
    pub why_it_matters: String,
    pub blocking_before_stage: String,
}

/// Candidate revision returned for later kernel commit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignIntentRevision {
    pub candidate_id: String,
    pub intent: DesignIntent,
    pub questions: Vec<IntentQuestion>,
    pub baseline_allowed: bool,
}

/// Intent validation error.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DesignIntentError {
    #[error("intent identifier and schema version are required")]
    MissingIdentity,
    #[error("intent has duplicate identifier: {0}")]
    DuplicateIdentifier(String),
    #[error("invalid power range for input: {0}")]
    InvalidPowerRange(String),
}

impl DesignIntent {
    /// Validates structural invariants and derives blocking questions.
    pub fn compile_candidate(self) -> Result<DesignIntentRevision, DesignIntentError> {
        if self.id.trim().is_empty() || self.schema_version.trim().is_empty() {
            return Err(DesignIntentError::MissingIdentity);
        }
        for input in &self.power.inputs {
            if input.minimum_v > input.nominal_v || input.nominal_v > input.maximum_v {
                return Err(DesignIntentError::InvalidPowerRange(input.name.clone()));
            }
        }
        let mut seen = BTreeSet::new();
        for id in self
            .interfaces
            .iter()
            .map(|value| value.id.as_str())
            .chain(self.hard_constraints.iter().map(|value| value.id.as_str()))
            .chain(self.soft_preferences.iter().map(|value| value.id.as_str()))
            .chain(self.assumptions.iter().map(|value| value.id.as_str()))
            .chain(self.unknowns.iter().map(|value| value.id.as_str()))
            .chain(self.conflicts.iter().map(|value| value.id.as_str()))
        {
            if !seen.insert(id) {
                return Err(DesignIntentError::DuplicateIdentifier(id.to_owned()));
            }
        }

        let mut questions: Vec<IntentQuestion> = self
            .unknowns
            .iter()
            .map(|unknown| IntentQuestion {
                id: format!("question:{}", unknown.id),
                field: unknown.field.clone(),
                question: format!("Please provide a confirmed value for {}.", unknown.field),
                why_it_matters: unknown.why_it_matters.clone(),
                blocking_before_stage: unknown.blocking_before_stage.clone(),
            })
            .collect();
        questions.extend(self.conflicts.iter().map(|conflict| IntentQuestion {
            id: format!("question:{}", conflict.id),
            field: conflict.field.clone(),
            question: format!(
                "Resolve the conflicting values for {}: {}.",
                conflict.field,
                conflict.alternatives.join(", ")
            ),
            why_it_matters: "A requirement baseline cannot contain contradictory facts.".to_owned(),
            blocking_before_stage: "intent-baselined".to_owned(),
        }));
        if self.power.inputs.is_empty() {
            questions.push(IntentQuestion {
                id: "question:power.input".to_owned(),
                field: "power.inputs".to_owned(),
                question: "What input voltage range and source are required?".to_owned(),
                why_it_matters: "Input power changes topology, ratings and safety classification."
                    .to_owned(),
                blocking_before_stage: "architecture-selected".to_owned(),
            });
        }
        if self.mechanical.width_mm.is_none() || self.mechanical.height_mm.is_none() {
            questions.push(IntentQuestion {
                id: "question:mechanical.envelope".to_owned(),
                field: "mechanical".to_owned(),
                question: "What are the confirmed board width and height limits?".to_owned(),
                why_it_matters: "The envelope constrains placement and connector selection."
                    .to_owned(),
                blocking_before_stage: "pcb-constrained".to_owned(),
            });
        }
        if self.manufacturing.target_manufacturer.is_none() {
            questions.push(IntentQuestion {
                id: "question:manufacturing.target".to_owned(),
                field: "manufacturing.targetManufacturer".to_owned(),
                question: "Which manufacturer profile will govern fabrication and assembly?"
                    .to_owned(),
                why_it_matters: "Manufacturing limits and output formats are profile-specific."
                    .to_owned(),
                blocking_before_stage: "pcb-constrained".to_owned(),
            });
        }
        questions.sort_by(|left, right| left.id.cmp(&right.id));
        let baseline_allowed = questions
            .iter()
            .all(|question| question.blocking_before_stage != "intent-baselined");
        Ok(DesignIntentRevision {
            candidate_id: format!("intent-candidate:{}", self.id),
            intent: self,
            questions,
            baseline_allowed,
        })
    }
}

/// Deterministic risk-classification output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskReport {
    pub class: RiskClass,
    pub production_release_allowed: bool,
    pub reasons: Vec<String>,
    pub required_reviews: Vec<String>,
}

/// Classifies supported and blocked designs from structured facts only.
#[must_use]
pub fn classify_risk(intent: &DesignIntent) -> RiskReport {
    let mut reasons = Vec::new();
    let maximum_input = intent
        .power
        .inputs
        .iter()
        .map(|input| input.maximum_v)
        .fold(0.0, f64::max);
    if maximum_input > 24.0 {
        reasons.push(format!(
            "maximum input voltage {maximum_input} V exceeds the MVP 24 V boundary"
        ));
    }
    if intent
        .interfaces
        .iter()
        .any(|interface| interface.isolation.eq_ignore_ascii_case("required"))
    {
        reasons.push("an isolated interface requires a dedicated safety review".to_owned());
    }
    let safety_keywords = ["medical", "life support", "mains", "automotive safety"];
    let lower_use_case = intent.product.use_case.to_ascii_lowercase();
    if safety_keywords
        .iter()
        .any(|keyword| lower_use_case.contains(keyword))
    {
        reasons.push("the declared use case is safety critical or outside the MVP".to_owned());
    }

    if reasons.is_empty() {
        RiskReport {
            class: RiskClass::LowVoltageGeneral,
            production_release_allowed: false,
            reasons: vec!["within the low-voltage logical-design scope".to_owned()],
            required_reviews: vec!["hardware-engineer".to_owned()],
        }
    } else {
        RiskReport {
            class: RiskClass::SafetyCriticalBlocked,
            production_release_allowed: false,
            reasons,
            required_reviews: vec![
                "hardware-engineer".to_owned(),
                "safety-specialist".to_owned(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_golden_intent_compiles_without_blocking_questions() {
        let fixture = include_str!("../../../testdata/logical/golden/design-intent.json");
        let intent: DesignIntent = serde_json::from_str(fixture).expect("valid intent fixture");
        let revision = intent.compile_candidate().expect("compile candidate");
        assert!(revision.questions.is_empty());
        assert!(revision.baseline_allowed);
        assert_eq!(
            classify_risk(&revision.intent).class,
            RiskClass::LowVoltageGeneral
        );
    }

    #[test]
    fn missing_power_is_a_directed_question() {
        let fixture = include_str!("../../../testdata/logical/golden/design-intent.json");
        let mut intent: DesignIntent = serde_json::from_str(fixture).expect("valid intent fixture");
        intent.power.inputs.clear();
        let revision = intent.compile_candidate().expect("compile candidate");
        assert!(
            revision
                .questions
                .iter()
                .any(|question| question.field == "power.inputs")
        );
    }
}
