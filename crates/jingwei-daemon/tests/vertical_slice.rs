use std::sync::Arc;

use jingwei_daemon::{Daemon, InMemoryClient};
use jingwei_jdp::{
    CompileJdlInput, CompileJdlOutput, EditCommandGroup, EditCommitOutput, EditPreviewOutput,
    EditPublishOutput, EditSessionInfo, EditSessionOpenInput, EditSessionRef,
    ExecutionReconcileInput, ExecutionReconcileOutput, ExecutionStatus, JDP_PROTOCOL_VERSION,
    JdpClient, JdpRequest, JdpResponse, PatchApplyInput, PatchInput, PatchOperation, PatchOutput,
    Precondition, ProjectCreateInput, ProjectInfo, ProposalDecisionInput, ProposalDecisionOutput,
    QueryInput, QueryOutput, SemanticEditCommand, operation,
};
use jingwei_jir::{
    CanonicalValue, ExecutionId, IdempotencyKey, ProjectId, ProposalAction, ProposalId, RequestId,
    RevisionId, WorkspaceId,
};
use jingwei_kernel::ExtensionRegistryBuilder;
use serde_json::json;
use tempfile::tempdir;

const SENSOR_JDL: &str = r#"project sensor {
  interface input;
  module core {
    component U1: "controller";
    net VCC;
    property supply: voltage = 3.3V;
    connect U1.VCC -> VCC;
    assert U1 connected VCC;
  }
}
"#;

#[tokio::test]
async fn cli_daemon_style_slice_is_atomic_idempotent_and_conflict_safe() {
    let directory = tempdir().unwrap();
    let workspace = WorkspaceId::new("ws-vertical").unwrap();
    let project = ProjectId::new("project-vertical").unwrap();
    let daemon = Arc::new(
        Daemon::create(
            directory.path(),
            workspace.clone(),
            ExtensionRegistryBuilder::new().build(),
        )
        .unwrap(),
    );
    let client = InMemoryClient::connect(daemon, "integration-test").unwrap();
    let initial = create_project(&client, &workspace, &project, "create").await;
    let compiled_revision = compile_jdl(
        &client, &workspace, &project, initial, "compile", SENSOR_JDL,
    )
    .await;
    let component = query_component(&client, &workspace, &project, &compiled_revision).await;
    let patch = PatchApplyInput {
        proposal_id: None,
        patch: PatchInput {
            operations: vec![PatchOperation::SetProperty {
                entity_id: component.id.clone(),
                key: "label".into(),
                value: CanonicalValue::new(json!("main controller")).unwrap(),
            }],
            preconditions: vec![
                Precondition::EntityExists {
                    entity_id: component.id.clone(),
                },
                Precondition::ObjectEditable {
                    entity_id: component.id.clone(),
                },
            ],
            expected_postconditions: vec!["graph.integrity".into()],
        },
    };
    let execution_id = ExecutionId::new("execution-apply-once").unwrap();
    let mut apply_request = request(
        &workspace,
        &project,
        operation::DESIGN_PATCH_APPLY,
        Some(compiled_revision.clone()),
        "apply-once",
        patch.clone(),
    );
    apply_request.execution_id = Some(execution_id.clone());
    let first: JdpResponse<PatchOutput> = client.send(apply_request.clone()).await.unwrap();
    let first_output = success(first);
    let replay: JdpResponse<PatchOutput> = client.send(apply_request.clone()).await.unwrap();
    let replay_output = success(replay);
    assert_eq!(first_output.revision, replay_output.revision);
    assert!(!replay_output.idempotent_replay);

    let reconciled: JdpResponse<ExecutionReconcileOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::EXECUTION_RECONCILE,
            None,
            "reconcile-apply",
            ExecutionReconcileInput {
                execution_id: execution_id.clone(),
                idempotency_key: apply_request.idempotency_key.clone(),
            },
        ))
        .await
        .unwrap();
    let reconciled = success(reconciled);
    assert!(matches!(reconciled.status, ExecutionStatus::Succeeded));
    assert!(reconciled.response_hash.is_some());
    assert_eq!(
        reconciled
            .result
            .as_ref()
            .and_then(|result| result.get("executionId"))
            .and_then(serde_json::Value::as_str),
        Some(execution_id.as_str())
    );

    let idempotent_new_execution: JdpResponse<PatchOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::DESIGN_PATCH_APPLY,
            Some(compiled_revision.clone()),
            "apply-once",
            patch.clone(),
        ))
        .await
        .unwrap();
    assert!(success(idempotent_new_execution).idempotent_replay);

    let conflicting_patch = PatchApplyInput {
        proposal_id: None,
        patch: PatchInput {
            operations: vec![PatchOperation::SetProperty {
                entity_id: component.id,
                key: "label".into(),
                value: CanonicalValue::new(json!("different input")).unwrap(),
            }],
            preconditions: Vec::new(),
            expected_postconditions: Vec::new(),
        },
    };
    let conflict: JdpResponse<PatchOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::DESIGN_PATCH_APPLY,
            Some(compiled_revision.clone()),
            "apply-once",
            conflicting_patch,
        ))
        .await
        .unwrap();
    assert_failure_code(conflict, "idempotency_conflict");

    accept_proposal(
        &client,
        &workspace,
        &project,
        first_output.proposal_id,
        "accept-patch",
    )
    .await;

    let stale: JdpResponse<PatchOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::DESIGN_PATCH_APPLY,
            Some(compiled_revision),
            "stale-key",
            patch,
        ))
        .await
        .unwrap();
    assert_failure_code(stale, "stale_base");
}

#[tokio::test]
async fn draft_edits_do_not_move_project_until_publish_and_support_undo_redo() {
    let directory = tempdir().unwrap();
    let workspace = WorkspaceId::new("ws-draft").unwrap();
    let project = ProjectId::new("project-draft").unwrap();
    let daemon = Arc::new(
        Daemon::create(
            directory.path(),
            workspace.clone(),
            ExtensionRegistryBuilder::new().build(),
        )
        .unwrap(),
    );
    let client = InMemoryClient::connect(daemon, "draft-test").unwrap();
    let initial = create_project(&client, &workspace, &project, "draft-create").await;
    let project_revision = compile_jdl(
        &client,
        &workspace,
        &project,
        initial,
        "draft-compile",
        "project draft { component U1: \"controller\"; }",
    )
    .await;
    let entity_id = query_component(&client, &workspace, &project, &project_revision)
        .await
        .id;
    let opened: JdpResponse<EditSessionInfo> = client
        .send(request(
            &workspace,
            &project,
            operation::EDIT_SESSION_OPEN,
            Some(project_revision.clone()),
            "draft-open",
            EditSessionOpenInput {
                base_revision: project_revision.clone(),
                lease_seconds: 600,
            },
        ))
        .await
        .unwrap();
    let session = success(opened);
    let mut group = EditCommandGroup {
        edit_session_id: session.edit_session_id.clone(),
        base_draft_revision: session.draft_head_revision,
        commands: vec![SemanticEditCommand {
            kind: "core.set_property".into(),
            payload: json!({"entityId": entity_id, "key": "label", "value": "edited"}),
        }],
        expected_postconditions: vec!["graph.integrity".into()],
        preview_hash: None,
    };
    let preview: JdpResponse<EditPreviewOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::EDIT_COMMAND_PREVIEW,
            None,
            "draft-preview",
            group.clone(),
        ))
        .await
        .unwrap();
    group.preview_hash = Some(success(preview).preview_hash);
    let committed: JdpResponse<EditCommitOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::EDIT_COMMAND_COMMIT,
            Some(group.base_draft_revision.clone()),
            "draft-commit",
            group,
        ))
        .await
        .unwrap();
    let committed = success(committed);
    assert_eq!(
        project_info(&client, &workspace, &project, "still-clean")
            .await
            .current_revision,
        project_revision
    );

    let undone: JdpResponse<EditCommitOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::EDIT_UNDO,
            Some(committed.draft_revision),
            "draft-undo",
            EditSessionRef {
                edit_session_id: session.edit_session_id.clone(),
            },
        ))
        .await
        .unwrap();
    let undone = success(undone);
    let redone: JdpResponse<EditCommitOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::EDIT_REDO,
            Some(undone.draft_revision),
            "draft-redo",
            EditSessionRef {
                edit_session_id: session.edit_session_id.clone(),
            },
        ))
        .await
        .unwrap();
    success(redone);
    let published: JdpResponse<EditPublishOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::EDIT_PUBLISH,
            Some(project_revision.clone()),
            "draft-publish",
            EditSessionRef {
                edit_session_id: session.edit_session_id,
            },
        ))
        .await
        .unwrap();
    let published = success(published);
    assert_eq!(
        project_info(&client, &workspace, &project, "after-publish")
            .await
            .current_revision,
        project_revision
    );
    let accepted = accept_proposal(
        &client,
        &workspace,
        &project,
        published.proposal_id,
        "accept-draft",
    )
    .await;
    assert_eq!(accepted.project_head, published.revision);
}

async fn create_project(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    key: &str,
) -> RevisionId {
    let response: JdpResponse<ProjectInfo> = client
        .send(request(
            workspace,
            project,
            operation::PROJECT_CREATE,
            None,
            key,
            ProjectCreateInput {
                name: "Test".into(),
            },
        ))
        .await
        .unwrap();
    success(response).current_revision
}

async fn compile_jdl(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    base: RevisionId,
    key: &str,
    source: &str,
) -> RevisionId {
    let response: JdpResponse<CompileJdlOutput> = client
        .send(request(
            workspace,
            project,
            operation::DESIGN_COMPILE_JDL,
            Some(base),
            key,
            CompileJdlInput {
                source_name: "test.jdl".into(),
                source: source.into(),
            },
        ))
        .await
        .unwrap();
    let compiled = success(response);
    let revision = compiled.revision.clone();
    accept_proposal(
        client,
        workspace,
        project,
        compiled.proposal_id,
        &format!("{key}-accept"),
    )
    .await;
    revision
}

async fn accept_proposal(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    proposal_id: ProposalId,
    key: &str,
) -> ProposalDecisionOutput {
    let input = ProposalDecisionInput {
        proposal_id,
        action: ProposalAction::Accept,
        reason: Some("integration review".into()),
    };
    let response: JdpResponse<ProposalDecisionOutput> = client
        .send(request(
            workspace,
            project,
            operation::CHANGE_PROPOSAL_DECIDE,
            None,
            key,
            input.clone(),
        ))
        .await
        .unwrap();
    let accepted = success(response);
    assert!(!accepted.idempotent_replay);
    let replay: JdpResponse<ProposalDecisionOutput> = client
        .send(request(
            workspace,
            project,
            operation::CHANGE_PROPOSAL_DECIDE,
            None,
            key,
            input,
        ))
        .await
        .unwrap();
    assert!(success(replay).idempotent_replay);
    accepted
}

async fn query_component(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    revision: &RevisionId,
) -> jingwei_jir::JirEntity {
    let response: JdpResponse<QueryOutput> = client
        .send(request(
            workspace,
            project,
            operation::DESIGN_QUERY,
            None,
            &format!("query-{revision}"),
            QueryInput {
                revision: Some(revision.clone()),
                kind: Some("core.component".into()),
                ids: Vec::new(),
                limit: 10,
                cursor: None,
            },
        ))
        .await
        .unwrap();
    success(response).entities.into_iter().next().unwrap()
}

async fn project_info(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    key: &str,
) -> ProjectInfo {
    let response: JdpResponse<ProjectInfo> = client
        .send(request(
            workspace,
            project,
            operation::PROJECT_GET,
            None,
            key,
            json!({}),
        ))
        .await
        .unwrap();
    success(response)
}

fn request<T: Clone>(
    workspace: &WorkspaceId,
    project: &ProjectId,
    operation: &str,
    base_revision: Option<RevisionId>,
    key: &str,
    input: T,
) -> JdpRequest<T> {
    JdpRequest {
        protocol_version: JDP_PROTOCOL_VERSION.into(),
        request_id: RequestId::new(format!("request-{key}")).unwrap(),
        idempotency_key: IdempotencyKey::new(format!("idem-{key}")).unwrap(),
        workspace_id: workspace.clone(),
        project_id: project.clone(),
        operation: operation.into(),
        execution_id: None,
        task_id: None,
        base_revision,
        deadline_ms: Some(10_000),
        dry_run: false,
        trace_context: None,
        input,
    }
}

fn success<T>(response: JdpResponse<T>) -> T {
    match response {
        JdpResponse::Success(success) => success.data,
        JdpResponse::Failure(failure) => panic!("unexpected failure: {}", failure.error.summary),
    }
}

fn assert_failure_code<T>(response: JdpResponse<T>, expected: &str) {
    match response {
        JdpResponse::Failure(failure) => assert_eq!(failure.error.code, expected),
        JdpResponse::Success(_) => panic!("expected failure {expected}"),
    }
}
