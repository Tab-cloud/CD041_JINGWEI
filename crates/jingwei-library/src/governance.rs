use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ApprovalState, LibraryAsset, LibraryAssetCandidate, LibraryProvider, VerificationState,
};

/// Scope in which a library decision is valid.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum GovernanceScope {
    Project { project_id: String },
    Organization { organization_id: String },
}

/// Authenticated role carried by a library governance action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GovernanceRole {
    AutomatedAgent,
    ProjectLibraryReviewer,
    OrganizationLibraryAdmin,
    ReleaseEngineer,
}

/// Immutable decision written to the library ledger.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GovernanceDecision {
    Approved,
    Rejected,
    Revoked,
}

/// Policy and evidence for one provider snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    pub id: String,
    pub asset_id: String,
    pub asset_content_hash: String,
    pub provider: LibraryProvider,
    pub captured_at_epoch_seconds: u64,
    pub ttl_seconds: u64,
    pub license_id: String,
    pub cache_policy: String,
    pub data_classification: String,
    pub offline_frozen: bool,
    pub evidence_refs: Vec<String>,
}

/// Result of evaluating a provider snapshot without reaching the network.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderSnapshotStatus {
    Fresh,
    Frozen,
    Stale,
    Unlicensed,
    CachePolicyDenied,
    DataClassificationDenied,
    HashMismatch,
    ProviderMismatch,
}

/// Project policy applied to every library asset used by a revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGovernancePolicy {
    pub allowed_licenses: BTreeSet<String>,
    pub allowed_cache_policies: BTreeSet<String>,
    pub allowed_data_classifications: BTreeSet<String>,
    pub allow_offline_frozen_snapshots: bool,
    pub require_verified_assets: bool,
    pub require_approved_assets: bool,
}

/// Product use class that controls the mechanical-model requirement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LibraryUseClass {
    General,
    CriticalConnector,
    MechanicalInterface,
}

/// Append-only governance record bound to the exact content hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGovernanceRecord {
    pub id: String,
    pub asset_id: String,
    pub asset_content_hash: String,
    pub scope: GovernanceScope,
    pub decision: GovernanceDecision,
    pub actor_id: String,
    pub actor_role: GovernanceRole,
    pub decided_at_epoch_seconds: u64,
    pub rationale: String,
    pub evidence_refs: Vec<String>,
    pub human_reviewed: bool,
}

/// Reverse index entry used to find revisions impacted by an asset event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryUsage {
    pub asset_id: String,
    pub asset_content_hash: String,
    pub project_id: String,
    pub revision: String,
    pub entity_ids: BTreeSet<String>,
}

/// Serializable append-only governance ledger.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGovernanceLedger {
    records: Vec<LibraryGovernanceRecord>,
    usages: Vec<LibraryUsage>,
}

/// Blocking governance finding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceFinding {
    pub code: String,
    pub asset_id: String,
    pub summary: String,
}

/// Exact candidate governance result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryGovernanceEvaluation {
    pub eligible: bool,
    pub snapshot_statuses: BTreeMap<String, ProviderSnapshotStatus>,
    pub findings: Vec<GovernanceFinding>,
    pub approval_record_ids: Vec<String>,
}

/// Rejected ledger mutation.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum GovernanceError {
    #[error("governance record id already exists: {0}")]
    DuplicateRecord(String),
    #[error("automated agents cannot approve, reject or revoke library assets")]
    AutomatedDecisionDenied,
    #[error("project-scoped library decisions require a project reviewer or organization admin")]
    ProjectRoleDenied,
    #[error("organization-scoped library decisions require an organization library admin")]
    OrganizationRoleDenied,
    #[error(
        "generated assets require explicit human review evidence before organization promotion"
    )]
    GeneratedPromotionDenied,
    #[error("approval/rejection rationale must not be empty")]
    MissingRationale,
    #[error("library usage already exists for project/revision/asset/hash")]
    DuplicateUsage,
}

impl LibraryGovernanceLedger {
    /// Returns the immutable decision history.
    #[must_use]
    pub fn records(&self) -> &[LibraryGovernanceRecord] {
        &self.records
    }

    /// Appends a role-checked decision. Existing records are never modified.
    pub fn append_record(
        &mut self,
        asset: &LibraryAsset,
        record: LibraryGovernanceRecord,
    ) -> Result<(), GovernanceError> {
        if self.records.iter().any(|item| item.id == record.id) {
            return Err(GovernanceError::DuplicateRecord(record.id));
        }
        if record.actor_role == GovernanceRole::AutomatedAgent {
            return Err(GovernanceError::AutomatedDecisionDenied);
        }
        if record.rationale.trim().is_empty() {
            return Err(GovernanceError::MissingRationale);
        }
        match &record.scope {
            GovernanceScope::Project { .. } => {
                if !matches!(
                    record.actor_role,
                    GovernanceRole::ProjectLibraryReviewer
                        | GovernanceRole::OrganizationLibraryAdmin
                ) {
                    return Err(GovernanceError::ProjectRoleDenied);
                }
            }
            GovernanceScope::Organization { .. } => {
                if record.actor_role != GovernanceRole::OrganizationLibraryAdmin {
                    return Err(GovernanceError::OrganizationRoleDenied);
                }
                if asset.source.provider == LibraryProvider::Generated
                    && (!record.human_reviewed || record.evidence_refs.is_empty())
                {
                    return Err(GovernanceError::GeneratedPromotionDenied);
                }
            }
        }
        self.records.push(record);
        Ok(())
    }

    /// Adds one exact reverse-index entry.
    pub fn record_usage(&mut self, usage: LibraryUsage) -> Result<(), GovernanceError> {
        let duplicate = self.usages.iter().any(|item| {
            item.asset_id == usage.asset_id
                && item.asset_content_hash == usage.asset_content_hash
                && item.project_id == usage.project_id
                && item.revision == usage.revision
        });
        if duplicate {
            return Err(GovernanceError::DuplicateUsage);
        }
        self.usages.push(usage);
        Ok(())
    }

    /// Finds every revision bound to an exact asset hash.
    #[must_use]
    pub fn affected_revisions(
        &self,
        asset_id: &str,
        asset_content_hash: &str,
    ) -> Vec<LibraryUsage> {
        let mut result = self
            .usages
            .iter()
            .filter(|usage| {
                usage.asset_id == asset_id && usage.asset_content_hash == asset_content_hash
            })
            .cloned()
            .collect::<Vec<_>>();
        result.sort_by(|left, right| {
            left.project_id
                .cmp(&right.project_id)
                .then_with(|| left.revision.cmp(&right.revision))
        });
        result
    }

    fn effective_record<'a>(
        &'a self,
        asset: &LibraryAsset,
        scopes: &[GovernanceScope],
    ) -> Option<&'a LibraryGovernanceRecord> {
        self.records
            .iter()
            .filter(|record| {
                record.asset_id == asset.id
                    && record.asset_content_hash == asset.content_hash
                    && scopes.contains(&record.scope)
            })
            .max_by(|left, right| {
                left.decided_at_epoch_seconds
                    .cmp(&right.decided_at_epoch_seconds)
                    .then_with(|| left.id.cmp(&right.id))
            })
    }
}

/// Evaluates symbol, footprint and optional model against frozen evidence and decisions.
#[must_use]
pub fn evaluate_candidate_governance(
    candidate: &LibraryAssetCandidate,
    snapshots: &[ProviderSnapshot],
    ledger: &LibraryGovernanceLedger,
    policy: &LibraryGovernancePolicy,
    scopes: &[GovernanceScope],
    use_class: LibraryUseClass,
    now_epoch_seconds: u64,
) -> LibraryGovernanceEvaluation {
    let mut assets = vec![&candidate.symbol, &candidate.footprint];
    if let Some(model) = &candidate.model3d {
        assets.push(model);
    }
    let mut findings = Vec::new();
    let mut snapshot_statuses = BTreeMap::new();
    let mut approval_record_ids = Vec::new();
    for asset in assets {
        let snapshot = snapshots.iter().find(|item| item.asset_id == asset.id);
        let status = snapshot.map_or(ProviderSnapshotStatus::HashMismatch, |item| {
            evaluate_snapshot(asset, item, policy, now_epoch_seconds)
        });
        snapshot_statuses.insert(asset.id.clone(), status.clone());
        if !matches!(
            status,
            ProviderSnapshotStatus::Fresh | ProviderSnapshotStatus::Frozen
        ) {
            findings.push(GovernanceFinding {
                code: format!("LIB.SNAPSHOT.{}", snapshot_status_code(&status)),
                asset_id: asset.id.clone(),
                summary: format!("provider snapshot is not eligible: {status:?}"),
            });
        }
        if policy.require_verified_assets && asset.verification_state != VerificationState::Passed {
            findings.push(GovernanceFinding {
                code: "LIB.VERIFICATION.REQUIRED".to_owned(),
                asset_id: asset.id.clone(),
                summary: "asset has not passed deterministic library verification".to_owned(),
            });
        }
        if policy.require_approved_assets {
            match ledger.effective_record(asset, scopes) {
                Some(record) if record.decision == GovernanceDecision::Approved => {
                    approval_record_ids.push(record.id.clone());
                }
                Some(record) => findings.push(GovernanceFinding {
                    code: match record.decision {
                        GovernanceDecision::Rejected => "LIB.APPROVAL.REJECTED",
                        GovernanceDecision::Revoked => "LIB.APPROVAL.REVOKED",
                        GovernanceDecision::Approved => unreachable!(),
                    }
                    .to_owned(),
                    asset_id: asset.id.clone(),
                    summary: "the latest exact-hash governance decision blocks use".to_owned(),
                }),
                None => findings.push(GovernanceFinding {
                    code: "LIB.APPROVAL.MISSING_OR_STALE".to_owned(),
                    asset_id: asset.id.clone(),
                    summary: "no approved record exists for the current asset content hash"
                        .to_owned(),
                }),
            }
        }
        if matches!(asset.approval_state, ApprovalState::Rejected) {
            findings.push(GovernanceFinding {
                code: "LIB.ASSET.REJECTED".to_owned(),
                asset_id: asset.id.clone(),
                summary: "asset metadata marks this version rejected".to_owned(),
            });
        }
    }
    if matches!(
        use_class,
        LibraryUseClass::CriticalConnector | LibraryUseClass::MechanicalInterface
    ) && candidate.model3d.is_none()
    {
        findings.push(GovernanceFinding {
            code: "LIB.MODEL3D.REQUIRED".to_owned(),
            asset_id: candidate.id.clone(),
            summary: "critical connector or mechanical interface requires an approved 3D model"
                .to_owned(),
        });
    }
    approval_record_ids.sort();
    findings.sort_by(|left, right| {
        left.asset_id
            .cmp(&right.asset_id)
            .then_with(|| left.code.cmp(&right.code))
    });
    LibraryGovernanceEvaluation {
        eligible: findings.is_empty(),
        snapshot_statuses,
        findings,
        approval_record_ids,
    }
}

fn evaluate_snapshot(
    asset: &LibraryAsset,
    snapshot: &ProviderSnapshot,
    policy: &LibraryGovernancePolicy,
    now_epoch_seconds: u64,
) -> ProviderSnapshotStatus {
    if snapshot.asset_content_hash != asset.content_hash {
        return ProviderSnapshotStatus::HashMismatch;
    }
    if snapshot.provider != asset.source.provider {
        return ProviderSnapshotStatus::ProviderMismatch;
    }
    if snapshot.license_id != asset.source.license_id
        || !policy.allowed_licenses.contains(&snapshot.license_id)
    {
        return ProviderSnapshotStatus::Unlicensed;
    }
    if !policy
        .allowed_cache_policies
        .contains(&snapshot.cache_policy)
    {
        return ProviderSnapshotStatus::CachePolicyDenied;
    }
    if !policy
        .allowed_data_classifications
        .contains(&snapshot.data_classification)
    {
        return ProviderSnapshotStatus::DataClassificationDenied;
    }
    let stale =
        now_epoch_seconds.saturating_sub(snapshot.captured_at_epoch_seconds) > snapshot.ttl_seconds;
    if stale {
        if snapshot.offline_frozen && policy.allow_offline_frozen_snapshots {
            ProviderSnapshotStatus::Frozen
        } else {
            ProviderSnapshotStatus::Stale
        }
    } else {
        ProviderSnapshotStatus::Fresh
    }
}

fn snapshot_status_code(status: &ProviderSnapshotStatus) -> &'static str {
    match status {
        ProviderSnapshotStatus::Fresh => "FRESH",
        ProviderSnapshotStatus::Frozen => "FROZEN",
        ProviderSnapshotStatus::Stale => "STALE",
        ProviderSnapshotStatus::Unlicensed => "UNLICENSED",
        ProviderSnapshotStatus::CachePolicyDenied => "CACHE_POLICY_DENIED",
        ProviderSnapshotStatus::DataClassificationDenied => "DATA_CLASSIFICATION_DENIED",
        ProviderSnapshotStatus::HashMismatch => "HASH_MISMATCH",
        ProviderSnapshotStatus::ProviderMismatch => "PROVIDER_MISMATCH",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use jingwei_components::{ExactMpn, PackageRef};

    use super::*;
    use crate::{
        AssetGeometry, AssetKind, AssetSource, FootprintDefinition, LibraryAsset,
        LibraryAssetCandidate, PadDefinition, PadShape, PinPadMap, SymbolDefinition,
    };

    fn asset(id: &str, kind: AssetKind, provider: LibraryProvider) -> LibraryAsset {
        let mpn = ExactMpn::parse("SN65HVD75DR").expect("fixture MPN");
        let package = PackageRef {
            name: "SOIC-8".to_owned(),
            pin_count: 8,
        };
        LibraryAsset {
            id: id.to_owned(),
            kind: kind.clone(),
            logical_name: id.to_owned(),
            format: "jingwei-native".to_owned(),
            content_hash: format!("sha256:{id}"),
            source: AssetSource {
                provider,
                upstream_id: Some(id.to_owned()),
                upstream_version: Some("1".to_owned()),
                source_url: None,
                license_id: "Apache-2.0".to_owned(),
                retrieved_at: "2026-08-20T00:00:00Z".to_owned(),
                generator_name: None,
                generator_version: None,
                input_evidence_refs: vec!["evidence:datasheet".to_owned()],
                redistribution_allowed: true,
            },
            compatible_mpns: vec![mpn],
            compatible_packages: vec![package],
            symbol: (kind == AssetKind::Symbol).then(|| SymbolDefinition {
                units: vec!["A".to_owned()],
                pins: Vec::new(),
                pin_one_marked: true,
            }),
            footprint: (kind == AssetKind::Footprint).then(|| FootprintDefinition {
                pads: vec![PadDefinition {
                    id: "pad:1".to_owned(),
                    number: "1".to_owned(),
                    shape: PadShape::Rect,
                    x_mm: 0.0,
                    y_mm: 0.0,
                    width_mm: 1.0,
                    height_mm: 1.0,
                    rotation_deg: 0.0,
                    electrical: true,
                    polarity: crate::Polarity::None,
                    solder_mask_expansion_mm: 0.0,
                    paste_expansion_mm: 0.0,
                }],
                geometry: AssetGeometry {
                    body_width_mm: 4.0,
                    body_height_mm: 5.0,
                    pitch_mm: 1.27,
                    courtyard_width_mm: 6.0,
                    courtyard_height_mm: 7.0,
                    origin_x_mm: 0.0,
                    origin_y_mm: 0.0,
                    orientation_deg: 0.0,
                    model3d_alignment: None,
                },
                pin_one_marked: true,
                silkscreen_clearance_mm: 0.2,
            }),
            metadata: BTreeMap::new(),
            verification_state: VerificationState::Passed,
            approval_state: ApprovalState::Candidate,
            evidence: Vec::new(),
            review_artifact: None,
        }
    }

    fn candidate(provider: LibraryProvider, include_model: bool) -> LibraryAssetCandidate {
        LibraryAssetCandidate {
            id: "candidate:rs485".to_owned(),
            symbol: asset("symbol:rs485", AssetKind::Symbol, provider.clone()),
            footprint: asset("footprint:soic8", AssetKind::Footprint, provider.clone()),
            model3d: include_model.then(|| asset("model:soic8", AssetKind::Model3d, provider)),
            pin_pad_map: PinPadMap {
                id: "map:rs485".to_owned(),
                entries: Vec::new(),
            },
        }
    }

    fn policy() -> LibraryGovernancePolicy {
        LibraryGovernancePolicy {
            allowed_licenses: BTreeSet::from(["Apache-2.0".to_owned()]),
            allowed_cache_policies: BTreeSet::from(["immutable".to_owned()]),
            allowed_data_classifications: BTreeSet::from(["public".to_owned()]),
            allow_offline_frozen_snapshots: true,
            require_verified_assets: true,
            require_approved_assets: true,
        }
    }

    fn snapshot(asset: &LibraryAsset, captured_at: u64, frozen: bool) -> ProviderSnapshot {
        ProviderSnapshot {
            id: format!("snapshot:{}", asset.id),
            asset_id: asset.id.clone(),
            asset_content_hash: asset.content_hash.clone(),
            provider: asset.source.provider.clone(),
            captured_at_epoch_seconds: captured_at,
            ttl_seconds: 100,
            license_id: asset.source.license_id.clone(),
            cache_policy: "immutable".to_owned(),
            data_classification: "public".to_owned(),
            offline_frozen: frozen,
            evidence_refs: vec!["evidence:provider".to_owned()],
        }
    }

    fn approve(ledger: &mut LibraryGovernanceLedger, asset: &LibraryAsset, id: &str, at: u64) {
        ledger
            .append_record(
                asset,
                LibraryGovernanceRecord {
                    id: id.to_owned(),
                    asset_id: asset.id.clone(),
                    asset_content_hash: asset.content_hash.clone(),
                    scope: GovernanceScope::Project {
                        project_id: "project:golden".to_owned(),
                    },
                    decision: GovernanceDecision::Approved,
                    actor_id: "actor:reviewer".to_owned(),
                    actor_role: GovernanceRole::ProjectLibraryReviewer,
                    decided_at_epoch_seconds: at,
                    rationale: "datasheet and rendered asset reviewed".to_owned(),
                    evidence_refs: vec!["evidence:review".to_owned()],
                    human_reviewed: true,
                },
            )
            .expect("approval must append");
    }

    #[test]
    fn stale_and_revoked_assets_fail_closed_but_frozen_snapshots_are_reproducible() {
        let candidate = candidate(LibraryProvider::LockedOfficial, true);
        let mut ledger = LibraryGovernanceLedger::default();
        for (index, asset) in [
            &candidate.symbol,
            &candidate.footprint,
            candidate.model3d.as_ref().expect("model"),
        ]
        .into_iter()
        .enumerate()
        {
            approve(&mut ledger, asset, &format!("approval:{index}"), 10);
        }
        let stale = [
            snapshot(&candidate.symbol, 0, false),
            snapshot(&candidate.footprint, 0, false),
            snapshot(candidate.model3d.as_ref().expect("model"), 0, false),
        ];
        let denied = evaluate_candidate_governance(
            &candidate,
            &stale,
            &ledger,
            &policy(),
            &[GovernanceScope::Project {
                project_id: "project:golden".to_owned(),
            }],
            LibraryUseClass::CriticalConnector,
            1_000,
        );
        assert!(!denied.eligible);
        assert!(
            denied
                .findings
                .iter()
                .any(|finding| finding.code == "LIB.SNAPSHOT.STALE")
        );

        let frozen = stale
            .into_iter()
            .map(|mut item| {
                item.offline_frozen = true;
                item
            })
            .collect::<Vec<_>>();
        assert!(
            evaluate_candidate_governance(
                &candidate,
                &frozen,
                &ledger,
                &policy(),
                &[GovernanceScope::Project {
                    project_id: "project:golden".to_owned(),
                }],
                LibraryUseClass::CriticalConnector,
                1_000,
            )
            .eligible
        );

        ledger
            .append_record(
                &candidate.symbol,
                LibraryGovernanceRecord {
                    id: "revoke:symbol".to_owned(),
                    asset_id: candidate.symbol.id.clone(),
                    asset_content_hash: candidate.symbol.content_hash.clone(),
                    scope: GovernanceScope::Project {
                        project_id: "project:golden".to_owned(),
                    },
                    decision: GovernanceDecision::Revoked,
                    actor_id: "actor:reviewer".to_owned(),
                    actor_role: GovernanceRole::ProjectLibraryReviewer,
                    decided_at_epoch_seconds: 20,
                    rationale: "manufacturer issued a pinout correction".to_owned(),
                    evidence_refs: vec!["evidence:pcn".to_owned()],
                    human_reviewed: true,
                },
            )
            .expect("revocation must append");
        let revoked = evaluate_candidate_governance(
            &candidate,
            &frozen,
            &ledger,
            &policy(),
            &[GovernanceScope::Project {
                project_id: "project:golden".to_owned(),
            }],
            LibraryUseClass::CriticalConnector,
            1_000,
        );
        assert!(
            revoked
                .findings
                .iter()
                .any(|finding| finding.code == "LIB.APPROVAL.REVOKED")
        );
    }

    #[test]
    fn generated_asset_cannot_self_promote_and_old_hash_approval_is_stale() {
        let mut generated = asset(
            "symbol:generated",
            AssetKind::Symbol,
            LibraryProvider::Generated,
        );
        let mut ledger = LibraryGovernanceLedger::default();
        let record = LibraryGovernanceRecord {
            id: "approval:auto".to_owned(),
            asset_id: generated.id.clone(),
            asset_content_hash: generated.content_hash.clone(),
            scope: GovernanceScope::Organization {
                organization_id: "org:jingwei".to_owned(),
            },
            decision: GovernanceDecision::Approved,
            actor_id: "agent:1".to_owned(),
            actor_role: GovernanceRole::AutomatedAgent,
            decided_at_epoch_seconds: 10,
            rationale: "generated".to_owned(),
            evidence_refs: Vec::new(),
            human_reviewed: false,
        };
        assert_eq!(
            ledger.append_record(&generated, record),
            Err(GovernanceError::AutomatedDecisionDenied)
        );
        approve(&mut ledger, &generated, "approval:old", 20);
        generated.content_hash = "sha256:new".to_owned();
        assert!(
            ledger
                .effective_record(
                    &generated,
                    &[GovernanceScope::Project {
                        project_id: "project:golden".to_owned(),
                    }]
                )
                .is_none()
        );
    }

    #[test]
    fn unlicensed_snapshot_and_missing_critical_model_block_use() {
        let candidate = candidate(LibraryProvider::Manufacturer, false);
        let mut ledger = LibraryGovernanceLedger::default();
        approve(&mut ledger, &candidate.symbol, "approval:symbol", 10);
        approve(&mut ledger, &candidate.footprint, "approval:footprint", 10);
        let mut snapshots = vec![
            snapshot(&candidate.symbol, 10, false),
            snapshot(&candidate.footprint, 10, false),
        ];
        snapshots[0].license_id = "LicenseRef-Unknown".to_owned();
        let result = evaluate_candidate_governance(
            &candidate,
            &snapshots,
            &ledger,
            &policy(),
            &[GovernanceScope::Project {
                project_id: "project:golden".to_owned(),
            }],
            LibraryUseClass::MechanicalInterface,
            20,
        );
        assert!(!result.eligible);
        assert!(
            result
                .findings
                .iter()
                .any(|finding| finding.code == "LIB.SNAPSHOT.UNLICENSED")
        );
        assert!(
            result
                .findings
                .iter()
                .any(|finding| finding.code == "LIB.MODEL3D.REQUIRED")
        );
    }

    #[test]
    fn impact_index_returns_exact_hash_revisions_only() {
        let mut ledger = LibraryGovernanceLedger::default();
        ledger
            .record_usage(LibraryUsage {
                asset_id: "symbol:rs485".to_owned(),
                asset_content_hash: "sha256:a".to_owned(),
                project_id: "project:b".to_owned(),
                revision: "rev:2".to_owned(),
                entity_ids: BTreeSet::from(["symbol:u1".to_owned()]),
            })
            .expect("usage");
        ledger
            .record_usage(LibraryUsage {
                asset_id: "symbol:rs485".to_owned(),
                asset_content_hash: "sha256:b".to_owned(),
                project_id: "project:a".to_owned(),
                revision: "rev:1".to_owned(),
                entity_ids: BTreeSet::from(["symbol:u2".to_owned()]),
            })
            .expect("usage");
        let affected = ledger.affected_revisions("symbol:rs485", "sha256:a");
        assert_eq!(affected.len(), 1);
        assert_eq!(affected[0].revision, "rev:2");
    }
}
