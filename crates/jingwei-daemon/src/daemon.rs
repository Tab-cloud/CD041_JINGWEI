//! Daemon lifecycle, request dispatch, capability negotiation, and error mapping.

use std::{
    collections::BTreeMap, panic::AssertUnwindSafe, path::Path, sync::Mutex, time::Duration,
};

use futures_util::FutureExt;
use jingwei_jdp::{
    ArtifactGetInput, ArtifactGetOutput, CapabilityState, CompileJdlInput, DiffInput,
    EditCommandGroup, EditSessionOpenInput, EditSessionRef, ErrorDetail, EventsInput, ExplainInput,
    FindingSummary, HandshakeRequest, HandshakeResponse, JDP_PROTOCOL_VERSION, JdpFailure,
    JdpRequest, JdpResponse, JdpSuccess, PatchApplyInput, PatchInput, ProjectCreateInput,
    QueryInput, SuggestedAction, SuggestedActionKind, TraceContext, ValidateInput,
    WorkspaceCapabilities, operation, tool_catalog,
};
use jingwei_jir::{ExecutionId, RevisionId, WorkspaceId};
use jingwei_kernel::{ExtensionRegistry, Kernel, KernelError, RequestContext};
use jingwei_store::{StoreError, WorkspaceStore, WriterLease};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

/// Structured liveness response.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub status: String,
    pub daemon_instance: String,
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
    session_token: String,
    recovery_summary: String,
    trace_spans: Mutex<Vec<DaemonSpan>>,
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

    fn from_store(store: WorkspaceStore, registry: ExtensionRegistry) -> Result<Self, DaemonError> {
        let instance_id = format!("daemon-{}", Uuid::new_v4());
        let writer_lease = store.acquire_writer(&instance_id)?;
        let recovery = store.recover()?;
        let recovery_summary = format!(
            "recovered={}, quarantined={}, removedIncomplete={}",
            recovery.recovered_transactions,
            recovery.quarantined_transactions,
            recovery.removed_incomplete_transactions
        );
        let workspace_id = store.metadata().workspace_id.clone();
        Ok(Self {
            kernel: Kernel::new(store, registry),
            workspace_id,
            instance_id,
            session_token: format!("session-{}-{}", Uuid::new_v4(), Uuid::new_v4()),
            recovery_summary,
            trace_spans: Mutex::new(Vec::new()),
            _writer_lease: writer_lease,
        })
    }

    /// Returns liveness without exposing tokens or workspace design data.
    pub fn health(&self) -> HealthStatus {
        HealthStatus {
            status: "healthy".into(),
            daemon_instance: self.instance_id.clone(),
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
    pub async fn dispatch(&self, request: JdpRequest<Value>) -> JdpResponse<Value> {
        let request_id = request.request_id.clone();
        let trace_span = match request.trace_context.as_ref() {
            Some(trace_context) => {
                if let Err(reason) = trace_context.validate() {
                    return failure(
                        request_id,
                        None,
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
                self.complete_trace_span(trace_span, "deadline");
                return failure(
                    request_id,
                    None,
                    detail(
                        "deadline_exceeded",
                        "dispatch",
                        "request deadline elapsed",
                        true,
                    ),
                );
            }
        } else {
            future.await
        };
        if let Ok(response) = result {
            let status = if response.is_success() { "ok" } else { "error" };
            self.complete_trace_span(trace_span, status);
            response
        } else {
            self.complete_trace_span(trace_span, "panic");
            failure(
                request_id,
                None,
                detail(
                    "internal_panic",
                    "service-boundary",
                    "request was safely aborted",
                    false,
                ),
            )
        }
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
        let execution_id = ExecutionId::generate();
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
        let context = RequestContext::local("local-jdp-client");
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
                    .map(|tool| {
                        let state = if matches!(
                            tool.name.as_str(),
                            operation::TASK_GET | operation::TASK_CANCEL
                        ) {
                            CapabilityState::CapabilityUnavailable
                        } else {
                            CapabilityState::Available
                        };
                        (tool.name, state)
                    })
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
            operation::TASK_GET | operation::TASK_CANCEL => {
                Err(KernelError::CapabilityUnavailable(
                    "no durable task with that ID is registered".into(),
                ))
            }
            _ => self
                .kernel
                .execute_domain_tool(
                    &request.project_id,
                    request.base_revision.as_ref(),
                    &request.operation,
                    request.input.clone(),
                    &context,
                )
                .await
                .and_then(|value| encode(value).map(|value| (value, None))),
        };
        match outcome {
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
        }
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
        KernelError::StaleBase { .. } => ("stale_base", "base-revision", false),
        KernelError::IdempotencyConflict => ("idempotency_conflict", "idempotency", false),
        KernelError::Precondition(_) => ("precondition_failed", "precondition", false),
        KernelError::Postcondition(_) => ("postcondition_failed", "postcondition", false),
        KernelError::InvalidPatch(_) => ("invalid_patch", "apply", false),
        KernelError::InvalidQuery(_) => ("invalid_query", "query", false),
        KernelError::CapabilityUnavailable(_) => ("capability_unavailable", "dispatch", false),
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
    let mut detail = detail(code, stage, &error.to_string(), retryable);
    detail.suggested_actions.push(SuggestedAction {
        kind: if retryable {
            SuggestedActionKind::Repair
        } else {
            SuggestedActionKind::ChangeInput
        },
        description: if retryable {
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
