//! JDP request, response, and structured-error envelopes.

use async_trait::async_trait;
use jingwei_jir::{ExecutionId, IdempotencyKey, ProjectId, RequestId, RevisionId, WorkspaceId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

/// W3C-compatible trace parent carried across the TypeScript/Rust JDP boundary.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    #[serde(default = "default_trace_flags")]
    pub trace_flags: u8,
}

impl TraceContext {
    /// Rejects malformed or all-zero W3C trace/span identifiers before dispatch.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !valid_hex_id(&self.trace_id, 32) {
            return Err("traceId must be 32 non-zero hexadecimal characters");
        }
        if !valid_hex_id(&self.span_id, 16) {
            return Err("spanId must be 16 non-zero hexadecimal characters");
        }
        Ok(())
    }
}

const fn default_trace_flags() -> u8 {
    1
}

fn valid_hex_id(value: &str, width: usize) -> bool {
    value.len() == width
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value.bytes().any(|byte| byte != b'0')
}

/// Versioned JDP request envelope.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JdpRequest<T> {
    pub protocol_version: String,
    pub request_id: RequestId,
    pub idempotency_key: IdempotencyKey,
    pub workspace_id: WorkspaceId,
    pub project_id: ProjectId,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<RevisionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_context: Option<TraceContext>,
    pub input: T,
}

impl<T> JdpRequest<T> {
    /// Maps the typed input while preserving the envelope.
    pub fn map_input<U>(self, input: U) -> JdpRequest<U> {
        JdpRequest {
            protocol_version: self.protocol_version,
            request_id: self.request_id,
            idempotency_key: self.idempotency_key,
            workspace_id: self.workspace_id,
            project_id: self.project_id,
            operation: self.operation,
            base_revision: self.base_revision,
            deadline_ms: self.deadline_ms,
            dry_run: self.dry_run,
            trace_context: self.trace_context,
            input,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TraceContext;

    #[test]
    fn trace_context_accepts_w3c_widths_and_rejects_zero_ids() {
        let valid = TraceContext {
            trace_id: "0123456789abcdef0123456789abcdef".into(),
            span_id: "0123456789abcdef".into(),
            trace_flags: 1,
        };
        assert_eq!(valid.validate(), Ok(()));

        let zero_trace = TraceContext {
            trace_id: "00000000000000000000000000000000".into(),
            ..valid.clone()
        };
        assert!(zero_trace.validate().is_err());

        let short_span = TraceContext {
            span_id: "1234".into(),
            ..valid
        };
        assert!(short_span.validate().is_err());
    }
}

/// A successful JDP result.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JdpSuccess<T> {
    pub success: bool,
    pub request_id: RequestId,
    pub execution_id: ExecutionId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<RevisionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_revision: Option<RevisionId>,
    pub data: T,
    #[serde(default)]
    pub artifact_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub findings: FindingSummary,
}

/// A failed JDP result.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JdpFailure {
    pub success: bool,
    pub request_id: RequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<ExecutionId>,
    pub error: ErrorDetail,
}

/// Success or failure wire response. The untagged layout preserves the `success` discriminator.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(untagged)]
pub enum JdpResponse<T> {
    Success(JdpSuccess<T>),
    Failure(JdpFailure),
}

impl<T> JdpResponse<T> {
    /// Returns true for a successful response.
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }
}

/// Counts by severity returned on every successful operation.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindingSummary {
    pub errors: u32,
    pub warnings: u32,
    pub info: u32,
}

/// Stable structured error body.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorDetail {
    pub code: String,
    pub stage: String,
    pub summary: String,
    pub fingerprint: String,
    pub retryable: bool,
    #[serde(default)]
    pub object_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub suggested_actions: Vec<SuggestedAction>,
}

impl ErrorDetail {
    /// Constructs a stable error fingerprint without including secrets or arbitrary input.
    pub fn new(
        code: impl Into<String>,
        stage: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        let code = code.into();
        let stage = stage.into();
        let summary = summary.into();
        let digest = jingwei_jir::canonical_sha256(&serde_json::json!({
            "code": code,
            "stage": stage,
            "summary": summary,
        }))
        .unwrap_or_else(|_| "fingerprint-unavailable".into());
        Self {
            code,
            stage,
            summary,
            fingerprint: digest[..digest.len().min(24)].to_owned(),
            retryable: false,
            object_refs: Vec::new(),
            evidence_refs: Vec::new(),
            suggested_actions: Vec::new(),
        }
    }
}

/// Machine-readable recovery suggestion.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedAction {
    pub kind: SuggestedActionKind,
    pub description: String,
}

/// Allowed recovery action categories.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SuggestedActionKind {
    ChangeInput,
    UseAlternative,
    RequestUser,
    Repair,
    AbortRelease,
}

/// Transport-independent JDP client used by CLI, GUI, runtime, and tests.
#[async_trait]
pub trait JdpClient: Send + Sync {
    /// Sends one request and waits for its terminal response.
    async fn send<I, O>(&self, request: JdpRequest<I>) -> Result<JdpResponse<O>, ClientError>
    where
        I: Serialize + Send + Sync,
        O: DeserializeOwned + Send;
}

/// Client transport or serialization error.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("JDP serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("JDP transport failed: {0}")]
    Transport(String),
}
