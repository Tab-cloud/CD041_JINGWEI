//! Transport implementations. Named-pipe framing uses bounded little-endian u32 lengths plus JSON.

use std::sync::Arc;

use async_trait::async_trait;
use jingwei_jdp::{
    ClientError, HandshakeRequest, JDP_PROTOCOL_VERSION, JdpClient, JdpRequest, JdpResponse,
    MAX_FRAME_BYTES,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{Daemon, DaemonError};

/// In-process transport that still serializes through the exact JDP wire types.
pub struct InMemoryClient {
    daemon: Arc<Daemon>,
}

impl InMemoryClient {
    /// Authenticates an in-process client using the same handshake contract.
    pub fn connect(daemon: Arc<Daemon>, client_name: &str) -> Result<Self, DaemonError> {
        daemon.handshake(&HandshakeRequest {
            protocol_versions: vec![JDP_PROTOCOL_VERSION.into()],
            workspace_id: daemon.readiness().workspace_id,
            client_name: client_name.into(),
            client_version: env!("CARGO_PKG_VERSION").into(),
            session_token: daemon.bootstrap_token().into(),
        })?;
        Ok(Self { daemon })
    }
}

#[async_trait]
impl JdpClient for InMemoryClient {
    async fn send<I, O>(&self, request: JdpRequest<I>) -> Result<JdpResponse<O>, ClientError>
    where
        I: Serialize + Send + Sync,
        O: DeserializeOwned + Send,
    {
        let input = serde_json::to_value(&request.input)?;
        let response = self.daemon.dispatch(request.map_input(input)).await;
        serde_json::from_value(serde_json::to_value(response)?).map_err(Into::into)
    }
}

#[cfg(windows)]
mod windows {
    use std::{io::ErrorKind, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use jingwei_jdp::{
        ClientError, HandshakeRequest, HandshakeResponse, JdpClient, JdpRequest, JdpResponse,
    };
    use serde::{Serialize, de::DeserializeOwned};
    use tokio::{
        io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
        net::windows::named_pipe::{
            ClientOptions, NamedPipeClient as TokioPipeClient, ServerOptions,
        },
        sync::Mutex,
    };

    use super::{Daemon, DaemonError, MAX_FRAME_BYTES, Value};

    /// Authenticated Windows named-pipe JDP client.
    pub struct NamedPipeClient {
        pipe: Mutex<TokioPipeClient>,
    }

    impl NamedPipeClient {
        /// Connects, performs handshake, and retains the negotiated stream.
        pub async fn connect(
            pipe_name: &str,
            request: &HandshakeRequest,
        ) -> Result<(Self, HandshakeResponse), ClientError> {
            let mut last_error = None;
            let mut pipe = None;
            for _ in 0..50 {
                match ClientOptions::new().open(pipe_name) {
                    Ok(client) => {
                        pipe = Some(client);
                        break;
                    }
                    Err(error)
                        if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::WouldBlock) =>
                    {
                        last_error = Some(error);
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }
                    Err(error) => return Err(ClientError::Transport(error.to_string())),
                }
            }
            let mut pipe = pipe.ok_or_else(|| {
                ClientError::Transport(last_error.map_or_else(
                    || "named pipe unavailable".into(),
                    |error| error.to_string(),
                ))
            })?;
            write_frame(&mut pipe, request).await?;
            let response: Result<HandshakeResponse, String> =
                read_frame(&mut pipe, 64 * 1024).await?;
            let response = response.map_err(ClientError::Transport)?;
            Ok((
                Self {
                    pipe: Mutex::new(pipe),
                },
                response,
            ))
        }
    }

    #[async_trait]
    impl JdpClient for NamedPipeClient {
        async fn send<I, O>(&self, request: JdpRequest<I>) -> Result<JdpResponse<O>, ClientError>
        where
            I: Serialize + Send + Sync,
            O: DeserializeOwned + Send,
        {
            let mut pipe = self.pipe.lock().await;
            write_frame(&mut *pipe, &request).await?;
            read_frame(&mut *pipe, MAX_FRAME_BYTES).await
        }
    }

    /// Serves one authenticated pipe connection until the client disconnects.
    pub async fn serve_named_pipe_once(
        daemon: Arc<Daemon>,
        pipe_name: &str,
    ) -> Result<(), DaemonError> {
        validate_pipe_name(pipe_name)?;
        let server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(pipe_name)
            .map_err(|error| DaemonError::Transport(error.to_string()))?;
        server
            .connect()
            .await
            .map_err(|error| DaemonError::Transport(error.to_string()))?;
        handle_connection(daemon, server).await
    }

    /// Serves concurrent authenticated clients on independently framed pipe instances.
    pub async fn serve_named_pipe(daemon: Arc<Daemon>, pipe_name: &str) -> Result<(), DaemonError> {
        validate_pipe_name(pipe_name)?;
        let mut first = true;
        loop {
            let server = ServerOptions::new()
                .first_pipe_instance(first)
                .create(pipe_name)
                .map_err(|error| DaemonError::Transport(error.to_string()))?;
            first = false;
            server
                .connect()
                .await
                .map_err(|error| DaemonError::Transport(error.to_string()))?;
            let connection_daemon = daemon.clone();
            tokio::spawn(async move {
                if let Err(error) = handle_connection(connection_daemon, server).await {
                    eprintln!("jingwei-daemon connection closed with error: {error}");
                }
            });
        }
    }

    fn validate_pipe_name(pipe_name: &str) -> Result<(), DaemonError> {
        if !pipe_name.starts_with(r"\\.\pipe\") || pipe_name.len() > 240 {
            return Err(DaemonError::Transport(
                "invalid Windows named pipe name".into(),
            ));
        }
        Ok(())
    }

    async fn handle_connection(
        daemon: Arc<Daemon>,
        mut server: tokio::net::windows::named_pipe::NamedPipeServer,
    ) -> Result<(), DaemonError> {
        let handshake: HandshakeRequest = read_frame(&mut server, 64 * 1024)
            .await
            .map_err(|error| DaemonError::Transport(error.to_string()))?;
        match daemon.handshake(&handshake) {
            Ok(response) => write_frame(&mut server, &Ok::<_, String>(response))
                .await
                .map_err(|error| DaemonError::Transport(error.to_string()))?,
            Err(error) => {
                write_frame(&mut server, &Err::<HandshakeResponse, _>(error.to_string()))
                    .await
                    .map_err(|write_error| DaemonError::Transport(write_error.to_string()))?;
                return Err(error);
            }
        }
        loop {
            let request: JdpRequest<Value> = match read_frame(&mut server, MAX_FRAME_BYTES).await {
                Ok(request) => request,
                Err(ClientError::Transport(message)) if message.contains("closed") => return Ok(()),
                Err(error) => return Err(DaemonError::Transport(error.to_string())),
            };
            let response = daemon.dispatch(request).await;
            write_frame(&mut server, &response)
                .await
                .map_err(|error| DaemonError::Transport(error.to_string()))?;
        }
    }

    async fn read_frame<T: DeserializeOwned>(
        stream: &mut (impl AsyncRead + Unpin),
        limit: usize,
    ) -> Result<T, ClientError> {
        let length = stream.read_u32_le().await.map_err(|error| {
            let message = if error.kind() == ErrorKind::UnexpectedEof {
                "connection closed".into()
            } else {
                error.to_string()
            };
            ClientError::Transport(message)
        })? as usize;
        if length == 0 || length > limit {
            return Err(ClientError::Transport(format!(
                "JDP frame length {length} exceeds limit {limit}"
            )));
        }
        let mut bytes = vec![0; length];
        stream
            .read_exact(&mut bytes)
            .await
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        serde_json::from_slice(&bytes).map_err(Into::into)
    }

    async fn write_frame<T: Serialize>(
        stream: &mut (impl AsyncWrite + Unpin),
        value: &T,
    ) -> Result<(), ClientError> {
        let bytes = serde_json::to_vec(value)?;
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(ClientError::Transport(
                "serialized JDP frame exceeds limit".into(),
            ));
        }
        let length = u32::try_from(bytes.len())
            .map_err(|_| ClientError::Transport("frame length overflow".into()))?;
        stream
            .write_u32_le(length)
            .await
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        stream
            .write_all(&bytes)
            .await
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        stream
            .flush()
            .await
            .map_err(|error| ClientError::Transport(error.to_string()))?;
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use std::sync::Arc;

        use jingwei_jdp::{
            CapabilityState, HandshakeRequest, JDP_PROTOCOL_VERSION, JdpClient, JdpRequest,
            JdpResponse, WorkspaceCapabilities, operation,
        };
        use jingwei_jir::{IdempotencyKey, ProjectId, RequestId, WorkspaceId};
        use jingwei_kernel::ExtensionRegistryBuilder;
        use tempfile::tempdir;

        use super::*;

        #[tokio::test]
        async fn named_pipe_performs_handshake_and_jdp_request() {
            let directory = tempdir().unwrap();
            let workspace = WorkspaceId::new("ws-pipe-test").unwrap();
            let daemon = Arc::new(
                Daemon::create(
                    directory.path(),
                    workspace.clone(),
                    ExtensionRegistryBuilder::new().build(),
                )
                .unwrap(),
            );
            let pipe_name = format!(r"\\.\pipe\jingwei-test-{}", uuid::Uuid::new_v4());
            let server_pipe_name = pipe_name.clone();
            let server_daemon = daemon.clone();
            let server = tokio::spawn(async move {
                serve_named_pipe_once(server_daemon, &server_pipe_name).await
            });
            let request = HandshakeRequest {
                protocol_versions: vec![JDP_PROTOCOL_VERSION.into()],
                workspace_id: workspace.clone(),
                client_name: "test".into(),
                client_version: "0.1.0".into(),
                session_token: daemon.bootstrap_token().into(),
            };
            let (client, response) = NamedPipeClient::connect(&pipe_name, &request)
                .await
                .unwrap();
            assert_eq!(response.protocol_version, JDP_PROTOCOL_VERSION);
            let capabilities: JdpResponse<WorkspaceCapabilities> = client
                .send(JdpRequest {
                    protocol_version: JDP_PROTOCOL_VERSION.into(),
                    request_id: RequestId::new("request-pipe-capabilities").unwrap(),
                    idempotency_key: IdempotencyKey::new("idem-pipe-capabilities").unwrap(),
                    workspace_id: workspace,
                    project_id: ProjectId::new("project-pipe-capabilities").unwrap(),
                    operation: operation::WORKSPACE_CAPABILITIES.into(),
                    execution_id: None,
                    task_id: None,
                    base_revision: None,
                    deadline_ms: Some(5_000),
                    dry_run: false,
                    trace_context: None,
                    input: serde_json::json!({}),
                })
                .await
                .unwrap();
            let JdpResponse::Success(capabilities) = capabilities else {
                panic!("workspace.capabilities must succeed over the named pipe");
            };
            assert!(matches!(
                capabilities.data.operations.get(operation::TASK_GET),
                Some(CapabilityState::Available)
            ));
            drop(client);
            server.await.unwrap().unwrap();
        }

        #[tokio::test]
        async fn named_pipe_accepts_runtime_and_workbench_clients_concurrently() {
            let directory = tempdir().unwrap();
            let workspace = WorkspaceId::new("ws-pipe-multiclient").unwrap();
            let daemon = Arc::new(
                Daemon::create(
                    directory.path(),
                    workspace.clone(),
                    ExtensionRegistryBuilder::new().build(),
                )
                .unwrap(),
            );
            let pipe_name = format!(r"\\.\pipe\jingwei-multiclient-{}", uuid::Uuid::new_v4());
            let server_daemon = daemon.clone();
            let server_name = pipe_name.clone();
            let server =
                tokio::spawn(async move { serve_named_pipe(server_daemon, &server_name).await });
            let handshake = |name: &str| HandshakeRequest {
                protocol_versions: vec![JDP_PROTOCOL_VERSION.into()],
                workspace_id: workspace.clone(),
                client_name: name.into(),
                client_version: "0.1.0".into(),
                session_token: daemon.bootstrap_token().into(),
            };
            let (runtime, _) = NamedPipeClient::connect(&pipe_name, &handshake("runtime"))
                .await
                .unwrap();
            let (workbench, _) = NamedPipeClient::connect(&pipe_name, &handshake("workbench"))
                .await
                .unwrap();
            for (sequence, client) in [&runtime, &workbench].into_iter().enumerate() {
                let response: JdpResponse<WorkspaceCapabilities> = client
                    .send(JdpRequest {
                        protocol_version: JDP_PROTOCOL_VERSION.into(),
                        request_id: RequestId::new(format!("request-multiclient-{sequence}"))
                            .unwrap(),
                        idempotency_key: IdempotencyKey::new(format!(
                            "idem-multiclient-{sequence}"
                        ))
                        .unwrap(),
                        workspace_id: workspace.clone(),
                        project_id: ProjectId::new("project-multiclient").unwrap(),
                        operation: operation::WORKSPACE_CAPABILITIES.into(),
                        execution_id: None,
                        task_id: None,
                        base_revision: None,
                        deadline_ms: Some(5_000),
                        dry_run: false,
                        trace_context: None,
                        input: serde_json::json!({}),
                    })
                    .await
                    .unwrap();
                assert!(matches!(response, JdpResponse::Success(_)));
            }
            drop(runtime);
            drop(workbench);
            server.abort();
        }
    }
}

#[cfg(windows)]
pub use windows::{NamedPipeClient, serve_named_pipe, serve_named_pipe_once};
