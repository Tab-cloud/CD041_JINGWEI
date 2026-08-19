//! Deterministic maturity assessment and offline release verification.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ApprovalAttestation, AssessmentId, DependencyLocks, EffectiveReleaseStatus, GateResult,
    GateResultId, GateStatus, MaturityAssessment, MaturityLevel, ProjectId, ReleaseId,
    ReleaseRecord, ReleaseRequest, ReleaseRequestId, RevisionId, RevokeRecord,
    canonical_json_bytes, canonical_sha256,
};

const KNOWN_GATES: [&str; 8] = ["G0", "G1", "G2", "G3", "G4", "G5", "G6", "G7"];

/// Raw gate result before it is bound to an assessment dependency set.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GateObservation {
    pub gate: String,
    pub status: GateStatus,
    pub finding_refs: Vec<String>,
    pub provider: String,
}

/// Role policy enforced before preparing a release request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleasePolicy {
    pub required_roles: BTreeSet<String>,
}

/// Current dependency state used to invalidate an old assessment.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentDependencyState {
    pub revision_id: RevisionId,
    pub locks: DependencyLocks,
}

/// Exact assessment validity projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentValidity {
    pub current: bool,
    pub stale_reasons: Vec<String>,
}

/// Trusted organization release public key.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedReleaseKey {
    pub key_id: String,
    pub public_key_hex: String,
}

/// Offline verifier result shown identically in UI and headless workflows.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfflineReleaseVerification {
    pub status: EffectiveReleaseStatus,
    pub findings: Vec<String>,
}

/// Evidence bundle supplied to the offline release verifier.
pub struct OfflineReleaseContext<'a> {
    pub assessment: &'a MaturityAssessment,
    pub current: &'a AssessmentDependencyState,
    pub artifact_manifest_hash: &'a str,
    pub attestations: &'a [ApprovalAttestation],
    pub policy: &'a ReleasePolicy,
    pub trusted_keys: &'a [TrustedReleaseKey],
    pub revocations: &'a [RevokeRecord],
    pub now: DateTime<Utc>,
}

/// Invalid lifecycle input or record.
#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum LifecycleEngineError {
    #[error("unknown maturity gate: {0}")]
    UnknownGate(String),
    #[error("duplicate maturity gate: {0}")]
    DuplicateGate(String),
    #[error("assessment identifier does not match its canonical content")]
    AssessmentIdMismatch,
    #[error("release request identifier does not match its canonical content")]
    ReleaseRequestIdMismatch,
    #[error("release record identifier does not match its canonical content")]
    ReleaseIdMismatch,
    #[error("release signer key identifier must not be empty")]
    MissingSignerKey,
    #[error("release record cannot be issued before its release request")]
    ReleaseIssuedBeforeRequest,
    #[error("release request requires a current L4 prototype-verified assessment")]
    AssessmentNotReleaseReady,
    #[error("release request assessment does not match the supplied assessment")]
    AssessmentMismatch,
    #[error("artifact manifest hash does not match the release request")]
    ManifestHashMismatch,
    #[error("required release attestation role is missing or expired: {0}")]
    MissingRole(String),
    #[error("attestation is not bound to the exact assessment: {0}")]
    AttestationMismatch(String),
    #[error("attestation actor lacks its asserted role claim: {0}")]
    AttestationRoleClaimMissing(String),
    #[error("canonical lifecycle hashing failed: {0}")]
    Canonical(String),
}

/// Creates immutable gate IDs and the highest defensible L0-L4 maturity level.
pub fn build_maturity_assessment(
    project_id: ProjectId,
    revision_id: RevisionId,
    locks: DependencyLocks,
    observations: Vec<GateObservation>,
    issued_at: DateTime<Utc>,
) -> Result<MaturityAssessment, LifecycleEngineError> {
    let mut seen = BTreeSet::new();
    let mut sorted = observations;
    sorted.sort_by(|left, right| left.gate.cmp(&right.gate));
    for observation in &sorted {
        if !KNOWN_GATES.contains(&observation.gate.as_str()) {
            return Err(LifecycleEngineError::UnknownGate(observation.gate.clone()));
        }
        if !seen.insert(observation.gate.clone()) {
            return Err(LifecycleEngineError::DuplicateGate(
                observation.gate.clone(),
            ));
        }
    }
    let mut gates = Vec::with_capacity(sorted.len());
    for observation in sorted {
        let hash = canonical_sha256(&(
            &project_id,
            &revision_id,
            &locks,
            &observation.gate,
            observation.status,
            &observation.finding_refs,
            &observation.provider,
        ))
        .map_err(|error| LifecycleEngineError::Canonical(error.to_string()))?;
        gates.push(GateResult {
            id: GateResultId::new(format!("gate-result-{hash}"))
                .expect("generated gate result ID is bounded"),
            gate: observation.gate,
            status: observation.status,
            finding_refs: observation.finding_refs,
            provider: observation.provider,
        });
    }
    let highest_level = highest_maturity(&gates);
    let hash = assessment_hash(
        &project_id,
        &revision_id,
        &locks,
        highest_level,
        &gates,
        issued_at,
    )?;
    Ok(MaturityAssessment {
        id: AssessmentId::new(format!("assessment-{hash}"))
            .expect("generated assessment ID is bounded"),
        project_id,
        revision_id,
        locks,
        highest_level,
        gates,
        issued_at,
    })
}

/// Verifies canonical identity and reports every dependency that made an assessment stale.
#[must_use]
pub fn assess_validity(
    assessment: &MaturityAssessment,
    current: &AssessmentDependencyState,
) -> AssessmentValidity {
    let mut stale_reasons = Vec::new();
    match expected_assessment_id(assessment) {
        Ok(expected) if expected == assessment.id => {}
        _ => stale_reasons.push("assessment canonical identity mismatch".to_owned()),
    }
    if assessment.revision_id != current.revision_id {
        stale_reasons.push("design revision changed".to_owned());
    }
    compare_lock(
        "intent revision",
        &assessment.locks.intent_revision,
        &current.locks.intent_revision,
        &mut stale_reasons,
    );
    compare_lock(
        "toolchain lock",
        &assessment.locks.toolchain_lock,
        &current.locks.toolchain_lock,
        &mut stale_reasons,
    );
    compare_lock(
        "rule set lock",
        &assessment.locks.rule_set_lock,
        &current.locks.rule_set_lock,
        &mut stale_reasons,
    );
    compare_lock(
        "manufacturer profile lock",
        &assessment.locks.manufacturer_profile_lock,
        &current.locks.manufacturer_profile_lock,
        &mut stale_reasons,
    );
    compare_lock(
        "evidence set hash",
        &assessment.locks.evidence_set_hash,
        &current.locks.evidence_set_hash,
        &mut stale_reasons,
    );
    compare_lock(
        "waiver set hash",
        &assessment.locks.waiver_set_hash,
        &current.locks.waiver_set_hash,
        &mut stale_reasons,
    );
    AssessmentValidity {
        current: stale_reasons.is_empty(),
        stale_reasons,
    }
}

/// Prepares an unsigned request for a separate organization signer.
pub fn prepare_release_request(
    assessment: &MaturityAssessment,
    current: &AssessmentDependencyState,
    artifact_manifest_hash: String,
    attestations: &[ApprovalAttestation],
    policy: &ReleasePolicy,
    issued_at: DateTime<Utc>,
) -> Result<ReleaseRequest, LifecycleEngineError> {
    if !assess_validity(assessment, current).current
        || assessment.highest_level != MaturityLevel::L4PrototypeVerified
        || !gates_passed(&assessment.gates, &KNOWN_GATES)
    {
        return Err(LifecycleEngineError::AssessmentNotReleaseReady);
    }
    validate_attestations(assessment, attestations, policy, issued_at)?;
    let mut attestation_ids = attestations
        .iter()
        .map(|attestation| attestation.id.clone())
        .collect::<Vec<_>>();
    attestation_ids.sort();
    let hash = release_request_hash(
        &assessment.project_id,
        &assessment.revision_id,
        &assessment.id,
        &artifact_manifest_hash,
        &attestation_ids,
        issued_at,
    )?;
    Ok(ReleaseRequest {
        id: ReleaseRequestId::new(format!("release-request-{hash}"))
            .expect("generated release request ID is bounded"),
        project_id: assessment.project_id.clone(),
        revision_id: assessment.revision_id.clone(),
        assessment_id: assessment.id.clone(),
        artifact_manifest_hash,
        attestation_ids,
        issued_at,
    })
}

/// Builds the canonical unsigned envelope an independent organization signer must sign.
pub fn prepare_unsigned_release_record(
    request: ReleaseRequest,
    signer_key_id: String,
    issued_at: DateTime<Utc>,
) -> Result<ReleaseRecord, LifecycleEngineError> {
    if signer_key_id.trim().is_empty() {
        return Err(LifecycleEngineError::MissingSignerKey);
    }
    if issued_at < request.issued_at {
        return Err(LifecycleEngineError::ReleaseIssuedBeforeRequest);
    }
    let id = expected_release_id(&request, &signer_key_id, issued_at)?;
    Ok(ReleaseRecord {
        id,
        request,
        signer_key_id,
        signature: String::new(),
        issued_at,
    })
}

/// Returns canonical bytes for the full release envelope, excluding only its signature.
pub fn release_signing_bytes(record: &ReleaseRecord) -> Result<Vec<u8>, LifecycleEngineError> {
    canonical_json_bytes(&(
        &record.id,
        &record.request,
        &record.signer_key_id,
        record.issued_at,
    ))
    .map_err(|error| LifecycleEngineError::Canonical(error.to_string()))
}

/// Performs deterministic offline identity, dependency, role, manifest and signature checks.
#[must_use]
pub fn verify_release_offline(
    release: &ReleaseRecord,
    context: &OfflineReleaseContext<'_>,
) -> OfflineReleaseVerification {
    let mut findings = Vec::new();
    let assessment = context.assessment;
    let validity = assess_validity(assessment, context.current);
    findings.extend(validity.stale_reasons);
    if assessment.highest_level != MaturityLevel::L4PrototypeVerified
        || !gates_passed(&assessment.gates, &KNOWN_GATES)
    {
        findings.push("assessment is not L4 with G0-G7 passed".to_owned());
    }
    if release.request.assessment_id != assessment.id
        || release.request.project_id != assessment.project_id
        || release.request.revision_id != assessment.revision_id
    {
        findings.push("release request assessment binding mismatch".to_owned());
    }
    if expected_release_request_id(&release.request).ok().as_ref() != Some(&release.request.id) {
        findings.push("release request canonical identity mismatch".to_owned());
    }
    if expected_release_id(&release.request, &release.signer_key_id, release.issued_at)
        .ok()
        .as_ref()
        != Some(&release.id)
    {
        findings.push("release record canonical identity mismatch".to_owned());
    }
    if release.issued_at < release.request.issued_at {
        findings.push("release record predates its request".to_owned());
    }
    if release.request.issued_at > context.now || release.issued_at > context.now {
        findings.push("release record is not yet effective".to_owned());
    }
    if release.request.artifact_manifest_hash != context.artifact_manifest_hash {
        findings.push("artifact manifest hash mismatch".to_owned());
    }
    if let Err(error) = validate_attestations(
        assessment,
        context.attestations,
        context.policy,
        context.now,
    ) {
        findings.push(error.to_string());
    }
    let supplied_ids = context
        .attestations
        .iter()
        .map(|attestation| attestation.id.clone())
        .collect::<BTreeSet<_>>();
    if release
        .request
        .attestation_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        != supplied_ids
    {
        findings.push("release request attestation set mismatch".to_owned());
    }
    if !verify_ed25519(
        &release.signer_key_id,
        &release.signature,
        &release_signing_bytes(release).unwrap_or_default(),
        context.trusted_keys,
    ) {
        findings.push("release signature is invalid or untrusted".to_owned());
    }
    if !findings.is_empty() {
        let stale = findings.iter().any(|finding| {
            finding.contains("changed")
                || finding.contains("expired")
                || finding.contains("hash mismatch")
        });
        return OfflineReleaseVerification {
            status: if stale {
                EffectiveReleaseStatus::Stale
            } else {
                EffectiveReleaseStatus::Invalid
            },
            findings,
        };
    }
    if context.revocations.iter().any(|record| {
        record.release_id == release.id
            && verify_ed25519(
                &record.signer_key_id,
                &record.signature,
                &revocation_signing_bytes(record).unwrap_or_default(),
                context.trusted_keys,
            )
    }) {
        return OfflineReleaseVerification {
            status: EffectiveReleaseStatus::Revoked,
            findings: vec!["a trusted organization revocation applies to this release".to_owned()],
        };
    }
    OfflineReleaseVerification {
        status: EffectiveReleaseStatus::Active,
        findings,
    }
}

/// Returns canonical bytes for an independently signed revocation, excluding its signature.
pub fn revocation_signing_bytes(record: &RevokeRecord) -> Result<Vec<u8>, LifecycleEngineError> {
    canonical_json_bytes(&(
        &record.id,
        &record.release_id,
        &record.reason,
        &record.signer_key_id,
        record.issued_at,
    ))
    .map_err(|error| LifecycleEngineError::Canonical(error.to_string()))
}

fn highest_maturity(gates: &[GateResult]) -> MaturityLevel {
    if gates_passed(gates, &KNOWN_GATES) {
        return MaturityLevel::L4PrototypeVerified;
    }
    if gates_passed(gates, &["G0", "G1", "G2", "G3", "G4", "G5"]) {
        return MaturityLevel::L3ManufacturingCandidate;
    }
    if gates_passed(gates, &["G0", "G1", "G2", "G3", "G4"]) {
        return MaturityLevel::L2EngineeringVerified;
    }
    if gates_passed(gates, &["G0"]) {
        return MaturityLevel::L1EcadCandidate;
    }
    MaturityLevel::L0Draft
}

fn gates_passed(gates: &[GateResult], required: &[&str]) -> bool {
    let statuses = gates
        .iter()
        .map(|gate| (gate.gate.as_str(), gate.status))
        .collect::<BTreeMap<_, _>>();
    required
        .iter()
        .all(|gate| statuses.get(gate) == Some(&GateStatus::Passed))
}

fn validate_attestations(
    assessment: &MaturityAssessment,
    attestations: &[ApprovalAttestation],
    policy: &ReleasePolicy,
    at: DateTime<Utc>,
) -> Result<(), LifecycleEngineError> {
    let mut effective_roles = BTreeSet::new();
    for attestation in attestations {
        if attestation.assessment_id != assessment.id {
            return Err(LifecycleEngineError::AttestationMismatch(
                attestation.id.to_string(),
            ));
        }
        if !attestation.actor.role_claims.contains(&attestation.role) {
            return Err(LifecycleEngineError::AttestationRoleClaimMissing(
                attestation.role.clone(),
            ));
        }
        if attestation.expires_at.is_none_or(|expiry| expiry >= at) {
            effective_roles.insert(attestation.role.clone());
        }
    }
    for role in &policy.required_roles {
        if !effective_roles.contains(role) {
            return Err(LifecycleEngineError::MissingRole(role.clone()));
        }
    }
    Ok(())
}

fn compare_lock(label: &str, assessed: &str, current: &str, reasons: &mut Vec<String>) {
    if assessed != current {
        reasons.push(format!("{label} changed"));
    }
}

fn expected_assessment_id(
    assessment: &MaturityAssessment,
) -> Result<AssessmentId, LifecycleEngineError> {
    let hash = assessment_hash(
        &assessment.project_id,
        &assessment.revision_id,
        &assessment.locks,
        assessment.highest_level,
        &assessment.gates,
        assessment.issued_at,
    )?;
    Ok(
        AssessmentId::new(format!("assessment-{hash}"))
            .expect("generated assessment ID is bounded"),
    )
}

fn assessment_hash(
    project_id: &ProjectId,
    revision_id: &RevisionId,
    locks: &DependencyLocks,
    highest_level: MaturityLevel,
    gates: &[GateResult],
    issued_at: DateTime<Utc>,
) -> Result<String, LifecycleEngineError> {
    canonical_sha256(&(
        project_id,
        revision_id,
        locks,
        highest_level,
        gates,
        issued_at,
    ))
    .map_err(|error| LifecycleEngineError::Canonical(error.to_string()))
}

fn expected_release_request_id(
    request: &ReleaseRequest,
) -> Result<ReleaseRequestId, LifecycleEngineError> {
    let hash = release_request_hash(
        &request.project_id,
        &request.revision_id,
        &request.assessment_id,
        &request.artifact_manifest_hash,
        &request.attestation_ids,
        request.issued_at,
    )?;
    Ok(ReleaseRequestId::new(format!("release-request-{hash}"))
        .expect("generated release request ID is bounded"))
}

fn release_request_hash(
    project_id: &ProjectId,
    revision_id: &RevisionId,
    assessment_id: &AssessmentId,
    artifact_manifest_hash: &str,
    attestation_ids: &[crate::AttestationId],
    issued_at: DateTime<Utc>,
) -> Result<String, LifecycleEngineError> {
    canonical_sha256(&(
        project_id,
        revision_id,
        assessment_id,
        artifact_manifest_hash,
        attestation_ids,
        issued_at,
    ))
    .map_err(|error| LifecycleEngineError::Canonical(error.to_string()))
}

fn expected_release_id(
    request: &ReleaseRequest,
    signer_key_id: &str,
    issued_at: DateTime<Utc>,
) -> Result<ReleaseId, LifecycleEngineError> {
    let hash = canonical_sha256(&(request, signer_key_id, issued_at))
        .map_err(|error| LifecycleEngineError::Canonical(error.to_string()))?;
    Ok(ReleaseId::new(format!("release-{hash}")).expect("generated release ID is bounded"))
}

fn verify_ed25519(
    key_id: &str,
    signature_hex: &str,
    message: &[u8],
    trusted_keys: &[TrustedReleaseKey],
) -> bool {
    let Some(trusted) = trusted_keys.iter().find(|key| key.key_id == key_id) else {
        return false;
    };
    let Ok(public_key): Result<[u8; 32], _> =
        hex::decode(&trusted.public_key_hex).and_then(|bytes| {
            bytes
                .try_into()
                .map_err(|_| hex::FromHexError::InvalidStringLength)
        })
    else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&public_key) else {
        return false;
    };
    let Ok(signature_bytes) = hex::decode(signature_hex) else {
        return false;
    };
    let Ok(signature) = Signature::from_slice(&signature_bytes) else {
        return false;
    };
    verifying_key.verify(message, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use ed25519_dalek::{Signer, SigningKey};

    use super::*;
    use crate::{AttestationId, AuthenticatedActorRef, RevokeId};

    fn locks() -> DependencyLocks {
        DependencyLocks {
            intent_revision: "intent:1".to_owned(),
            toolchain_lock: "toolchain:1".to_owned(),
            rule_set_lock: "rules:1".to_owned(),
            manufacturer_profile_lock: "profile:1".to_owned(),
            evidence_set_hash: "evidence:1".to_owned(),
            waiver_set_hash: "waivers:1".to_owned(),
        }
    }

    fn time(second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 20, 0, 0, second)
            .single()
            .expect("valid fixture time")
    }

    fn assessment() -> MaturityAssessment {
        build_maturity_assessment(
            ProjectId::new("project:golden").unwrap(),
            RevisionId::new("revision:golden").unwrap(),
            locks(),
            KNOWN_GATES
                .into_iter()
                .map(|gate| GateObservation {
                    gate: gate.to_owned(),
                    status: GateStatus::Passed,
                    finding_refs: Vec::new(),
                    provider: format!("provider:{gate}"),
                })
                .collect(),
            time(1),
        )
        .expect("assessment")
    }

    fn attestation(assessment: &MaturityAssessment, role: &str) -> ApprovalAttestation {
        ApprovalAttestation {
            id: AttestationId::new(format!("attestation:{role}")).unwrap(),
            role: role.to_owned(),
            actor: AuthenticatedActorRef {
                subject: format!("actor:{role}"),
                issuer: "fixture-iam".to_owned(),
                authentication_method: "test-key".to_owned(),
                role_claims: BTreeSet::from([role.to_owned()]),
            },
            assessment_id: assessment.id.clone(),
            issued_at: time(2),
            expires_at: None,
        }
    }

    #[test]
    fn dependency_change_invalidates_assessment_and_release_request() {
        let assessment = assessment();
        assert_eq!(assessment.highest_level, MaturityLevel::L4PrototypeVerified);
        let mut changed = locks();
        changed.rule_set_lock = "rules:2".to_owned();
        let current = AssessmentDependencyState {
            revision_id: assessment.revision_id.clone(),
            locks: changed,
        };
        let validity = assess_validity(&assessment, &current);
        assert!(!validity.current);
        assert_eq!(validity.stale_reasons, ["rule set lock changed"]);
        assert_eq!(
            prepare_release_request(
                &assessment,
                &current,
                "manifest:1".to_owned(),
                &[],
                &ReleasePolicy {
                    required_roles: BTreeSet::new(),
                },
                time(3),
            ),
            Err(LifecycleEngineError::AssessmentNotReleaseReady)
        );
    }

    #[test]
    fn independent_ed25519_record_verifies_and_revocation_wins() {
        let assessment = assessment();
        let current = AssessmentDependencyState {
            revision_id: assessment.revision_id.clone(),
            locks: locks(),
        };
        let policy = ReleasePolicy {
            required_roles: BTreeSet::from([
                "hardware-engineer".to_owned(),
                "release-manager".to_owned(),
            ]),
        };
        let attestations = vec![
            attestation(&assessment, "hardware-engineer"),
            attestation(&assessment, "release-manager"),
        ];
        let request = prepare_release_request(
            &assessment,
            &current,
            "manifest:1".to_owned(),
            &attestations,
            &policy,
            time(3),
        )
        .expect("release request");
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let trusted = TrustedReleaseKey {
            key_id: "organization-key:1".to_owned(),
            public_key_hex: hex::encode(signing_key.verifying_key().as_bytes()),
        };
        let mut release = prepare_unsigned_release_record(request, trusted.key_id.clone(), time(4))
            .expect("unsigned release envelope");
        release.signature = hex::encode(
            signing_key
                .sign(&release_signing_bytes(&release).expect("signing bytes"))
                .to_bytes(),
        );
        let verified = verify_release_offline(
            &release,
            &OfflineReleaseContext {
                assessment: &assessment,
                current: &current,
                artifact_manifest_hash: "manifest:1",
                attestations: &attestations,
                policy: &policy,
                trusted_keys: std::slice::from_ref(&trusted),
                revocations: &[],
                now: time(5),
            },
        );
        assert_eq!(verified.status, EffectiveReleaseStatus::Active);

        let mut tampered = release.clone();
        tampered.id = ReleaseId::new("release:redirected").unwrap();
        let tampered_result = verify_release_offline(
            &tampered,
            &OfflineReleaseContext {
                assessment: &assessment,
                current: &current,
                artifact_manifest_hash: "manifest:1",
                attestations: &attestations,
                policy: &policy,
                trusted_keys: std::slice::from_ref(&trusted),
                revocations: &[],
                now: time(5),
            },
        );
        assert_eq!(tampered_result.status, EffectiveReleaseStatus::Invalid);
        assert!(
            tampered_result
                .findings
                .iter()
                .any(|finding| finding.contains("release record canonical identity mismatch"))
        );

        let mut revoke = RevokeRecord {
            id: RevokeId::new("revoke:1").unwrap(),
            release_id: release.id.clone(),
            reason: "fixture recall".to_owned(),
            signer_key_id: trusted.key_id.clone(),
            signature: String::new(),
            issued_at: time(6),
        };
        revoke.signature = hex::encode(
            signing_key
                .sign(&revocation_signing_bytes(&revoke).expect("revoke bytes"))
                .to_bytes(),
        );
        let revoked = verify_release_offline(
            &release,
            &OfflineReleaseContext {
                assessment: &assessment,
                current: &current,
                artifact_manifest_hash: "manifest:1",
                attestations: &attestations,
                policy: &policy,
                trusted_keys: &[trusted],
                revocations: &[revoke],
                now: time(7),
            },
        );
        assert_eq!(revoked.status, EffectiveReleaseStatus::Revoked);
    }

    #[test]
    fn missing_named_role_blocks_unsigned_request_preparation() {
        let assessment = assessment();
        let current = AssessmentDependencyState {
            revision_id: assessment.revision_id.clone(),
            locks: locks(),
        };
        let result = prepare_release_request(
            &assessment,
            &current,
            "manifest:1".to_owned(),
            &[attestation(&assessment, "hardware-engineer")],
            &ReleasePolicy {
                required_roles: BTreeSet::from(["release-manager".to_owned()]),
            },
            time(3),
        );
        assert_eq!(
            result,
            Err(LifecycleEngineError::MissingRole(
                "release-manager".to_owned()
            ))
        );
    }
}
