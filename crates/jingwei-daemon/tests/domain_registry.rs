use std::sync::Arc;

use jingwei_daemon::{Daemon, InMemoryClient, production_registry};
use jingwei_jdp::{
    CapabilityState, JDP_PROTOCOL_VERSION, JdpClient, JdpRequest, JdpResponse, ProjectCreateInput,
    ProjectInfo, WorkspaceCapabilities, operation,
};
use jingwei_jir::{IdempotencyKey, ProjectId, RequestId, WorkspaceId};
use jingwei_kernel::ToolResult;
use serde_json::{Value, json};
use tempfile::tempdir;

#[tokio::test]
async fn production_daemon_advertises_and_executes_domain_tools() {
    let directory = tempdir().unwrap();
    let workspace = WorkspaceId::new("ws-domain-registry").unwrap();
    let project = ProjectId::new("project-domain-registry").unwrap();
    let daemon = Arc::new(
        Daemon::create(
            directory.path(),
            workspace.clone(),
            production_registry().unwrap(),
        )
        .unwrap(),
    );
    let client = InMemoryClient::connect(daemon, "domain-registry-test").unwrap();

    let created: JdpResponse<ProjectInfo> = client
        .send(request(
            &workspace,
            &project,
            operation::PROJECT_CREATE,
            "create",
            serde_json::to_value(ProjectCreateInput {
                name: "Domain registry".into(),
            })
            .unwrap(),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(created) = created else {
        panic!("project create must succeed");
    };

    let capabilities: JdpResponse<WorkspaceCapabilities> = client
        .send(request(
            &workspace,
            &project,
            operation::WORKSPACE_CAPABILITIES,
            "capabilities",
            json!({}),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(capabilities) = capabilities else {
        panic!("capabilities must succeed");
    };
    for operation in [
        "requirement_compile",
        "risk_classify",
        "manufacturing_profile_validate",
        "release_readiness",
    ] {
        assert!(matches!(
            capabilities.data.operations.get(operation),
            Some(CapabilityState::Available)
        ));
    }

    let intent: Value = serde_json::from_str(include_str!(
        "../../../testdata/logical/golden/design-intent.json"
    ))
    .unwrap();
    let compiled: JdpResponse<ToolResult> = client
        .send(JdpRequest {
            base_revision: Some(created.data.current_revision.clone()),
            ..request(
                &workspace,
                &project,
                "requirement_compile",
                "requirement-compile",
                intent,
            )
        })
        .await
        .unwrap();
    let JdpResponse::Success(compiled) = compiled else {
        panic!("logical domain tool must execute");
    };
    assert_eq!(compiled.data.data["baselineAllowed"], true);
    assert!(compiled.data.candidate_operations.is_empty());

    let readiness: JdpResponse<ToolResult> = client
        .send(request(
            &workspace,
            &project,
            "release_readiness",
            "release-readiness",
            json!({"candidateId": "candidate-test"}),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(readiness) = readiness else {
        panic!("physical domain tool must execute");
    };
    assert_eq!(readiness.data.data["productionReleaseAvailable"], false);
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
        base_revision: None,
        deadline_ms: Some(10_000),
        dry_run: false,
        input,
    }
}
