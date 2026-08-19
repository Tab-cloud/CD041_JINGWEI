mod credential_store;
mod model_configuration;

use model_configuration::{
    model_configuration_delete_credential, model_configuration_get, model_configuration_save,
    validate_credential_reference,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

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
}

#[derive(Default)]
struct SidecarSupervisor {
    statuses: Mutex<HashMap<String, SidecarStatus>>,
    children: Mutex<HashMap<String, CommandChild>>,
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
}

#[tauri::command]
fn sidecar_status(state: tauri::State<'_, SidecarSupervisor>) -> Vec<SidecarStatus> {
    state.status_list()
}

#[tauri::command]
async fn ensure_sidecars(app: AppHandle) -> Result<Vec<SidecarStatus>, String> {
    let manifest = read_manifest(&app)?;
    validate_manifest(&manifest)?;
    for entry in manifest.sidecars.clone() {
        if let Err(reason) = start_sidecar(&app, &manifest, entry.clone()) {
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
    let bootstrap_token = uuid::Uuid::new_v4().to_string();
    let (mut events, child) = app
        .shell()
        .command(executable.to_string_lossy().to_string())
        .env(
            "JINGWEI_DESKTOP_PROTOCOL_VERSION",
            &manifest.protocol_version,
        )
        .env("JINGWEI_BOOTSTRAP_TOKEN", bootstrap_token)
        .spawn()
        .map_err(|error| format!("无法启动 {} sidecar: {error}", entry.kind))?;

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
                    let parsed = serde_json::from_slice::<SidecarHandshake>(&bytes)
                        .map_err(|_| "handshake 不是有效 JSON".to_string())
                        .and_then(|handshake| validate_handshake(&entry, &handshake));
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

fn validate_handshake(entry: &SidecarEntry, handshake: &SidecarHandshake) -> Result<(), String> {
    if handshake.protocol_version != EXPECTED_PROTOCOL_VERSION {
        return Err("sidecar protocol version mismatch".into());
    }
    if handshake.version != entry.version || handshake.target != current_target() {
        return Err("sidecar version 或 target handshake mismatch".into());
    }
    if handshake.health != "healthy" {
        return Err("sidecar handshake 未报告 healthy".into());
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
    let target = root.join(relative);
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
            open_external_url,
            validate_credential_reference,
            model_configuration_get,
            model_configuration_save,
            model_configuration_delete_credential,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build Jingwei workbench");
    app.run(|_, _| {});
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
        };
        assert!(validate_handshake(&configured, &valid).is_ok());
        let mismatch = SidecarHandshake {
            protocol_version: "0.2".into(),
            ..valid
        };
        assert!(validate_handshake(&configured, &mismatch).is_err());
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
}
