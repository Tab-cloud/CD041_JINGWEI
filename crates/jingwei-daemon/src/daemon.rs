//! Daemon lifecycle, request dispatch, capability negotiation, and error mapping.

use std::{
    collections::{BTreeMap, HashMap},
    panic::AssertUnwindSafe,
    path::Path,
    sync::Mutex,
    time::Duration,
};

use chrono::Utc;
use futures_util::FutureExt;
use jingwei_jdp::{
    ArtifactGetInput, ArtifactGetOutput, CapabilityState, CompileJdlInput, DiffInput,
    EditCommandGroup, EditSessionOpenInput, EditSessionRef, ErrorDetail, EventsInput,
    ExecutionReconcileInput, ExecutionReconcileOutput, ExecutionStatus, ExplainInput,
    FindingSummary, HandshakeRequest, HandshakeResponse, JDP_PROTOCOL_VERSION, JdpFailure,
    JdpRequest, JdpResponse, JdpSuccess, PatchApplyInput, PatchInput, Permission,
    ProjectCreateInput, ProposalDecisionInput, QueryInput, SuggestedAction, SuggestedActionKind,
    TaskRef, TaskState, TaskWaitInput, TraceContext, ValidateInput, WorkspaceCapabilities,
    operation, tool_catalog,
};
use jingwei_jir::{ExecutionId, ProjectId, RevisionId, TaskId, WorkspaceId};
use jingwei_kernel::{ExtensionRegistry, Kernel, KernelError, RequestContext};
use jingwei_store::{DurableTaskRecord, ExecutionRecord, StoreError, WorkspaceStore, WriterLease};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::watch;
use uuid::Uuid;

/// Structured liveness response.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub status: String,
    pub daemon_instance: String,
    pub daemon_epoch: u64,
    pub build: String,
}

/// Structured readiness response.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessStatus {
    pub ready: bool,
    pub workspace_id: WorkspaceId,
    pub read_write: bool,
    pub recovery_summary: String,
}

/// Sanitized daemon-side child span proving cross-runtime trace propagation.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
    pub trace_flags: u8,
    pub name: String,
    pub request_id: String,
    pub operation: String,
    pub status: String,
}

/// Running daemon. Its private lease guarantees one writer for the workspace.
pub struct Daemon {
    kernel: Kernel,
    workspace_id: WorkspaceId,
    instance_id: String,
    epoch: u64,
    session_token: String,
    recovery_summary: String,
    trace_spans: Mutex<Vec<DaemonSpan>>,
    active_tasks: Mutex<HashMap<TaskId, watch::Sender<bool>>>,
    _writer_lease: WriterLease,
}

impl Daemon {
    /// Creates or opens a workspace and acquires its single-writer lease.
    pub fn create(
        root: impl AsRef<Path>,
        workspace_id: WorkspaceId,
        registry: ExtensionRegistry,
    ) -> Result<Self, DaemonError> {
        let store = WorkspaceStore::create(root, workspace_id)?;
        Self::from_store(store, registry)
    }

    /// Opens an existing workspace and acquires its single-writer lease.
    pub fn open(root: impl AsRef<Path>, registry: ExtensionRegistry) -> Result<Self, DaemonError> {
        let store = WorkspaceStore::open(root)?;
        Self::from_store(store, registry)
    }

    /// Opens with a launcher-provided high-entropy token delivered over inherited standard input.
    pub fn open_with_token(
        root: impl AsRef<Path>,
        registry: ExtensionRegistry,
        session_token: String,
    ) -> Result<Self, DaemonError> {
        if session_token.len() < 32 || session_token.chars().any(char::is_control) {
            return Err(DaemonError::Authentication);
        }
        let store = WorkspaceStore::open(root)?;
        let mut daemon = Self::from_store(store, registry)?;
        daemon.session_token = session_token;
        Ok(daemon)
    }

    /// Creates a workspace with a launcher-provided token delivered over inherited stdin.
    pub fn create_with_token(
        root: impl AsRef<Path>,
        workspace_id: WorkspaceId,
        registry: ExtensionRegistry,
        session_token: String,
    ) -> Result<Self, DaemonError> {
        if session_token.len() < 32 || session_token.chars().any(char::is_control) {
            return Err(DaemonError::Authentication);
        }
        let store = WorkspaceStore::create(root, workspace_id)?;
        let mut daemon = Self::from_store(store, registry)?;
        daemon.session_token = session_token;
        Ok(daemon)
    }

    fn from_store(store: WorkspaceStore, registry: ExtensionRegistry) -> Result<Self, DaemonError> {
        let instance_id = format!("daemon-{}", Uuid::new_v4());
        let writer_lease = store.acquire_writer(&instance_id)?;
        let recovery = store.recover()?;
        let interrupted_tasks = store.reconcile_incomplete_tasks()?;
        let epoch = store.allocate_daemon_epoch()?;
        let recovery_summary = format!(
            "recovered={}, quarantined={}, removedIncomplete={}, interruptedTasks={}",
            recovery.recovered_transactions,
            recovery.quarantined_transactions,
            recovery.removed_incomplete_transactions,
            interrupted_tasks,
        );
        let workspace_id = store.metadata().workspace_id.clone();
        Ok(Self {
            kernel: Kernel::new(store, registry),
            workspace_id,
            instance_id,
            epoch,
            session_token: format!("session-{}-{}", Uuid::new_v4(), Uuid::new_v4()),
            recovery_summary,
            trace_spans: Mutex::new(Vec::new()),
            active_tasks: Mutex::new(HashMap::new()),
            _writer_lease: writer_lease,
        })
    }

    /// Returns liveness without exposing tokens or workspace design data.
    pub fn health(&self) -> HealthStatus {
        HealthStatus {
            status: "healthy".into(),
            daemon_instance: self.instance_id.clone(),
            daemon_epoch: self.epoch,
            build: env!("CARGO_PKG_VERSION").into(),
        }
    }

    /// Returns readiness after lock acquisition and recovery.
    pub fn readiness(&self) -> ReadinessStatus {
        ReadinessStatus {
            ready: true,
            workspace_id: self.workspace_id.clone(),
            read_write: true,
            recovery_summary: self.recovery_summary.clone(),
        }
    }

    /// Provides the high-entropy bootstrap token only to an in-process trusted launcher/client.
    pub fn bootstrap_token(&self) -> &str {
        &self.session_token
    }

    /// Returns completed sanitized spans for local diagnostics and acceptance evidence.
    pub fn trace_spans(&self) -> Vec<DaemonSpan> {
        self.trace_spans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Authenticates and negotiates a local connection.
    pub fn handshake(&self, request: &HandshakeRequest) -> Result<HandshakeResponse, DaemonError> {
        if request.session_token != self.session_token {
            return Err(DaemonError::Authentication);
        }
        if request.workspace_id != self.workspace_id {
            return Err(DaemonError::WorkspaceMismatch);
        }
        if !request
            .protocol_versions
            .iter()
            .any(|version| version == JDP_PROTOCOL_VERSION)
        {
            return Err(DaemonError::ProtocolMismatch);
        }
        Ok(HandshakeResponse {
            protocol_version: JDP_PROTOCOL_VERSION.into(),
            daemon_build: env!("CARGO_PKG_VERSION").into(),
            daemon_instance: self.instance_id.clone(),
            daemon_epoch: self.epoch,
            workspace_id: self.workspace_id.clone(),
            read_write: true,
            capabilities: tool_catalog()
                .into_iter()
                .chain(self.kernel.extension_tool_descriptors())
                .map(|tool| tool.name)
                .collect(),
        })
    }

    /// Dispatches one bounded request, containing panic and deadline failures at the service boundary.
    pub async fn dispatch(&self, mut request: JdpRequest<Value>) -> JdpResponse<Value> {
        let request_id = request.request_id.clone();
        let execution_id = request
            .execution_id
            .clone()
            .unwrap_or_else(ExecutionId::generate);
        request.execution_id = Some(execution_id.clone());
        let project_id = request.project_id.clone();
        let idempotency_key = request.idempotency_key.clone();
        let operation_name = request.operation.clone();
        let task_id = request.task_id.clone();
        let trace_span = match request.trace_context.as_ref() {
            Some(trace_context) => {
                if let Err(reason) = trace_context.validate() {
                    return failure(
                        request_id,
                        Some(execution_id),
                        detail("invalid_trace_context", "trace-context", reason, false),
                    );
                }
                Some(new_child_span(
                    trace_context,
                    request.request_id.as_str(),
                    &request.operation,
                ))
            }
            None => None,
        };
        let deadline = request.deadline_ms;
        let future = AssertUnwindSafe(self.dispatch_inner(request)).catch_unwind();
        let result = if let Some(milliseconds) = deadline {
            if let Ok(result) = tokio::time::timeout(
                Duration::from_millis(milliseconds.clamp(1, 300_000)),
                future,
            )
            .await
            {
                result
            } else {
                self.quarantine_interrupted_task(
                    &project_id,
                    task_id.as_ref(),
                    "request deadline elapsed before the task proved a terminal state",
                );
                self.complete_trace_span(trace_span, "deadline");
                let response = failure(
                    request_id,
                    Some(execution_id),
                    detail(
                        "deadline_exceeded",
                        "dispatch",
                        "request deadline elapsed",
                        true,
                    ),
                );
                return if operation_name == operation::EXECUTION_RECONCILE {
                    response
                } else {
                    self.record_terminal_execution(
                        &project_id,
                        &idempotency_key,
                        &operation_name,
                        response,
                    )
                };
            }
        } else {
            future.await
        };
        if let Ok(response) = result {
            let status = if response.is_success() { "ok" } else { "error" };
            self.complete_trace_span(trace_span, status);
            response
        } else {
            self.quarantine_interrupted_task(
                &project_id,
                task_id.as_ref(),
                "request panicked before the task proved a terminal state",
            );
            self.complete_trace_span(trace_span, "panic");
            let response = failure(
                request_id,
                Some(execution_id),
                detail(
                    "internal_panic",
                    "service-boundary",
                    "request was safely aborted",
                    false,
                ),
            );
            if operation_name == operation::EXECUTION_RECONCILE {
                response
            } else {
                self.record_terminal_execution(
                    &project_id,
                    &idempotency_key,
                    &operation_name,
                    response,
                )
            }
        }
    }

    fn quarantine_interrupted_task(
        &self,
        project_id: &ProjectId,
        task_id: Option<&TaskId>,
        reason: &str,
    ) {
        let Some(task_id) = task_id else {
            return;
        };
        self.active_tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(task_id);
        let Ok(Some(mut record)) = self.kernel.store().task(task_id) else {
            return;
        };
        if record.project_id != *project_id || record.terminal {
            return;
        }
        record.state = "needs-reconciliation".into();
        record.terminal = true;
        record.updated_at = Utc::now();
        record.quarantine_reason = Some(reason.into());
        record.repair_actions = vec![
            "inspect isolated task state and output hashes".into(),
            "resolve quarantine before retrying with a new execution identity".into(),
        ];
        let _ = self.kernel.store().put_task(&record);
    }

    fn complete_trace_span(&self, span: Option<DaemonSpan>, status: &str) {
        if let Some(mut span) = span {
            span.status = status.into();
            self.trace_spans
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(span);
        }
    }

    async fn dispatch_inner(&self, request: JdpRequest<Value>) -> JdpResponse<Value> {
        let execution_id = request
            .execution_id
            .clone()
            .unwrap_or_else(ExecutionId::generate);
        if request.protocol_version != JDP_PROTOCOL_VERSION {
            return failure(
                request.request_id,
                Some(execution_id),
                detail(
                    "protocol_version_unsupported",
                    "handshake",
                    "unsupported JDP protocol version",
                    false,
                ),
            );
        }
        if request.workspace_id != self.workspace_id {
            return failure(
                request.request_id,
                Some(execution_id),
                detail(
                    "workspace_mismatch",
                    "authorization",
                    "request workspace does not match daemon",
                    false,
                ),
            );
        }
        if request.operation != operation::EXECUTION_RECONCILE {
            match self.kernel.store().execution(&execution_id) {
                Ok(Some(record))
                    if record.project_id == request.project_id
                        && record.idempotency_key == request.idempotency_key
                        && record.operation == request.operation =>
                {
                    let verified_hash = jingwei_jir::canonical_sha256(&record.response);
                    if !matches!(verified_hash, Ok(ref hash) if hash == &record.response_hash) {
                        return failure(
                            request.request_id,
                            Some(execution_id),
                            detail(
                                "execution_ledger_corrupt",
                                "reconciliation",
                                "stored response hash does not match the durable response",
                                false,
                            ),
                        );
                    }
                    return serde_json::from_value(record.response).unwrap_or_else(|error| {
                        failure(
                            request.request_id,
                            Some(execution_id),
                            detail(
                                "execution_ledger_corrupt",
                                "reconciliation",
                                &error.to_string(),
                                false,
                            ),
                        )
                    });
                }
                Ok(Some(_)) => {
                    return failure(
                        request.request_id,
                        Some(execution_id),
                        detail(
                            "execution_identity_conflict",
                            "reconciliation",
                            "executionId was reused with a different project, operation, or idempotency key",
                            false,
                        ),
                    );
                }
                Ok(None) => {}
                Err(error) => {
                    return failure(
                        request.request_id,
                        Some(execution_id),
                        error_detail(&KernelError::from(error)),
                    );
                }
            }
        }
        let context = RequestContext::local("local-jdp-client").for_daemon_command(
            request.request_id.as_str(),
            execution_id.clone(),
            self.instance_id.clone(),
            self.epoch,
        );
        let base = request.base_revision.clone();
        let outcome: Result<(Value, Option<RevisionId>), KernelError> = match request
            .operation
            .as_str()
        {
            operation::PROJECT_CREATE => match parse_input::<ProjectCreateInput>(&request.input) {
                Ok(input) => self
                    .kernel
                    .create_project(
                        request.project_id.clone(),
                        input.name,
                        request.request_id.clone(),
                        request.idempotency_key.clone(),
                        &context,
                    )
                    .and_then(|value| encode(value).map(|value| (value, None))),
                Err(error) => Err(error),
            },
            operation::PROJECT_GET => self
                .kernel
                .project_info(&request.project_id, &context)
                .and_then(|value| encode(value).map(|value| (value, None))),
            operation::DESIGN_COMPILE_JDL => match required_base(&request) {
                Ok(base) => match parse_input::<CompileJdlInput>(&request.input) {
                    Ok(input) => self
                        .kernel
                        .compile_jdl(
                            &request.project_id,
                            &base,
                            &input.source_name,
                            &input.source,
                            request.request_id.clone(),
                            request.idempotency_key.clone(),
                            &context,
                        )
                        .await
                        .and_then(|commit| {
                            let revision = commit.output.revision.clone();
                            encode(commit.output).map(|value| (value, Some(revision)))
                        }),
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            },
            operation::DESIGN_QUERY => parse_input::<QueryInput>(&request.input)
                .and_then(|input| self.kernel.query(&request.project_id, &input, &context))
                .and_then(|value| encode(value).map(|value| (value, None))),
            operation::DESIGN_EXPLAIN => parse_input::<ExplainInput>(&request.input)
                .and_then(|input| {
                    self.kernel.explain(
                        &request.project_id,
                        input.revision.as_ref(),
                        &input.entity_id,
                        &context,
                    )
                })
                .and_then(|value| encode(value).map(|value| (value, None))),
            operation::DESIGN_PATCH_PROPOSE => match required_base(&request) {
                Ok(base) => match parse_input::<PatchInput>(&request.input) {
                    Ok(input) => self
                        .kernel
                        .propose_patch(&request.project_id, &base, &input, &context)
                        .await
                        .and_then(|value| encode(value).map(|value| (value, None))),
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            },
            operation::DESIGN_PATCH_APPLY => match required_base(&request) {
                Ok(base) => match parse_input::<PatchApplyInput>(&request.input) {
                    Ok(input) => self
                        .kernel
                        .apply_patch(
                            &request.project_id,
                            &base,
                            &input.patch,
                            request.request_id.clone(),
                            request.idempotency_key.clone(),
                            &context,
                        )
                        .await
                        .and_then(|value| {
                            let revision = value.revision.clone();
                            encode(value).map(|value| (value, Some(revision)))
                        }),
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            },
            operation::DESIGN_DIFF => parse_input::<DiffInput>(&request.input)
                .and_then(|input| {
                    self.kernel
                        .diff(&request.project_id, &input.left, &input.right, &context)
                })
                .and_then(|value| encode(value).map(|value| (value, None))),
            operation::DESIGN_VALIDATE => match parse_input::<ValidateInput>(&request.input) {
                Ok(input) => self
                    .kernel
                    .validate(&request.project_id, &input, &context)
                    .await
                    .and_then(|value| encode(value).map(|value| (value, None))),
                Err(error) => Err(error),
            },
            operation::EDIT_SESSION_OPEN => {
                match parse_input::<EditSessionOpenInput>(&request.input) {
                    Ok(input) => self
                        .kernel
                        .open_edit_session(
                            &request.project_id,
                            &input.base_revision,
                            input.lease_seconds,
                            request.request_id.clone(),
                            request.idempotency_key.clone(),
                            &context,
                        )
                        .and_then(|value| {
                            let revision = value.draft_head_revision.clone();
                            encode(value).map(|value| (value, Some(revision)))
                        }),
                    Err(error) => Err(error),
                }
            }
            operation::EDIT_COMMAND_PREVIEW => parse_input::<EditCommandGroup>(&request.input)
                .and_then(|input| {
                    self.kernel
                        .preview_edit(&request.project_id, &input, &context)
                })
                .and_then(|value| encode(value).map(|value| (value, None))),
            operation::EDIT_COMMAND_COMMIT => parse_input::<EditCommandGroup>(&request.input)
                .and_then(|input| {
                    self.kernel.commit_edit(
                        &request.project_id,
                        &input,
                        request.request_id.clone(),
                        request.idempotency_key.clone(),
                        &context,
                    )
                })
                .and_then(|value| {
                    let revision = value.draft_revision.clone();
                    encode(value).map(|value| (value, Some(revision)))
                }),
            operation::EDIT_UNDO | operation::EDIT_REDO => {
                parse_input::<EditSessionRef>(&request.input)
                    .and_then(|input| {
                        if request.operation == operation::EDIT_UNDO {
                            self.kernel.undo_edit(
                                &request.project_id,
                                &input.edit_session_id,
                                request.request_id.clone(),
                                request.idempotency_key.clone(),
                                &context,
                            )
                        } else {
                            self.kernel.redo_edit(
                                &request.project_id,
                                &input.edit_session_id,
                                request.request_id.clone(),
                                request.idempotency_key.clone(),
                                &context,
                            )
                        }
                    })
                    .and_then(|value| {
                        let revision = value.draft_revision.clone();
                        encode(value).map(|value| (value, Some(revision)))
                    })
            }
            operation::EDIT_CHECKPOINT => parse_input::<EditSessionRef>(&request.input)
                .and_then(|input| {
                    self.kernel.checkpoint_edit(
                        &request.project_id,
                        &input.edit_session_id,
                        &context,
                    )
                })
                .and_then(|value| encode(value).map(|value| (value, None))),
            operation::EDIT_PUBLISH => parse_input::<EditSessionRef>(&request.input)
                .and_then(|input| {
                    self.kernel.publish_edit(
                        &request.project_id,
                        &input.edit_session_id,
                        request.request_id.clone(),
                        request.idempotency_key.clone(),
                        &context,
                    )
                })
                .and_then(|value| {
                    let revision = value.revision.clone();
                    encode(value).map(|value| (value, Some(revision)))
                }),
            operation::CHANGE_PROPOSAL_DECIDE => {
                parse_input::<ProposalDecisionInput>(&request.input)
                    .and_then(|input| {
                        self.kernel.decide_change_proposal(
                            &request.project_id,
                            input,
                            &request.idempotency_key,
                            &context,
                        )
                    })
                    .and_then(|value| {
                        let revision = value.project_head.clone();
                        encode(value).map(|value| (value, Some(revision)))
                    })
            }
            operation::EXECUTION_RECONCILE => {
                parse_input::<ExecutionReconcileInput>(&request.input)
                    .and_then(|input| self.reconcile_execution(&request.project_id, input))
                    .and_then(|value| encode(value).map(|value| (value, None)))
            }
            operation::TASK_GET => parse_input::<TaskRef>(&request.input)
                .and_then(|input| self.task_state(&request.project_id, &input.task_id))
                .and_then(|value| encode(value).map(|value| (value, None))),
            operation::TASK_CANCEL => parse_input::<TaskRef>(&request.input)
                .and_then(|input| self.cancel_task(&request.project_id, &input.task_id))
                .and_then(|value| encode(value).map(|value| (value, None))),
            operation::TASK_WAIT => match parse_input::<TaskWaitInput>(&request.input) {
                Ok(input) => self
                    .wait_task(&request.project_id, &input.task_id, input.timeout_ms)
                    .await
                    .and_then(|value| encode(value).map(|value| (value, None))),
                Err(error) => Err(error),
            },
            operation::TASK_EVENTS => parse_input::<EventsInput>(&request.input)
                .and_then(|input| {
                    self.kernel
                        .store()
                        .events_after(&request.project_id, input.after_sequence, input.limit)
                        .map_err(KernelError::from)
                })
                .and_then(|value| encode(value).map(|value| (value, None))),
            operation::ARTIFACT_GET => parse_input::<ArtifactGetInput>(&request.input)
                .and_then(|input| {
                    self.kernel
                        .store()
                        .artifact(&input.artifact_id)
                        .map_err(KernelError::from)
                })
                .and_then(|(record, bytes)| {
                    encode(ArtifactGetOutput {
                        artifact_id: record.id,
                        media_type: record.media_type,
                        sha256: record.sha256,
                        bytes_hex: hex::encode(bytes),
                    })
                    .map(|value| (value, None))
                }),
            operation::WORKSPACE_CAPABILITIES => {
                let operations = tool_catalog()
                    .into_iter()
                    .map(|tool| (tool.name, CapabilityState::Available))
                    .chain(
                        self.kernel
                            .extension_tool_descriptors()
                            .into_iter()
                            .map(|tool| (tool.name, CapabilityState::Available)),
                    )
                    .collect::<BTreeMap<_, _>>();
                encode(WorkspaceCapabilities {
                    protocol_versions: vec![JDP_PROTOCOL_VERSION.into()],
                    schema_versions: vec![jingwei_jir::JIR_SCHEMA_VERSION.into()],
                    operations,
                    read_write: true,
                    transport: if cfg!(windows) {
                        "in-memory,named-pipe".into()
                    } else {
                        "in-memory".into()
                    },
                })
                .map(|value| (value, None))
            }
            _ => {
                let descriptor = self
                    .kernel
                    .extension_tool_descriptors()
                    .into_iter()
                    .find(|descriptor| descriptor.name == request.operation);
                if let Some(descriptor) = descriptor.filter(|descriptor| descriptor.cancellable) {
                    match request.task_id.clone() {
                        Some(task_id) => self
                            .execute_cancellable_domain(
                                request.project_id.clone(),
                                request.base_revision.clone(),
                                request.operation.clone(),
                                request.input.clone(),
                                execution_id.clone(),
                                task_id,
                                matches!(descriptor.permission, Permission::ExternalProcess),
                                &context,
                            )
                            .await
                            .map(|value| (value, None)),
                        None => Err(KernelError::Precondition(
                            "cancellable operation requires taskId".into(),
                        )),
                    }
                } else {
                    self.kernel
                        .execute_domain_tool(
                            &request.project_id,
                            request.base_revision.as_ref(),
                            &request.operation,
                            request.input.clone(),
                            &context,
                        )
                        .await
                        .and_then(|value| encode(value).map(|value| (value, None)))
                }
            }
        };
        let response = match outcome {
            Ok((data, new_revision)) => JdpResponse::Success(JdpSuccess {
                success: true,
                request_id: request.request_id,
                execution_id,
                base_revision: base,
                new_revision,
                data,
                artifact_refs: Vec::new(),
                evidence_refs: Vec::new(),
                findings: FindingSummary::default(),
            }),
            Err(error) => failure(request.request_id, Some(execution_id), error_detail(&error)),
        };
        if request.operation == operation::EXECUTION_RECONCILE {
            return response;
        }
        self.record_terminal_execution(
            &request.project_id,
            &request.idempotency_key,
            &request.operation,
            response,
        )
    }

    fn record_terminal_execution(
        &self,
        project_id: &ProjectId,
        idempotency_key: &jingwei_jir::IdempotencyKey,
        operation_name: &str,
        response: JdpResponse<Value>,
    ) -> JdpResponse<Value> {
        let (request_id, execution_id, status) = match &response {
            JdpResponse::Success(success) => (
                success.request_id.clone(),
                success.execution_id.clone(),
                "succeeded",
            ),
            JdpResponse::Failure(failure) => (
                failure.request_id.clone(),
                failure
                    .execution_id
                    .clone()
                    .unwrap_or_else(ExecutionId::generate),
                "failed",
            ),
        };
        let result = (|| -> Result<(), KernelError> {
            let value = serde_json::to_value(&response)?;
            let response_hash = jingwei_jir::canonical_sha256(&value)?;
            self.kernel.store().record_execution(&ExecutionRecord {
                execution_id: execution_id.clone(),
                project_id: project_id.clone(),
                idempotency_key: idempotency_key.clone(),
                operation: operation_name.into(),
                status: status.into(),
                response_hash,
                response: value,
                recorded_at: Utc::now(),
            })?;
            Ok(())
        })();
        match result {
            Ok(()) => response,
            Err(error) => failure(request_id, Some(execution_id), error_detail(&error)),
        }
    }

    fn reconcile_execution(
        &self,
        project_id: &ProjectId,
        input: ExecutionReconcileInput,
    ) -> Result<ExecutionReconcileOutput, KernelError> {
        if let Some(record) = self.kernel.store().execution(&input.execution_id)? {
            if record.project_id != *project_id || record.idempotency_key != input.idempotency_key {
                return Ok(ExecutionReconcileOutput {
                    execution_id: input.execution_id,
                    idempotency_key: input.idempotency_key,
                    status: ExecutionStatus::Unknown,
                    response_hash: None,
                    result: None,
                    quarantine_reason: Some(
                        "execution and idempotency identities do not refer to the same durable record"
                            .into(),
                    ),
                    repair_actions: vec![
                        "inspect the original tool execution before retrying".into(),
                    ],
                });
            }
            return Ok(ExecutionReconcileOutput {
                execution_id: input.execution_id,
                idempotency_key: input.idempotency_key,
                status: if record.status == "succeeded" {
                    ExecutionStatus::Succeeded
                } else {
                    ExecutionStatus::Failed
                },
                response_hash: Some(record.response_hash),
                result: Some(record.response),
                quarantine_reason: None,
                repair_actions: Vec::new(),
            });
        }

        if let Some(journal) = self
            .kernel
            .store()
            .operation_journal(project_id, &input.execution_id)?
        {
            if journal.idempotency_key != input.idempotency_key {
                return Ok(ExecutionReconcileOutput {
                    execution_id: input.execution_id,
                    idempotency_key: input.idempotency_key,
                    status: ExecutionStatus::Unknown,
                    response_hash: None,
                    result: None,
                    quarantine_reason: Some(
                        "committed execution has a different idempotency key".into(),
                    ),
                    repair_actions: vec!["do not resubmit with a changed identity".into()],
                });
            }
            let idempotency = self
                .kernel
                .store()
                .idempotency(project_id, &input.idempotency_key)?
                .ok_or_else(|| {
                    KernelError::CorruptProject(
                        "journal exists without its atomic idempotency result".into(),
                    )
                })?;
            let reconstructed = serde_json::to_value(JdpSuccess {
                success: true,
                request_id: journal.request_id,
                execution_id: journal.execution_id,
                base_revision: journal.base_revision,
                new_revision: Some(journal.result_revision),
                data: idempotency.response,
                artifact_refs: journal.artifact_refs,
                evidence_refs: journal.evidence_refs,
                findings: FindingSummary::default(),
            })?;
            let response_hash = jingwei_jir::canonical_sha256(&reconstructed)?;
            return Ok(ExecutionReconcileOutput {
                execution_id: input.execution_id,
                idempotency_key: input.idempotency_key,
                status: ExecutionStatus::Succeeded,
                response_hash: Some(response_hash),
                result: Some(reconstructed),
                quarantine_reason: None,
                repair_actions: Vec::new(),
            });
        }

        if self
            .kernel
            .store()
            .idempotency(project_id, &input.idempotency_key)?
            .is_some()
        {
            return Ok(ExecutionReconcileOutput {
                execution_id: input.execution_id,
                idempotency_key: input.idempotency_key,
                status: ExecutionStatus::Unknown,
                response_hash: None,
                result: None,
                quarantine_reason: Some(
                    "idempotency result exists but the supplied executionId is not its committed execution"
                        .into(),
                ),
                repair_actions: vec!["recover the original executionId from the runtime ledger".into()],
            });
        }

        Ok(ExecutionReconcileOutput {
            execution_id: input.execution_id,
            idempotency_key: input.idempotency_key,
            status: ExecutionStatus::NotFound,
            response_hash: None,
            result: None,
            quarantine_reason: None,
            repair_actions: Vec::new(),
        })
    }

    fn task_state(
        &self,
        project_id: &ProjectId,
        task_id: &TaskId,
    ) -> Result<TaskState, KernelError> {
        let record = self
            .kernel
            .store()
            .task(task_id)?
            .filter(|record| record.project_id == *project_id)
            .ok_or_else(|| KernelError::TaskNotFound(task_id.to_string()))?;
        Ok(task_state_from_record(record))
    }

    fn cancel_task(
        &self,
        project_id: &ProjectId,
        task_id: &TaskId,
    ) -> Result<TaskState, KernelError> {
        let mut record = self
            .kernel
            .store()
            .task(task_id)?
            .filter(|record| record.project_id == *project_id)
            .ok_or_else(|| KernelError::TaskNotFound(task_id.to_string()))?;
        if record.terminal {
            return Ok(task_state_from_record(record));
        }
        if !record.cancellable {
            return Err(KernelError::CapabilityUnavailable(format!(
                "task {task_id} is not cancellable"
            )));
        }
        record.state = "cancel-requested".into();
        record.updated_at = Utc::now();
        self.kernel.store().put_task(&record)?;
        let signalled = self
            .active_tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(task_id)
            .is_some_and(|sender| sender.send(true).is_ok());
        if !signalled {
            record.state = "needs-reconciliation".into();
            record.terminal = true;
            record.updated_at = Utc::now();
            record.quarantine_reason =
                Some("task owner is unavailable, so cancellation terminal state is unknown".into());
            record.repair_actions = vec![
                "inspect the task process and isolated outputs".into(),
                "resolve quarantine before any retry".into(),
            ];
            self.kernel.store().put_task(&record)?;
        }
        Ok(task_state_from_record(record))
    }

    async fn wait_task(
        &self,
        project_id: &ProjectId,
        task_id: &TaskId,
        timeout_ms: u64,
    ) -> Result<TaskState, KernelError> {
        let deadline =
            tokio::time::Instant::now() + Duration::from_millis(timeout_ms.clamp(1, 300_000));
        loop {
            let state = self.task_state(project_id, task_id)?;
            if state.terminal || tokio::time::Instant::now() >= deadline {
                return Ok(state);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_cancellable_domain(
        &self,
        project_id: ProjectId,
        base_revision: Option<RevisionId>,
        operation_name: String,
        input: Value,
        execution_id: ExecutionId,
        task_id: TaskId,
        external_side_effects: bool,
        context: &RequestContext,
    ) -> Result<Value, KernelError> {
        if self.kernel.store().task(&task_id)?.is_some() {
            return Err(KernelError::Precondition(format!(
                "taskId already exists: {task_id}"
            )));
        }
        let now = Utc::now();
        let mut record = DurableTaskRecord {
            task_id: task_id.clone(),
            project_id: project_id.clone(),
            execution_id,
            operation: operation_name.clone(),
            state: "running".into(),
            cancellable: true,
            external_side_effects,
            terminal: false,
            created_at: now,
            updated_at: now,
            quarantine_reason: None,
            repair_actions: Vec::new(),
        };
        self.kernel.store().put_task(&record)?;
        let (sender, mut receiver) = watch::channel(false);
        self.active_tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(task_id.clone(), sender);

        let execution = self.kernel.execute_domain_tool(
            &project_id,
            base_revision.as_ref(),
            &operation_name,
            input,
            context,
        );
        tokio::pin!(execution);
        let result = tokio::select! {
            result = &mut execution => {
                record.state = if result.is_ok() { "succeeded".into() } else { "failed".into() };
                record.terminal = true;
                record.updated_at = Utc::now();
                self.kernel.store().put_task(&record)?;
                result.and_then(encode)
            }
            changed = receiver.changed() => {
                let cancelled = changed.is_ok() && *receiver.borrow();
                record.updated_at = Utc::now();
                record.terminal = true;
                if !cancelled || external_side_effects {
                    record.state = "needs-reconciliation".into();
                    record.quarantine_reason = Some(if external_side_effects {
                        "external process cancellation did not prove a terminal side-effect state".into()
                    } else {
                        "task owner disappeared before cancellation was confirmed".into()
                    });
                    record.repair_actions = vec![
                        "inspect isolated process state and output hashes".into(),
                        "resolve quarantine before retrying".into(),
                    ];
                    self.kernel.store().put_task(&record)?;
                    Err(KernelError::TaskNeedsReconciliation(task_id.to_string()))
                } else {
                    record.state = "cancelled".into();
                    self.kernel.store().put_task(&record)?;
                    Err(KernelError::TaskCancelled(task_id.to_string()))
                }
            }
        };
        self.active_tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&task_id);
        result
    }
}

fn task_state_from_record(record: DurableTaskRecord) -> TaskState {
    TaskState {
        task_id: record.task_id,
        state: record.state,
        cancellable: record.cancellable,
        terminal: record.terminal,
        execution_id: record.execution_id,
        operation: record.operation,
        quarantine_reason: record.quarantine_reason,
        repair_actions: record.repair_actions,
    }
}

fn new_child_span(parent: &TraceContext, request_id: &str, operation: &str) -> DaemonSpan {
    let generated = Uuid::new_v4().simple().to_string();
    let span_id = generated[..16].to_owned();
    DaemonSpan {
        trace_id: parent.trace_id.to_ascii_lowercase(),
        span_id,
        parent_span_id: parent.span_id.to_ascii_lowercase(),
        trace_flags: parent.trace_flags,
        name: "jdp.request.daemon".into(),
        request_id: request_id.into(),
        operation: operation.into(),
        status: "started".into(),
    }
}

fn required_base(request: &JdpRequest<Value>) -> Result<RevisionId, KernelError> {
    request
        .base_revision
        .clone()
        .ok_or_else(|| KernelError::Precondition("write operation requires baseRevision".into()))
}

fn parse_input<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T, KernelError> {
    serde_json::from_value(value.clone()).map_err(KernelError::from)
}

fn encode(value: impl Serialize) -> Result<Value, KernelError> {
    serde_json::to_value(value).map_err(KernelError::from)
}

fn failure(
    request_id: jingwei_jir::RequestId,
    execution_id: Option<ExecutionId>,
    error: ErrorDetail,
) -> JdpResponse<Value> {
    JdpResponse::Failure(JdpFailure {
        success: false,
        request_id,
        execution_id,
        error,
    })
}

fn detail(code: &str, stage: &str, summary: &str, retryable: bool) -> ErrorDetail {
    let mut error = ErrorDetail::new(code, stage, summary);
    error.retryable = retryable;
    error
}

fn error_detail(error: &KernelError) -> ErrorDetail {
    let (code, stage, retryable) = match error {
        KernelError::PermissionDenied(_) => ("permission_denied", "authorization", false),
        KernelError::ProjectExists(_) => ("project_exists", "precondition", false),
        KernelError::ProjectNotFound(_) => ("project_not_found", "precondition", false),
        KernelError::StaleBase { .. } => ("stale_base", "base-revision", false),
        KernelError::IdempotencyConflict => ("idempotency_conflict", "idempotency", false),
        KernelError::Precondition(_) => ("precondition_failed", "precondition", false),
        KernelError::Postcondition(_) => ("postcondition_failed", "postcondition", false),
        KernelError::InvalidPatch(_) => ("invalid_patch", "apply", false),
        KernelError::InvalidQuery(_) => ("invalid_query", "query", false),
        KernelError::CapabilityUnavailable(_) => ("capability_unavailable", "dispatch", false),
        KernelError::TaskNotFound(_) => ("task_not_found", "task-control", false),
        KernelError::TaskCancelled(_) => ("task_cancelled", "task-control", false),
        KernelError::TaskNeedsReconciliation(_) => {
            ("task_needs_reconciliation", "safety-quarantine", false)
        }
        KernelError::DomainTool(_) => ("domain_tool_failed", "domain-tool", false),
        KernelError::ResourceConflict(_) => ("resource_claim_conflict", "resource-claim", true),
        KernelError::Parse(_) => ("jdl_parse_failed", "parse", false),
        KernelError::Compile(_) => ("jdl_compile_failed", "compile", false),
        KernelError::Store(StoreError::WriterAlreadyHeld) => {
            ("workspace_writer_locked", "workspace-lock", true)
        }
        KernelError::Store(_) => ("store_error", "persistence", false),
        KernelError::CorruptProject(_)
        | KernelError::Graph(_)
        | KernelError::Canonical(_)
        | KernelError::InvalidId(_)
        | KernelError::Json(_) => ("invalid_input_or_state", "validation", false),
    };
    let repair_required = matches!(error, KernelError::TaskNeedsReconciliation(_));
    let mut detail = detail(code, stage, &error.to_string(), retryable);
    detail.suggested_actions.push(SuggestedAction {
        kind: if retryable || repair_required {
            SuggestedActionKind::Repair
        } else {
            SuggestedActionKind::ChangeInput
        },
        description: if repair_required {
            "inspect and resolve the quarantined task before any retry".into()
        } else if retryable {
            "retry only after the reported workspace condition changes".into()
        } else {
            "inspect the typed error and submit a corrected request".into()
        },
    });
    detail
}

/// Daemon startup or authentication failure.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("local client authentication failed")]
    Authentication,
    #[error("local client workspace does not match daemon")]
    WorkspaceMismatch,
    #[error("no mutually supported JDP protocol version")]
    ProtocolMismatch,
    #[error("transport failed: {0}")]
    Transport(String),
}
