use std::{fs, sync::Arc};

use jingwei_daemon::{Daemon, InMemoryClient};
use jingwei_jdp::{
    JDP_PROTOCOL_VERSION, JdpClient, JdpRequest, ProjectCreateInput, ProjectInfo, operation,
};
use jingwei_jir::{IdempotencyKey, ProjectId, RequestId, WorkspaceId};
use jingwei_kernel::ExtensionRegistryBuilder;
use tempfile::tempdir;

#[tokio::test]
async fn daemon_epoch_is_durable_and_commit_correlation_names_the_exact_process() {
    let directory = tempdir().unwrap();
    let workspace = WorkspaceId::new("ws-daemon-epoch").unwrap();
    let project = ProjectId::new("project-daemon-epoch").unwrap();
    let request_id = RequestId::new("request-daemon-epoch").unwrap();

    let first_health = {
        let daemon = Arc::new(
            Daemon::create(
                directory.path(),
                workspace.clone(),
                ExtensionRegistryBuilder::new().build(),
            )
            .unwrap(),
        );
        let health = daemon.health();
        let client = InMemoryClient::connect(daemon.clone(), "epoch-test").unwrap();
        let response = client
            .send::<_, ProjectInfo>(JdpRequest {
                protocol_version: JDP_PROTOCOL_VERSION.into(),
                request_id: request_id.clone(),
                idempotency_key: IdempotencyKey::new("idem-daemon-epoch").unwrap(),
                workspace_id: workspace.clone(),
                project_id: project,
                operation: operation::PROJECT_CREATE.into(),
                execution_id: None,
                task_id: None,
                base_revision: None,
                deadline_ms: Some(10_000),
                dry_run: false,
                trace_context: None,
                input: ProjectCreateInput {
                    name: "Daemon epoch".into(),
                },
            })
            .await
            .unwrap();
        assert!(matches!(response, jingwei_jdp::JdpResponse::Success(_)));

        let project_root = fs::read_dir(directory.path().join(".jingwei/projects"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let journal_path = fs::read_dir(project_root.join("journal"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let journal: serde_json::Value =
            serde_json::from_slice(&fs::read(journal_path).unwrap()).unwrap();
        assert_eq!(
            journal["correlation"]["daemonInstanceId"],
            health.daemon_instance
        );
        assert_eq!(journal["correlation"]["daemonEpoch"], health.daemon_epoch);
        assert_eq!(journal["correlation"]["sourceId"], request_id.as_str());
        health
    };

    let second = Daemon::open(directory.path(), ExtensionRegistryBuilder::new().build()).unwrap();
    let second_health = second.health();
    assert_ne!(second_health.daemon_instance, first_health.daemon_instance);
    assert_eq!(second_health.daemon_epoch, first_health.daemon_epoch + 1);
}
