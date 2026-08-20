mod credential_store;
mod model_configuration;

use model_configuration::{
    model_configuration_delete_credential, model_configuration_get, model_configuration_save,
    resolve_provider_test_configuration, validate_credential_reference,
    ResolvedProviderTestConfiguration,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;
use zeroize::Zeroize;

const EXPECTED_PROTOCOL_VERSION: &str = "0.1";
const MAX_HANDSHAKE_BYTES: usize = 4096;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SidecarManifest {
    schema_version: String,
    protocol_version: String,
    target: String,
    sidecars: Vec<SidecarEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SidecarEntry {
    kind: String,
    relative_path: String,
    version: String,
    sha256: String,
    bundled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SidecarStatus {
    kind: String,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SidecarHandshake {
    protocol_version: String,
    version: String,
    target: String,
    health: String,
    #[serde(default)]
    rpc_endpoint: Option<String>,
}

#[derive(Default)]
struct SidecarSupervisor {
    statuses: Mutex<HashMap<String, SidecarStatus>>,
    children: Mutex<HashMap<String, CommandChild>>,
    bootstrap: Mutex<Option<SidecarBootstrap>>,
}

#[derive(Clone)]
struct SidecarBootstrap {
    daemon_session_token: String,
    runtime_bootstrap_token: String,
    daemon_pipe_name: String,
    runtime_pipe_name: String,
    daemon_workspace: PathBuf,
    runtime_database: PathBuf,
    workspace_id: String,
    project_id: String,
}

impl SidecarSupervisor {
    fn set_status(&self, status: SidecarStatus) {
        if let Ok(mut statuses) = self.statuses.lock() {
            statuses.insert(status.kind.clone(), status);
        }
    }

    fn status_list(&self) -> Vec<SidecarStatus> {
        let mut statuses = self
            .statuses
            .lock()
            .map(|items| items.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        statuses.sort_by_key(|status| match status.kind.as_str() {
            "runtime" => 0,
            "daemon" => 1,
            _ => 2,
        });
        statuses
    }

    fn bootstrap(&self, app: &AppHandle) -> Result<SidecarBootstrap, String> {
        let mut current = self
            .bootstrap
            .lock()
            .map_err(|_| "sidecar bootstrap state poisoned".to_string())?;
        if let Some(bootstrap) = current.as_ref() {
            return Ok(bootstrap.clone());
        }
        let data_root = app
            .path()
            .app_local_data_dir()
            .map_err(|error| format!("无法解析应用数据目录: {error}"))?;
        fs::create_dir_all(&data_root).map_err(|error| format!("无法创建应用数据目录: {error}"))?;
        let bootstrap = SidecarBootstrap {
            daemon_session_token: format!(
                "daemon-{}-{}",
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4()
            ),
            runtime_bootstrap_token: format!(
                "runtime-{}-{}",
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4()
            ),
            daemon_pipe_name: format!(r"\\.\pipe\jingwei-desktop-{}", uuid::Uuid::new_v4()),
            runtime_pipe_name: format!(r"\\.\pipe\jingwei-runtime-{}", uuid::Uuid::new_v4()),
            daemon_workspace: data_root.join("daemon-workspace"),
            runtime_database: data_root.join("runtime").join("runtime.sqlite"),
            workspace_id: "ws-desktop".into(),
            project_id: "project-desktop".into(),
        };
        *current = Some(bootstrap.clone());
        Ok(bootstrap)
    }

    fn shutdown(&self) {
        if let Ok(mut children) = self.children.lock() {
            for (_, child) in children.drain() {
                let _ = child.kill();
            }
        }
    }
}

#[tauri::command]
fn sidecar_status(state: tauri::State<'_, SidecarSupervisor>) -> Vec<SidecarStatus> {
    state.status_list()
}

#[tauri::command]
async fn ensure_sidecars(app: AppHandle) -> Result<Vec<SidecarStatus>, String> {
    let manifest = read_manifest(&app)?;
    validate_manifest(&manifest)?;
    let bootstrap = app.state::<SidecarSupervisor>().bootstrap(&app)?;
    let mut entries = manifest.sidecars.clone();
    entries.sort_by_key(|entry| if entry.kind == "daemon" { 0 } else { 1 });
    for entry in entries {
        if let Err(reason) = start_sidecar(&app, &manifest, entry.clone(), &bootstrap) {
            app.state::<SidecarSupervisor>().set_status(SidecarStatus {
                kind: entry.kind,
                state: "unavailable".into(),
                version: Some(entry.version),
                reason: Some(reason),
            });
        }
    }
    Ok(app.state::<SidecarSupervisor>().status_list())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TransportConfiguration {
    protocol_version: &'static str,
    workspace_id: String,
    project_id: String,
    mode: &'static str,
}

#[tauri::command]
fn transport_configuration(app: AppHandle) -> Result<TransportConfiguration, String> {
    let bootstrap = app.state::<SidecarSupervisor>().bootstrap(&app)?;
    Ok(TransportConfiguration {
        protocol_version: EXPECTED_PROTOCOL_VERSION,
        workspace_id: bootstrap.workspace_id,
        project_id: bootstrap.project_id,
        mode: "real",
    })
}

#[tauri::command]
async fn runtime_rpc(
    app: AppHandle,
    method: String,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    if !matches!(
        method.as_str(),
        "health"
            | "capabilities"
            | "session.create"
            | "input.admit"
            | "run.resume"
            | "run.cancel"
            | "approval.list"
            | "approval.reply"
            | "events.read"
            | "artifact.get"
    ) {
        return Err("Runtime RPC method is not exposed to the workbench".into());
    }
    let bootstrap = app.state::<SidecarSupervisor>().bootstrap(&app)?;
    runtime_pipe_request(&bootstrap, &method, params).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderTestParams<'a> {
    provider: &'a str,
    base_url: &'a str,
    model: &'a str,
    api_key: &'a str,
    timeout_ms: u64,
}

#[derive(Serialize)]
struct RuntimeRpcEnvelope<'a, T: Serialize> {
    id: String,
    token: &'a str,
    method: &'a str,
    params: T,
}

#[tauri::command]
async fn model_configuration_test(app: AppHandle) -> Result<serde_json::Value, String> {
    let mut configuration = resolve_provider_test_configuration(&app)?;
    let bootstrap = app.state::<SidecarSupervisor>().bootstrap(&app)?;
    let result = runtime_provider_test_request(&bootstrap, &configuration).await;
    configuration.api_key.zeroize();
    result
}

#[tauri::command]
async fn design_rpc(
    app: AppHandle,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let bootstrap = app.state::<SidecarSupervisor>().bootstrap(&app)?;
    if request
        .get("workspaceId")
        .and_then(serde_json::Value::as_str)
        != Some(bootstrap.workspace_id.as_str())
        || request.get("projectId").and_then(serde_json::Value::as_str)
            != Some(bootstrap.project_id.as_str())
    {
        return Err("JDP request is outside the desktop workspace/project boundary".into());
    }
    daemon_pipe_request(&bootstrap, request).await
}

#[cfg(windows)]
async fn runtime_pipe_request(
    bootstrap: &SidecarBootstrap,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut pipe = open_named_pipe(&bootstrap.runtime_pipe_name).await?;
    write_pipe_frame(
        &mut pipe,
        &serde_json::json!({
            "id": format!("ui-{}", uuid::Uuid::new_v4()),
            "token": bootstrap.runtime_bootstrap_token,
            "method": method,
            "params": params,
        }),
    )
    .await?;
    let response = read_pipe_frame(&mut pipe, 4 * 1024 * 1024).await?;
    if response.get("success").and_then(serde_json::Value::as_bool) == Some(true) {
        Ok(response
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    } else {
        Err(response
            .pointer("/error/summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Runtime RPC failed")
            .to_string())
    }
}

#[cfg(windows)]
async fn runtime_provider_test_request(
    bootstrap: &SidecarBootstrap,
    configuration: &ResolvedProviderTestConfiguration,
) -> Result<serde_json::Value, String> {
    let mut pipe = open_named_pipe(&bootstrap.runtime_pipe_name).await?;
    let envelope = RuntimeRpcEnvelope {
        id: format!("provider-test-{}", uuid::Uuid::new_v4()),
        token: &bootstrap.runtime_bootstrap_token,
        method: "provider.test",
        params: ProviderTestParams {
            provider: &configuration.provider,
            base_url: &configuration.base_url,
            model: &configuration.model,
            api_key: &configuration.api_key,
            timeout_ms: configuration.timeout_ms,
        },
    };
    write_sensitive_pipe_frame(&mut pipe, &envelope).await?;
    let response = read_pipe_frame(&mut pipe, 64 * 1024).await?;
    if response.get("success").and_then(serde_json::Value::as_bool) == Some(true) {
        Ok(response
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    } else {
        Err(response
            .pointer("/error/summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Runtime provider test failed")
            .to_string())
    }
}

#[cfg(windows)]
async fn daemon_pipe_request(
    bootstrap: &SidecarBootstrap,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut pipe = open_named_pipe(&bootstrap.daemon_pipe_name).await?;
    write_pipe_frame(
        &mut pipe,
        &serde_json::json!({
            "protocolVersions": [EXPECTED_PROTOCOL_VERSION],
            "workspaceId": bootstrap.workspace_id,
            "clientName": "jingwei-workbench",
            "clientVersion": env!("CARGO_PKG_VERSION"),
            "sessionToken": bootstrap.daemon_session_token,
        }),
    )
    .await?;
    let handshake = read_pipe_frame(&mut pipe, 64 * 1024).await?;
    let negotiated = handshake.get("Ok").unwrap_or(&handshake);
    if negotiated
        .get("protocolVersion")
        .and_then(serde_json::Value::as_str)
        != Some(EXPECTED_PROTOCOL_VERSION)
    {
        return Err(handshake
            .get("Err")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("daemon JDP handshake failed")
            .to_string());
    }
    write_pipe_frame(&mut pipe, &request).await?;
    read_pipe_frame(&mut pipe, 4 * 1024 * 1024).await
}

#[cfg(windows)]
async fn open_named_pipe(
    pipe_name: &str,
) -> Result<tokio::net::windows::named_pipe::NamedPipeClient, String> {
    use std::io::ErrorKind;
    use tokio::net::windows::named_pipe::ClientOptions;

    let mut last_error = None;
    for _ in 0..200 {
        match ClientOptions::new().open(pipe_name) {
            Ok(pipe) => return Ok(pipe),
            Err(error) if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::WouldBlock) => {
                last_error = Some(error.to_string());
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            Err(error) => return Err(format!("named pipe connect failed: {error}")),
        }
    }
    Err(format!(
        "named pipe did not become available: {}",
        last_error.unwrap_or_else(|| "unknown error".into())
    ))
}

#[cfg(windows)]
async fn write_pipe_frame(
    pipe: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    value: &serde_json::Value,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;

    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() > 4 * 1024 * 1024 {
        return Err("local RPC request exceeds frame limit".into());
    }
    pipe.write_u32_le(u32::try_from(bytes.len()).map_err(|_| "frame length overflow")?)
        .await
        .map_err(|error| error.to_string())?;
    pipe.write_all(&bytes)
        .await
        .map_err(|error| error.to_string())?;
    pipe.flush().await.map_err(|error| error.to_string())
}

#[cfg(windows)]
async fn write_sensitive_pipe_frame<T: Serialize>(
    pipe: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    value: &T,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;

    let mut bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if bytes.is_empty() || bytes.len() > 64 * 1024 {
        bytes.zeroize();
        return Err("provider test request exceeds frame limit".into());
    }
    let result = async {
        pipe.write_u32_le(u32::try_from(bytes.len()).map_err(|_| "frame length overflow")?)
            .await
            .map_err(|error| error.to_string())?;
        pipe.write_all(&bytes)
            .await
            .map_err(|error| error.to_string())?;
        pipe.flush().await.map_err(|error| error.to_string())
    }
    .await;
    bytes.zeroize();
    result
}

#[cfg(windows)]
async fn read_pipe_frame(
    pipe: &mut tokio::net::windows::named_pipe::NamedPipeClient,
    limit: usize,
) -> Result<serde_json::Value, String> {
    use tokio::io::AsyncReadExt;

    let length = pipe
        .read_u32_le()
        .await
        .map_err(|error| error.to_string())? as usize;
    if length == 0 || length > limit {
        return Err(format!(
            "local RPC response frame length {length} is invalid"
        ));
    }
    let mut bytes = vec![0; length];
    pipe.read_exact(&mut bytes)
        .await
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

#[cfg(not(windows))]
async fn runtime_pipe_request(
    _bootstrap: &SidecarBootstrap,
    _method: &str,
    _params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    Err("Runtime local RPC is currently available only on Windows".into())
}

#[cfg(not(windows))]
async fn runtime_provider_test_request(
    _bootstrap: &SidecarBootstrap,
    _configuration: &ResolvedProviderTestConfiguration,
) -> Result<serde_json::Value, String> {
    Err("Runtime provider test is currently available only on Windows".into())
}

#[cfg(not(windows))]
async fn daemon_pipe_request(
    _bootstrap: &SidecarBootstrap,
    _request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    Err("Daemon local RPC is currently available only on Windows".into())
}

#[tauri::command]
fn open_external_url(app: AppHandle, url: String) -> Result<(), String> {
    let parsed = validate_external_url(&url)?;
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|error| format!("无法打开系统浏览器: {error}"))
}

fn start_sidecar(
    app: &AppHandle,
    manifest: &SidecarManifest,
    entry: SidecarEntry,
    bootstrap: &SidecarBootstrap,
) -> Result<(), String> {
    validate_entry(&entry, &manifest.target)?;
    if !entry.bundled {
        return Err("开发清单明确标记为未捆绑；不会启动占位二进制".into());
    }
    if app
        .state::<SidecarSupervisor>()
        .children
        .lock()
        .map_err(|_| "sidecar child state poisoned".to_string())?
        .contains_key(&entry.kind)
    {
        return Ok(());
    }

    let executable = resolve_resource_path(app, &entry.relative_path)?;
    verify_file_hash(&executable, &entry.sha256)?;
    let mut command = app
        .shell()
        .command(executable.to_string_lossy().to_string());
    let bootstrap_input = if entry.kind == "daemon" {
        command = command.args([
            "serve",
            "--workspace",
            &bootstrap.daemon_workspace.display().to_string(),
            "--workspace-id",
            &bootstrap.workspace_id,
            "--pipe-name",
            &bootstrap.daemon_pipe_name,
            "--migrate-if-needed",
        ]);
        format!("{}\n", bootstrap.daemon_session_token)
    } else {
        format!(
            "{}\n",
            serde_json::json!({
                "bootstrapToken": bootstrap.runtime_bootstrap_token,
                "daemonPipeName": bootstrap.daemon_pipe_name,
                "daemonSessionToken": bootstrap.daemon_session_token,
                "databasePath": bootstrap.runtime_database,
                "workspaceId": bootstrap.workspace_id,
                "projectId": bootstrap.project_id,
                "runtimePipeName": bootstrap.runtime_pipe_name,
            })
        )
    };
    let (mut events, mut child) = command
        .spawn()
        .map_err(|error| format!("无法启动 {} sidecar: {error}", entry.kind))?;
    if let Err(error) = child.write(bootstrap_input.as_bytes()) {
        let _ = child.kill();
        return Err(format!(
            "无法通过 stdin 初始化 {} sidecar: {error}",
            entry.kind
        ));
    }

    app.state::<SidecarSupervisor>()
        .children
        .lock()
        .map_err(|_| "sidecar child state poisoned".to_string())?
        .insert(entry.kind.clone(), child);
    app.state::<SidecarSupervisor>().set_status(SidecarStatus {
        kind: entry.kind.clone(),
        state: "starting".into(),
        version: Some(entry.version.clone()),
        reason: Some("等待版本化 stdout handshake".into()),
    });

    let monitor_app = app.clone();
    let runtime_pipe_name = bootstrap.runtime_pipe_name.clone();
    tauri::async_runtime::spawn(async move {
        let mut handshake_complete = false;
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stdout(bytes) if !handshake_complete => {
                    if bytes.len() > MAX_HANDSHAKE_BYTES {
                        fail_sidecar(
                            &monitor_app,
                            &entry,
                            "handshake 超出 4096 字节",
                            "version-mismatch",
                        );
                        break;
                    }
                    let expected_endpoint =
                        (entry.kind == "runtime").then_some(runtime_pipe_name.as_str());
                    let parsed = serde_json::from_slice::<SidecarHandshake>(&bytes)
                        .map_err(|_| "handshake 不是有效 JSON".to_string())
                        .and_then(|handshake| {
                            validate_handshake(&entry, &handshake, expected_endpoint)
                        });
                    match parsed {
                        Ok(()) => {
                            handshake_complete = true;
                            monitor_app
                                .state::<SidecarSupervisor>()
                                .set_status(SidecarStatus {
                                    kind: entry.kind.clone(),
                                    state: "healthy".into(),
                                    version: Some(entry.version.clone()),
                                    reason: None,
                                });
                        }
                        Err(reason) => {
                            fail_sidecar(&monitor_app, &entry, &reason, "version-mismatch");
                            break;
                        }
                    }
                }
                CommandEvent::Terminated(payload) => {
                    let reason = format!(
                        "进程退出 code={:?} signal={:?}",
                        payload.code, payload.signal
                    );
                    fail_sidecar(&monitor_app, &entry, &reason, "crashed");
                    break;
                }
                _ => {}
            }
        }
    });
    Ok(())
}

fn fail_sidecar(app: &AppHandle, entry: &SidecarEntry, reason: &str, state: &str) {
    if let Ok(mut children) = app.state::<SidecarSupervisor>().children.lock() {
        if let Some(child) = children.remove(&entry.kind) {
            let _ = child.kill();
        }
    }
    app.state::<SidecarSupervisor>()
        .set_status(sidecar_failure_status(entry, reason, state));
}

fn sidecar_failure_status(entry: &SidecarEntry, reason: &str, state: &str) -> SidecarStatus {
    SidecarStatus {
        kind: entry.kind.clone(),
        state: state.into(),
        version: Some(entry.version.clone()),
        reason: Some(reason.into()),
    }
}

fn validate_manifest(manifest: &SidecarManifest) -> Result<(), String> {
    if manifest.schema_version != "0.1" || manifest.protocol_version != EXPECTED_PROTOCOL_VERSION {
        return Err("sidecar manifest 或 protocol 版本不兼容".into());
    }
    if manifest.target != current_target() {
        return Err(format!(
            "sidecar target {} 与当前 {} 不匹配",
            manifest.target,
            current_target()
        ));
    }
    if manifest.sidecars.len() != 2
        || !manifest
            .sidecars
            .iter()
            .any(|entry| entry.kind == "runtime")
        || !manifest.sidecars.iter().any(|entry| entry.kind == "daemon")
    {
        return Err("清单必须恰好声明独立 runtime 与 daemon sidecar".into());
    }
    Ok(())
}

fn validate_entry(entry: &SidecarEntry, target: &str) -> Result<(), String> {
    if !matches!(entry.kind.as_str(), "runtime" | "daemon") {
        return Err("未知 sidecar kind".into());
    }
    let path = Path::new(&entry.relative_path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("sidecar relativePath 只能包含规范相对路径段".into());
    }
    if !entry.relative_path.contains(target) {
        return Err("sidecar 文件名未绑定目标 target triple".into());
    }
    if entry.bundled
        && (entry.sha256.len() != 64
            || !entry
                .sha256
                .chars()
                .all(|character| character.is_ascii_hexdigit()))
    {
        return Err("已捆绑 sidecar 必须提供 64 位 SHA-256".into());
    }
    Ok(())
}

fn validate_handshake(
    entry: &SidecarEntry,
    handshake: &SidecarHandshake,
    expected_rpc_endpoint: Option<&str>,
) -> Result<(), String> {
    if handshake.protocol_version != EXPECTED_PROTOCOL_VERSION {
        return Err("sidecar protocol version mismatch".into());
    }
    if handshake.version != entry.version || handshake.target != current_target() {
        return Err("sidecar version 或 target handshake mismatch".into());
    }
    if handshake.health != "healthy" {
        return Err("sidecar handshake 未报告 healthy".into());
    }
    if expected_rpc_endpoint
        .is_some_and(|expected| handshake.rpc_endpoint.as_deref() != Some(expected))
    {
        return Err("runtime RPC endpoint discovery mismatch".into());
    }
    Ok(())
}

fn validate_external_url(value: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(value).map_err(|_| "外部链接无效".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("只能打开 HTTP 或 HTTPS 外部链接".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("外部链接不能包含内嵌凭据".into());
    }
    Ok(parsed)
}

fn read_manifest(app: &AppHandle) -> Result<SidecarManifest, String> {
    let bundled = app
        .path()
        .resource_dir()
        .map(|directory| directory.join("sidecars").join("manifest.json"))
        .ok()
        .filter(|path| path.exists());
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("sidecars")
        .join("manifest.json");
    let path = bundled.unwrap_or(development);
    let bytes = fs::read(&path).map_err(|error| format!("无法读取 sidecar manifest: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("sidecar manifest 无效: {error}"))
}

fn resolve_resource_path(app: &AppHandle, relative_path: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("sidecar path 越过 resource 边界".into());
    }
    let root = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?;
    let bundled = root.join("sidecars").join(relative);
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("sidecars")
        .join(relative);
    let target = if bundled.is_file() {
        bundled
    } else {
        development
    };
    if !target.is_file() {
        return Err(format!("sidecar 文件不存在: {}", entry_name(&target)));
    }
    Ok(target)
}

fn verify_file_hash(path: &Path, expected: &str) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| format!("无法读取 sidecar: {error}"))?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if !actual.eq_ignore_ascii_case(expected) {
        return Err("sidecar SHA-256 与受控 manifest 不匹配".into());
    }
    Ok(())
}

fn entry_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown-sidecar")
        .to_string()
}

fn current_target() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "windows") => "x86_64-pc-windows-msvc",
        ("aarch64", "windows") => "aarch64-pc-windows-msvc",
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        _ => "unsupported-target",
    }
}

fn initialize_statuses(app: &AppHandle) {
    let supervisor = app.state::<SidecarSupervisor>();
    match read_manifest(app).and_then(|manifest| {
        validate_manifest(&manifest)?;
        Ok(manifest)
    }) {
        Ok(manifest) => {
            for entry in manifest.sidecars {
                supervisor.set_status(initial_sidecar_status(&entry));
            }
        }
        Err(reason) => {
            for kind in ["runtime", "daemon"] {
                supervisor.set_status(SidecarStatus {
                    kind: kind.into(),
                    state: "version-mismatch".into(),
                    version: None,
                    reason: Some(reason.clone()),
                });
            }
        }
    }
}

fn initial_sidecar_status(entry: &SidecarEntry) -> SidecarStatus {
    SidecarStatus {
        kind: entry.kind.clone(),
        state: "unavailable".into(),
        version: Some(entry.version.clone()),
        reason: Some(if entry.bundled {
            "尚未启动；等待显式 lifecycle bootstrap".into()
        } else {
            "mock/dev 包未包含真实 sidecar 二进制".into()
        }),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .manage(SidecarSupervisor::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("external-navigation")
                .on_navigation(|webview, url| {
                    let internal = matches!(url.scheme(), "tauri" | "ipc")
                        || matches!(url.host_str(), Some("tauri.localhost"))
                        || (cfg!(debug_assertions)
                            && url.scheme() == "http"
                            && matches!(url.host_str(), Some("localhost") | Some("127.0.0.1"))
                            && url.port_or_known_default() == Some(1420));
                    if webview.label() != "main" || internal {
                        return true;
                    }
                    if validate_external_url(url.as_str()).is_ok() {
                        let _ = webview
                            .app_handle()
                            .opener()
                            .open_url(url.as_str(), None::<&str>);
                    }
                    false
                })
                .build(),
        )
        .setup(|app| {
            initialize_statuses(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            sidecar_status,
            ensure_sidecars,
            transport_configuration,
            runtime_rpc,
            design_rpc,
            open_external_url,
            validate_credential_reference,
            model_configuration_get,
            model_configuration_save,
            model_configuration_delete_credential,
            model_configuration_test,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Jingwei workbench");
    app.run(|app_handle, event| {
        if matches!(event, RunEvent::Exit | RunEvent::ExitRequested { .. }) {
            app_handle.state::<SidecarSupervisor>().shutdown();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(bundled: bool) -> SidecarEntry {
        SidecarEntry {
            kind: "daemon".into(),
            relative_path: format!(
                "bin/jingwei-daemon-{}{}",
                current_target(),
                if cfg!(windows) { ".exe" } else { "" }
            ),
            version: "0.1.0".into(),
            sha256: if bundled {
                "a".repeat(64)
            } else {
                "unavailable".into()
            },
            bundled,
        }
    }

    #[test]
    fn rejects_external_credentials_and_unsafe_schemes() {
        assert!(validate_external_url("https://example.com/evidence").is_ok());
        assert!(validate_external_url("javascript:alert(1)").is_err());
        assert!(validate_external_url("https://user:secret@example.com").is_err());
    }

    #[test]
    fn rejects_sidecar_path_traversal() {
        let mut invalid = entry(true);
        invalid.relative_path = "../jingwei-daemon.exe".into();
        assert!(validate_entry(&invalid, current_target()).is_err());
    }

    #[test]
    fn requires_hash_for_bundled_sidecar() {
        let mut invalid = entry(true);
        invalid.sha256 = "unavailable".into();
        assert!(validate_entry(&invalid, current_target()).is_err());
        assert!(validate_entry(&entry(false), current_target()).is_ok());
    }

    #[test]
    fn exposes_unavailable_status_for_dev_manifest_entries() {
        let status = initial_sidecar_status(&entry(false));
        assert_eq!(status.state, "unavailable");
        assert!(status.reason.unwrap().contains("未包含真实 sidecar"));
    }

    #[test]
    fn exposes_version_mismatch_and_crash_failures() {
        let sidecar = entry(true);
        let mismatch = sidecar_failure_status(&sidecar, "bad handshake", "version-mismatch");
        let crashed = sidecar_failure_status(&sidecar, "exit code 9", "crashed");
        assert_eq!(mismatch.state, "version-mismatch");
        assert_eq!(mismatch.reason.as_deref(), Some("bad handshake"));
        assert_eq!(crashed.state, "crashed");
        assert_eq!(crashed.reason.as_deref(), Some("exit code 9"));
    }

    #[test]
    fn handshake_is_version_and_target_bound() {
        let configured = entry(true);
        let valid = SidecarHandshake {
            protocol_version: "0.1".into(),
            version: "0.1.0".into(),
            target: current_target().into(),
            health: "healthy".into(),
            rpc_endpoint: None,
        };
        assert!(validate_handshake(&configured, &valid, None).is_ok());
        let mismatch = SidecarHandshake {
            protocol_version: "0.2".into(),
            ..valid
        };
        assert!(validate_handshake(&configured, &mismatch, None).is_err());
    }

    #[test]
    fn credential_bridge_only_accepts_opaque_references() {
        assert_eq!(
            validate_credential_reference("credential://windows/jingwei.openai".into()).unwrap(),
            "credential://windows/jingwei.openai"
        );
        assert!(validate_credential_reference("secret-value".into()).is_err());
        assert!(validate_credential_reference("credential://windows/UPPER".into()).is_err());
    }

    #[test]
    fn bundled_sidecars_are_resolved_below_the_sidecars_resource_directory() {
        let root = Path::new("resource-root");
        let relative = Path::new("bin/jingwei-daemon.exe");
        assert_eq!(
            root.join("sidecars").join(relative),
            PathBuf::from("resource-root/sidecars/bin/jingwei-daemon.exe")
        );
    }
}
