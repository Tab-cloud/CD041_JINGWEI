use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use jingwei_daemon::{Daemon, InMemoryClient};
use jingwei_jdp::{
    Effect, ExecutionReconcileInput, ExecutionReconcileOutput, ExecutionStatus,
    JDP_PROTOCOL_VERSION, JdpClient, JdpRequest, JdpResponse, Permission, ProjectCreateInput,
    ProjectInfo, TaskRef, TaskState, TaskWaitInput, ToolDescriptor, operation,
};
use jingwei_jir::{ExecutionId, IdempotencyKey, ProjectId, RequestId, TaskId, WorkspaceId};
use jingwei_kernel::{
    DomainToolModule, ExtensionRegistryBuilder, ToolContext, ToolRequest, ToolResult,
};
use jingwei_store::{DurableTaskRecord, WorkspaceStore};
use serde_json::{Value, json};
use tempfile::tempdir;

struct SlowTasks;

#[async_trait]
impl DomainToolModule for SlowTasks {
    fn module_name(&self) -> &'static str {
        "slice-2-slow-tasks"
    }

    fn descriptors(&self) -> Vec<ToolDescriptor> {
        vec![
            descriptor("test.slow-internal", Permission::CandidateWrite),
            descriptor("test.slow-external", Permission::ExternalProcess),
        ]
    }

    async fn execute(
        &self,
        _context: ToolContext,
        request: ToolRequest,
    ) -> Result<ToolResult, String> {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        Ok(ToolResult {
            candidate_operations: Vec::new(),
            data: json!({"operation": request.name, "terminal": "succeeded"}),
            artifact_refs: Vec::new(),
            evidence_refs: Vec::new(),
        })
    }
}

#[tokio::test]
async fn request_deadline_quarantines_the_task_and_records_a_reconcilable_failure() {
    let directory = tempdir().unwrap();
    let workspace = WorkspaceId::new("ws-task-deadline").unwrap();
    let project = ProjectId::new("project-task-deadline").unwrap();
    let registry = ExtensionRegistryBuilder::new()
        .tool_module(SlowTasks)
        .unwrap()
        .build();
    let daemon = Arc::new(Daemon::create(directory.path(), workspace.clone(), registry).unwrap());
    let client = InMemoryClient::connect(daemon, "deadline-client").unwrap();
    let created: JdpResponse<ProjectInfo> = client
        .send(request(
            &workspace,
            &project,
            operation::PROJECT_CREATE,
            "deadline-create",
            None,
            json!(ProjectCreateInput {
                name: "Deadline recovery".into()
            }),
        ))
        .await
        .unwrap();
    assert!(created.is_success());

    let task_id = TaskId::new("task-deadline").unwrap();
    let execution_id = ExecutionId::new("execution-deadline").unwrap();
    let idempotency_key = IdempotencyKey::new("idem-deadline").unwrap();
    let mut slow = request(
        &workspace,
        &project,
        "test.slow-external",
        "deadline",
        None,
        json!({}),
    );
    slow.execution_id = Some(execution_id.clone());
    slow.task_id = Some(task_id.clone());
    slow.idempotency_key = idempotency_key.clone();
    slow.deadline_ms = Some(25);
    let response: JdpResponse<Value> = client.send(slow).await.unwrap();
    let JdpResponse::Failure(failure) = response else {
        panic!("deadline must fail closed")
    };
    assert_eq!(failure.error.code, "deadline_exceeded");
    assert_eq!(failure.execution_id, Some(execution_id.clone()));

    let task: JdpResponse<TaskState> = client
        .send(request(
            &workspace,
            &project,
            operation::TASK_GET,
            "deadline-task-get",
            None,
            json!(TaskRef {
                task_id: task_id.clone(),
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(task) = task else {
        panic!("deadline task state must be readable")
    };
    assert_eq!(task.data.state, "needs-reconciliation");
    assert!(task.data.terminal);
    assert!(task.data.quarantine_reason.is_some());

    let reconciled: JdpResponse<ExecutionReconcileOutput> = client
        .send(request(
            &workspace,
            &project,
            operation::EXECUTION_RECONCILE,
            "deadline-reconcile",
            None,
            json!(ExecutionReconcileInput {
                execution_id,
                idempotency_key,
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(reconciled) = reconciled else {
        panic!("deadline execution must reconcile")
    };
    assert_eq!(reconciled.data.status, ExecutionStatus::Failed);
    assert!(reconciled.data.response_hash.is_some());
}

fn descriptor(name: &str, permission: Permission) -> ToolDescriptor {
    ToolDescriptor {
        name: name.into(),
        description: "bounded cancellation test tool".into(),
        input_schema: json!({"type": "object"}),
        output_schema: json!({"type": "object"}),
        permission,
        effect: Effect::ReadOnly,
        resource_claim_template: vec!["workspace:{workspaceId}".into()],
        idempotent: true,
        requires_base_revision: false,
        cancellable: true,
        default_timeout_ms: 30_000,
        protocol_version: JDP_PROTOCOL_VERSION.into(),
        schema_version: "0.2".into(),
    }
}

#[tokio::test]
async fn cancellation_wait_and_external_quarantine_are_fail_closed() {
    let directory = tempdir().unwrap();
    let workspace = WorkspaceId::new("ws-task-control").unwrap();
    let project = ProjectId::new("project-task-control").unwrap();
    let registry = ExtensionRegistryBuilder::new()
        .tool_module(SlowTasks)
        .unwrap()
        .build();
    let daemon = Arc::new(Daemon::create(directory.path(), workspace.clone(), registry).unwrap());
    let setup = InMemoryClient::connect(daemon.clone(), "task-setup").unwrap();
    let created: JdpResponse<ProjectInfo> = setup
        .send(request(
            &workspace,
            &project,
            operation::PROJECT_CREATE,
            "create",
            None,
            json!(ProjectCreateInput {
                name: "Task control".into()
            }),
        ))
        .await
        .unwrap();
    let JdpResponse::Success(created) = created else {
        panic!("project creation failed")
    };

    for (suffix, operation_name, expected_code, expected_state) in [
        (
            "internal",
            "test.slow-internal",
            "task_cancelled",
            "cancelled",
        ),
        (
            "external",
            "test.slow-external",
            "task_needs_reconciliation",
            "needs-reconciliation",
        ),
    ] {
        let task_id = TaskId::new(format!("task-{suffix}")).unwrap();
        let execution_id = ExecutionId::new(format!("execution-{suffix}")).unwrap();
        let worker = InMemoryClient::connect(daemon.clone(), "task-worker").unwrap();
        let mut work_request = request(
            &workspace,
            &project,
            operation_name,
            suffix,
            Some(created.data.current_revision.to_string()),
            json!({}),
        );
        work_request.execution_id = Some(execution_id);
        work_request.task_id = Some(task_id.clone());
        let running = tokio::spawn(async move {
            worker
                .send::<_, Value>(work_request)
                .await
                .expect("slow request transport")
        });

        let controller = InMemoryClient::connect(daemon.clone(), "task-controller").unwrap();
        let running_state =
            wait_until_registered(&controller, &workspace, &project, &task_id).await;
        assert!(!running_state.terminal);
        let timed: JdpResponse<TaskState> = controller
            .send(request(
                &workspace,
                &project,
                operation::TASK_WAIT,
                &format!("wait-short-{suffix}"),
                None,
                json!(TaskWaitInput {
                    task_id: task_id.clone(),
                    timeout_ms: 5,
                }),
            ))
            .await
            .unwrap();
        let JdpResponse::Success(timed) = timed else {
            panic!("task.wait failed")
        };
        assert!(!timed.data.terminal);

        let cancelled: JdpResponse<TaskState> = controller
            .send(request(
                &workspace,
                &project,
                operation::TASK_CANCEL,
                &format!("cancel-{suffix}"),
                None,
                json!(TaskRef {
                    task_id: task_id.clone(),
                }),
            ))
            .await
            .unwrap();
        assert!(matches!(cancelled, JdpResponse::Success(_)));
        let response = running.await.unwrap();
        let JdpResponse::Failure(failure) = response else {
            panic!("cancelled task must not report success")
        };
        assert_eq!(failure.error.code, expected_code);

        let terminal: JdpResponse<TaskState> = controller
            .send(request(
                &workspace,
                &project,
                operation::TASK_WAIT,
                &format!("wait-terminal-{suffix}"),
                None,
                json!(TaskWaitInput {
                    task_id: task_id.clone(),
                    timeout_ms: 1_000,
                }),
            ))
            .await
            .unwrap();
        let JdpResponse::Success(terminal) = terminal else {
            panic!("terminal task.wait failed")
        };
        assert!(terminal.data.terminal);
        assert_eq!(terminal.data.state, expected_state);
        if suffix == "external" {
            assert!(terminal.data.quarantine_reason.is_some());
            assert!(!terminal.data.repair_actions.is_empty());
        }
    }
}

#[test]
fn daemon_restart_quarantines_unfinished_durable_task() {
    let directory = tempdir().unwrap();
    let workspace = WorkspaceId::new("ws-task-restart").unwrap();
    let project = ProjectId::new("project-task-restart").unwrap();
    let task_id = TaskId::new("task-interrupted").unwrap();
    let store = WorkspaceStore::create(directory.path(), workspace.clone()).unwrap();
    store
        .put_task(&DurableTaskRecord {
            task_id: task_id.clone(),
            project_id: project,
            execution_id: ExecutionId::new("execution-interrupted").unwrap(),
            operation: "test.slow-external".into(),
            state: "running".into(),
            cancellable: true,
            external_side_effects: true,
            terminal: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            quarantine_reason: None,
            repair_actions: Vec::new(),
        })
        .unwrap();
    drop(store);

    let daemon = Daemon::open(directory.path(), ExtensionRegistryBuilder::new().build()).unwrap();
    assert!(
        daemon
            .readiness()
            .recovery_summary
            .contains("interruptedTasks=1")
    );
    drop(daemon);
    let reopened = WorkspaceStore::open(directory.path()).unwrap();
    let task = reopened.task(&task_id).unwrap().unwrap();
    assert!(task.terminal);
    assert_eq!(task.state, "needs-reconciliation");
    assert!(task.quarantine_reason.is_some());
}

async fn wait_until_registered(
    client: &InMemoryClient,
    workspace: &WorkspaceId,
    project: &ProjectId,
    task_id: &TaskId,
) -> TaskState {
    for attempt in 0..100 {
        let response: JdpResponse<TaskState> = client
            .send(request(
                workspace,
                project,
                operation::TASK_GET,
                &format!("task-get-{attempt}"),
                None,
                json!(TaskRef {
                    task_id: task_id.clone(),
                }),
            ))
            .await
            .unwrap();
        if let JdpResponse::Success(success) = response {
            return success.data;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("task was not registered")
}

fn request(
    workspace: &WorkspaceId,
    project: &ProjectId,
    operation_name: &str,
    key: &str,
    base_revision: Option<String>,
    input: Value,
) -> JdpRequest<Value> {
    JdpRequest {
        protocol_version: JDP_PROTOCOL_VERSION.into(),
        request_id: RequestId::new(format!("request-{key}")).unwrap(),
        idempotency_key: IdempotencyKey::new(format!("idem-{key}")).unwrap(),
        workspace_id: workspace.clone(),
        project_id: project.clone(),
        operation: operation_name.into(),
        execution_id: None,
        task_id: None,
        base_revision: base_revision.map(|revision| revision.parse().unwrap()),
        deadline_ms: Some(30_000),
        dry_run: false,
        trace_context: None,
        input,
    }
}
