use std::collections::BTreeSet;
use std::sync::Arc;

use jingwei_components::{EvidenceRef, ExactMpn, PackageRef};
use jingwei_daemon::{Daemon, InMemoryClient, production_registry};
use jingwei_jdp::{
    JDP_PROTOCOL_VERSION, JdpClient, JdpRequest, JdpResponse, ProjectCreateInput, ProjectInfo,
    operation,
};
use jingwei_jir::{IdempotencyKey, ProjectId, RequestId, WorkspaceId};
use jingwei_kernel::ToolResult;
use jingwei_library::{
    AssetGeometry, LibraryGovernanceLedger, LibraryGovernancePolicy, LibraryUseClass,
    PadDefinition, PadShape, Polarity, ProviderSnapshot, StructuredFootprintInput,
    StructuredPinInput, generate_assets,
};
use serde_json::{Value, json};
use tempfile::tempdir;

#[tokio::test]
async fn daemon_library_governance_rejects_a_stale_provider_snapshot() {
    let directory = tempdir().unwrap();
    let workspace = WorkspaceId::new("ws-library-governance").unwrap();
    let project = ProjectId::new("project-library-governance").unwrap();
    let daemon = Arc::new(
        Daemon::create(
            directory.path(),
            workspace.clone(),
            production_registry().unwrap(),
        )
        .unwrap(),
    );
    let client = InMemoryClient::connect(daemon, "library-governance-test").unwrap();
    let created: JdpResponse<ProjectInfo> = client
        .send(request(
            &workspace,
            &project,
            operation::PROJECT_CREATE,
            "create",
            serde_json::to_value(ProjectCreateInput {
                name: "Library governance".into(),
            })
            .unwrap(),
        ))
        .await
        .unwrap();
    assert!(matches!(created, JdpResponse::Success(_)));

    let candidate = generate_assets(&fixture_input()).expect("generated fixture");
    let snapshots = [&candidate.symbol, &candidate.footprint]
        .into_iter()
        .map(|asset| ProviderSnapshot {
            id: format!("snapshot:{}", asset.id),
            asset_id: asset.id.clone(),
            asset_content_hash: asset.content_hash.clone(),
            provider: asset.source.provider.clone(),
            captured_at_epoch_seconds: 1,
            ttl_seconds: 10,
            license_id: asset.source.license_id.clone(),
            cache_policy: "immutable".into(),
            data_classification: "public".into(),
            offline_frozen: false,
            evidence_refs: vec!["evidence:fixture".into()],
        })
        .collect::<Vec<_>>();
    let response: JdpResponse<ToolResult> = client
        .send(request(
            &workspace,
            &project,
            "library_governance_evaluate",
            "evaluate",
            json!({
                "candidate": candidate,
                "snapshots": snapshots,
                "ledger": LibraryGovernanceLedger::default(),
                "policy": LibraryGovernancePolicy {
                    allowed_licenses: BTreeSet::from(["project-private-generated".to_owned()]),
                    allowed_cache_policies: BTreeSet::from(["immutable".to_owned()]),
                    allowed_data_classifications: BTreeSet::from(["public".to_owned()]),
                    allow_offline_frozen_snapshots: false,
                    require_verified_assets: false,
                    require_approved_assets: false,
                },
                "scopes": [],
                "useClass": LibraryUseClass::General,
                "nowEpochSeconds": 100,
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(response) = response else {
        panic!("governance tool must execute");
    };
    assert_eq!(response.data.data["eligible"], false);
    assert!(
        response.data.data["findings"]
            .as_array()
            .expect("findings")
            .iter()
            .all(|finding| finding["code"] == "LIB.SNAPSHOT.STALE")
    );
}

fn fixture_input() -> StructuredFootprintInput {
    StructuredFootprintInput {
        logical_name: "Fixture resistor".into(),
        package: PackageRef {
            name: "R_0603".into(),
            pin_count: 1,
        },
        manufacturer: "Jingwei Fixtures".into(),
        exact_mpn: ExactMpn::parse("JW-FIXTURE-1").unwrap(),
        pins: vec![StructuredPinInput {
            number: "1".into(),
            name: "P".into(),
            unit: "A".into(),
            electrical_type: "passive".into(),
            direction: "bidirectional".into(),
            polarity: Polarity::None,
            pad_numbers: vec!["1".into()],
            allow_multiple_pads: false,
            evidence_refs: vec!["evidence:fixture".into()],
        }],
        pads: vec![PadDefinition {
            id: "pad:1".into(),
            number: "1".into(),
            shape: PadShape::Rect,
            x_mm: 0.0,
            y_mm: 0.0,
            width_mm: 1.0,
            height_mm: 1.0,
            rotation_deg: 0.0,
            electrical: true,
            polarity: Polarity::None,
            solder_mask_expansion_mm: 0.0,
            paste_expansion_mm: 0.0,
        }],
        geometry: AssetGeometry {
            body_width_mm: 1.0,
            body_height_mm: 1.0,
            pitch_mm: 0.0,
            courtyard_width_mm: 2.0,
            courtyard_height_mm: 2.0,
            origin_x_mm: 0.0,
            origin_y_mm: 0.0,
            orientation_deg: 0.0,
            model3d_alignment: None,
        },
        pin_one_marked: true,
        silkscreen_clearance_mm: 0.2,
        source_evidence: vec![EvidenceRef {
            id: "evidence:fixture".into(),
            kind: "controlled-fixture".into(),
            source_uri: None,
            publisher: "Jingwei".into(),
            version: "1".into(),
            retrieved_at: "2026-08-20T00:00:00Z".into(),
            content_hash: format!("sha256:{}", "1".repeat(64)),
            locator: Some("library governance fixture".into()),
            license: "project-private-generated".into(),
            cache_policy: "immutable".into(),
        }],
        generator_version: "0.1.0".into(),
    }
}

fn request(
    workspace: &WorkspaceId,
    project: &ProjectId,
    operation: &str,
    key: &str,
    input: Value,
) -> JdpRequest<Value> {
    JdpRequest {
        protocol_version: JDP_PROTOCOL_VERSION.into(),
        request_id: RequestId::new(format!("request-{key}")).unwrap(),
        idempotency_key: IdempotencyKey::new(format!("idem-{key}")).unwrap(),
        workspace_id: workspace.clone(),
        project_id: project.clone(),
        operation: operation.into(),
        execution_id: None,
        task_id: None,
        base_revision: None,
        deadline_ms: Some(10_000),
        dry_run: false,
        trace_context: None,
        input,
    }
}
