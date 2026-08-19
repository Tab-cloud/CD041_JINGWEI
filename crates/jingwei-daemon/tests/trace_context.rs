use jingwei_daemon::Daemon;
use jingwei_jdp::{JDP_PROTOCOL_VERSION, JdpRequest, JdpResponse, TraceContext, operation};
use jingwei_jir::{IdempotencyKey, ProjectId, RequestId, WorkspaceId};
use jingwei_kernel::ExtensionRegistryBuilder;
use tempfile::tempdir;

#[tokio::test]
async fn daemon_validates_parent_and_records_a_child_span() {
    let directory = tempdir().unwrap();
    let workspace_id = WorkspaceId::new("ws-trace-context").unwrap();
    let daemon = Daemon::create(
        directory.path(),
        workspace_id.clone(),
        ExtensionRegistryBuilder::new().build(),
    )
    .unwrap();
    let parent = TraceContext {
        trace_id: "0123456789abcdef0123456789abcdef".into(),
        span_id: "fedcba9876543210".into(),
        trace_flags: 1,
    };

    let response = daemon
        .dispatch(request(workspace_id.clone(), Some(parent.clone())))
        .await;
    assert!(response.is_success());

    let spans = daemon.trace_spans();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].trace_id, parent.trace_id);
    assert_eq!(spans[0].parent_span_id, parent.span_id);
    assert_eq!(spans[0].span_id.len(), 16);
    assert_ne!(spans[0].span_id, spans[0].parent_span_id);
    assert_eq!(spans[0].operation, operation::WORKSPACE_CAPABILITIES);
    assert_eq!(spans[0].status, "ok");

    let invalid = TraceContext {
        trace_id: "00000000000000000000000000000000".into(),
        span_id: "fedcba9876543210".into(),
        trace_flags: 1,
    };
    let response = daemon.dispatch(request(workspace_id, Some(invalid))).await;
    let JdpResponse::Failure(failure) = response else {
        panic!("an all-zero trace id must be rejected");
    };
    assert_eq!(failure.error.code, "invalid_trace_context");
    assert_eq!(daemon.trace_spans().len(), 1);
}

fn request(
    workspace_id: WorkspaceId,
    trace_context: Option<TraceContext>,
) -> JdpRequest<serde_json::Value> {
    JdpRequest {
        protocol_version: JDP_PROTOCOL_VERSION.into(),
        request_id: RequestId::new("request-trace-context").unwrap(),
        idempotency_key: IdempotencyKey::new("idem-trace-context").unwrap(),
        workspace_id,
        project_id: ProjectId::new("project-trace-context").unwrap(),
        operation: operation::WORKSPACE_CAPABILITIES.into(),
        base_revision: None,
        deadline_ms: Some(5_000),
        dry_run: false,
        trace_context,
        input: serde_json::json!({}),
    }
}
