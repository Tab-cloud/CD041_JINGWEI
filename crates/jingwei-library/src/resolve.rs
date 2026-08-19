use jingwei_components::{ExactMpn, PackageRef};
use serde::{Deserialize, Serialize};

use crate::{ApprovalState, LibraryAssetCandidate, LibraryProvider, VerificationState};

/// Library resolution and license policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPolicy {
    pub allowed_providers: Vec<LibraryProvider>,
    pub require_verified: bool,
    pub require_approved: bool,
    pub require_3d_model: bool,
}

/// Exactness status for a library query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ResolutionStatus {
    Exact,
    CompatibleNeedsReview,
    NotFound,
    Conflict,
}

/// Deterministic ordered resolution result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryResolution {
    pub status: ResolutionStatus,
    pub selected: Option<LibraryAssetCandidate>,
    pub considered_ids: Vec<String>,
    pub missing_evidence: Vec<String>,
    pub reasons: Vec<String>,
}

/// Resolves only exact MPN/package/pin-count matches using the mandated order.
#[must_use]
pub fn resolve_library(
    mpn: &ExactMpn,
    package: &PackageRef,
    policy: &LibraryPolicy,
    candidates: &[LibraryAssetCandidate],
) -> LibraryResolution {
    let mut exact = candidates
        .iter()
        .filter(|candidate| {
            candidate.symbol.compatible_mpns.contains(mpn)
                && candidate.footprint.compatible_mpns.contains(mpn)
                && candidate.symbol.compatible_packages.contains(package)
                && candidate.footprint.compatible_packages.contains(package)
                && policy
                    .allowed_providers
                    .contains(&candidate.symbol.source.provider)
                && policy
                    .allowed_providers
                    .contains(&candidate.footprint.source.provider)
        })
        .cloned()
        .collect::<Vec<_>>();
    exact.sort_by_key(|candidate| {
        provider_rank(&candidate.symbol.source.provider)
            .max(provider_rank(&candidate.footprint.source.provider))
    });
    let considered_ids = exact.iter().map(|item| item.id.clone()).collect();
    if exact.is_empty() {
        return LibraryResolution {
            status: ResolutionStatus::NotFound,
            selected: None,
            considered_ids,
            missing_evidence: vec!["exact MPN/package/pin-count library asset".to_owned()],
            reasons: vec!["fuzzy name or package matching is not accepted as exact".to_owned()],
        };
    }
    if exact.len() > 1
        && provider_rank(&exact[0].symbol.source.provider)
            == provider_rank(&exact[1].symbol.source.provider)
    {
        return LibraryResolution {
            status: ResolutionStatus::Conflict,
            selected: None,
            considered_ids,
            missing_evidence: Vec::new(),
            reasons: vec![
                "multiple exact candidates exist at the same policy precedence".to_owned(),
            ],
        };
    }
    let candidate = exact.remove(0);
    let verified = candidate.symbol.verification_state == VerificationState::Passed
        && candidate.footprint.verification_state == VerificationState::Passed;
    let approved = is_approved(&candidate.symbol.approval_state)
        && is_approved(&candidate.footprint.approval_state);
    let has_model = candidate.model3d.is_some();
    let exact_status = (!policy.require_verified || verified)
        && (!policy.require_approved || approved)
        && (!policy.require_3d_model || has_model);
    let mut missing_evidence = Vec::new();
    if policy.require_verified && !verified {
        missing_evidence.push("passed symbol and footprint verification".to_owned());
    }
    if policy.require_approved && !approved {
        missing_evidence.push("project or organization approval".to_owned());
    }
    if policy.require_3d_model && !has_model {
        missing_evidence.push("compatible 3D model".to_owned());
    }
    LibraryResolution {
        status: if exact_status {
            ResolutionStatus::Exact
        } else {
            ResolutionStatus::CompatibleNeedsReview
        },
        selected: Some(candidate),
        considered_ids,
        missing_evidence,
        reasons: vec![
            "resolved by exact MPN, exact package name, pin count and policy order".to_owned(),
        ],
    }
}

fn provider_rank(provider: &LibraryProvider) -> u8 {
    match provider {
        LibraryProvider::Project => 0,
        LibraryProvider::ApprovedOrganization => 1,
        LibraryProvider::LockedOfficial => 2,
        LibraryProvider::ApprovedThirdParty => 3,
        LibraryProvider::Manufacturer => 4,
        LibraryProvider::Generated => 5,
    }
}

fn is_approved(state: &ApprovalState) -> bool {
    matches!(
        state,
        ApprovalState::ProjectApproved | ApprovalState::OrganizationApproved
    )
}
