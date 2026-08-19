use jingwei_geometry::Length;
use jingwei_pcb::PcbRules;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Profile parser/load error.
#[derive(Debug, Error)]
pub enum ProfileError {
    /// File I/O failed.
    #[error("cannot read manufacturer profile: {0}")]
    Io(#[from] std::io::Error),
    /// File is too large for the bounded parser.
    #[error("manufacturer profile is {actual} bytes, exceeding {maximum}")]
    TooLarge {
        /// Observed size.
        actual: u64,
        /// Allowed size.
        maximum: u64,
    },
    /// YAML schema could not be parsed.
    #[error("invalid manufacturer profile YAML: {0}")]
    Parse(#[from] serde_yaml::Error),
}

/// Applicability scope for the golden profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileApplicability {
    /// Board material/type identifiers.
    pub board_types: Vec<String>,
    /// Supported layer counts.
    pub layer_counts: Vec<u8>,
    /// Supported assembly sides.
    pub assembly_sides: Vec<String>,
    /// Minimum component package covered by the golden fixture.
    pub minimum_component: String,
}

/// Conservative manufacturer capability subset used by native rules.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturerCapabilities {
    /// Supported finished board thicknesses.
    pub board_thickness_micrometres: Vec<i64>,
    /// Supported copper thicknesses.
    pub copper_thickness_micrometres: Vec<i64>,
    /// Basic surface finishes.
    pub surface_finishes: Vec<String>,
    /// Minimum track width.
    pub minimum_track_width_micrometres: i64,
    /// Minimum copper clearance.
    pub minimum_clearance_micrometres: i64,
    /// Minimum finished drill.
    pub minimum_drill_micrometres: i64,
    /// Minimum annular ring.
    pub minimum_annular_ring_micrometres: i64,
    /// Minimum solder-mask bridge.
    pub minimum_mask_bridge_micrometres: i64,
    /// Minimum copper-to-edge clearance.
    pub minimum_edge_clearance_micrometres: i64,
    /// Minimum non-plated hole.
    pub minimum_npth_micrometres: i64,
    /// Minimum slot width.
    pub minimum_slot_width_micrometres: i64,
    /// Supported basic plating types.
    pub supported_plating: Vec<String>,
}

/// Pick-and-place coordinate convention.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoordinateConvention {
    /// Unit.
    pub unit: String,
    /// Origin convention.
    pub origin: String,
    /// X direction.
    pub x_direction: String,
    /// Y direction.
    pub y_direction: String,
    /// Front rotation mapping.
    pub front_rotation: String,
    /// Back rotation mapping.
    pub back_rotation: String,
}

/// Required output filename mapping.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OutputMapping {
    /// Logical output role.
    pub role: String,
    /// Relative package path.
    pub path: String,
}

/// Versioned manufacturer profile.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManufacturerProfile {
    /// Schema version.
    pub schema_version: String,
    /// Stable profile ID.
    pub id: String,
    /// Profile revision.
    pub version: String,
    /// Human-readable name.
    pub name: String,
    /// Lifecycle/status marker.
    pub status: String,
    /// Required limitation statement.
    pub disclaimer: String,
    /// Source URLs.
    pub source: Vec<String>,
    /// Last reviewed date.
    pub updated_on: String,
    /// Applicable design scope.
    pub applicability: ProfileApplicability,
    /// Conservative rules.
    pub capabilities: ManufacturerCapabilities,
    /// Coordinate conventions.
    pub coordinates: CoordinateConvention,
    /// Required output names.
    pub outputs: Vec<OutputMapping>,
    /// Explicit missing coverage.
    pub coverage_gaps: Vec<String>,
}

impl ManufacturerProfile {
    /// Converts the profile's deterministic subset to native PCB rules.
    pub fn pcb_rules(&self) -> PcbRules {
        PcbRules {
            version: format!("{}@{}", self.id, self.version),
            profile_id: self.id.clone(),
            minimum_track_width: Length::from_micrometres(
                self.capabilities.minimum_track_width_micrometres,
            ),
            minimum_clearance: Length::from_micrometres(
                self.capabilities.minimum_clearance_micrometres,
            ),
            minimum_drill: Length::from_micrometres(self.capabilities.minimum_drill_micrometres),
            minimum_annular_ring: Length::from_micrometres(
                self.capabilities.minimum_annular_ring_micrometres,
            ),
            minimum_edge_clearance: Length::from_micrometres(
                self.capabilities.minimum_edge_clearance_micrometres,
            ),
            minimum_mask_bridge: Length::from_micrometres(
                self.capabilities.minimum_mask_bridge_micrometres,
            ),
            minimum_npth: Length::from_micrometres(self.capabilities.minimum_npth_micrometres),
            maximum_artifact_bytes: 16 * 1024 * 1024,
            evidence_refs: self
                .source
                .iter()
                .map(|source| format!("manufacturer-source:{source}"))
                .collect(),
        }
    }
}

/// Profile validation finding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFinding {
    /// Machine-readable code.
    pub code: String,
    /// Whether the profile is unusable until repaired.
    pub blocking: bool,
    /// Explanation.
    pub message: String,
}

/// Profile validation report.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileValidation {
    /// Profile ID.
    pub profile_id: String,
    /// Profile version.
    pub profile_version: String,
    /// Whether no blocking finding exists.
    pub usable_for_golden_test: bool,
    /// Schema and coverage findings.
    pub findings: Vec<ProfileFinding>,
}

/// Loads a bounded YAML manufacturer profile.
///
/// # Errors
///
/// Returns [`ProfileError`] for I/O failure, oversized input or invalid YAML.
pub fn load_profile(path: &Path, maximum_bytes: u64) -> Result<ManufacturerProfile, ProfileError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > maximum_bytes {
        return Err(ProfileError::TooLarge {
            actual: metadata.len(),
            maximum: maximum_bytes,
        });
    }
    let content = fs::read_to_string(path)?;
    Ok(serde_yaml::from_str(&content)?)
}

/// Validates completeness and limitation markings for the Phase 0 profile.
pub fn validate_profile(profile: &ManufacturerProfile) -> ProfileValidation {
    let mut findings = Vec::new();
    let required_roles = [
        "gerber-copper-front",
        "gerber-copper-back",
        "gerber-outline",
        "gerber-job",
        "drill",
        "ipc356",
        "bom",
        "cpl",
    ];
    for role in required_roles {
        if !profile.outputs.iter().any(|output| output.role == role) {
            findings.push(ProfileFinding {
                code: "profile.missing-output-role".to_owned(),
                blocking: true,
                message: format!("missing required output role {role}"),
            });
        }
    }
    for (name, value) in [
        (
            "minimumTrackWidthMicrometres",
            profile.capabilities.minimum_track_width_micrometres,
        ),
        (
            "minimumClearanceMicrometres",
            profile.capabilities.minimum_clearance_micrometres,
        ),
        (
            "minimumDrillMicrometres",
            profile.capabilities.minimum_drill_micrometres,
        ),
        (
            "minimumAnnularRingMicrometres",
            profile.capabilities.minimum_annular_ring_micrometres,
        ),
    ] {
        if value <= 0 {
            findings.push(ProfileFinding {
                code: "profile.invalid-positive-rule".to_owned(),
                blocking: true,
                message: format!("{name} must be positive"),
            });
        }
    }
    if !profile.disclaimer.to_ascii_lowercase().contains("factory")
        || !profile
            .disclaimer
            .to_ascii_lowercase()
            .contains("golden test")
    {
        findings.push(ProfileFinding {
            code: "profile.missing-limitation".to_owned(),
            blocking: true,
            message: "profile must say it is golden-test-only and requires factory confirmation"
                .to_owned(),
        });
    }
    if profile.source.is_empty() || profile.updated_on.is_empty() {
        findings.push(ProfileFinding {
            code: "profile.missing-provenance".to_owned(),
            blocking: true,
            message: "profile source and review date are required".to_owned(),
        });
    }
    if profile.coverage_gaps.is_empty() {
        findings.push(ProfileFinding {
            code: "profile.coverage-not-declared".to_owned(),
            blocking: true,
            message: "profile must expose incomplete capability coverage".to_owned(),
        });
    }
    let usable_for_golden_test = findings.iter().all(|finding| !finding.blocking);
    ProfileValidation {
        profile_id: profile.id.clone(),
        profile_version: profile.version.clone(),
        usable_for_golden_test,
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_profile_is_complete_and_explicitly_limited() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../profiles/manufacturers/jlcpcb-standard-v0.yaml");
        let profile = load_profile(&path, 1024 * 1024).unwrap();
        let report = validate_profile(&profile);
        assert!(report.usable_for_golden_test, "{:?}", report.findings);
        assert!(!profile.coverage_gaps.is_empty());
    }
}
