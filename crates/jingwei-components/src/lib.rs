//! Deterministic requirement, evidence and exact-MPN component selection.
//!
//! This crate deliberately returns domain candidates and decisions. It does not
//! create JIR revisions; the kernel transaction service remains the sole writer.

mod evidence;
mod intent;
mod part;

pub use evidence::{EvidenceRef, Finding, FindingCoverage, Severity, SuggestedAction};
pub use intent::{
    ActorRef, BaselineMetadata, DesignIntent, DesignIntentError, DesignIntentRevision,
    EnvironmentIntent, IntentConflict, IntentQuestion, InterfaceIntent, ManufacturingIntent,
    MechanicalEnvelope, PowerInput, PowerIntent, PowerRail, ProductIntent, RequirementStatement,
    RiskClass, RiskReport, SourceRef, UnknownRequirement, classify_risk,
};
pub use part::{
    ApprovedSubstitution, CandidateEvaluation, Constraint, ConstraintOperator, ExactMpn, Lifecycle,
    LocalCatalogAdapter, PackageRef, ParameterValue, PartCandidate, PartDecision, PartRequirement,
    PartSearchError, PartSelectionError, PartSourceAdapter, PartSourceSnapshot, ScoreComponent,
    SupplierOffer, WeightedPreference, compare_candidates, select_part, validate_substitution,
};

/// Domain schema version implemented by this crate.
pub const LOGICAL_SCHEMA_VERSION: &str = "logical-domain/0.1";
