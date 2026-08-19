import type { SidecarStatus } from "../store/workbenchState";

interface TauriWindow extends Window {
  __TAURI_INTERNALS__?: unknown;
}

export function isTauriRuntime(): boolean {
  return Boolean((window as TauriWindow).__TAURI_INTERNALS__);
}

export async function getSidecarStatuses(): Promise<SidecarStatus[]> {
  if (!isTauriRuntime()) return browserFallbackStatuses();
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<SidecarStatus[]>("sidecar_status");
}

export async function ensureSidecars(): Promise<SidecarStatus[]> {
  if (!isTauriRuntime()) return browserFallbackStatuses();
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<SidecarStatus[]>("ensure_sidecars");
}

export async function openExternalUrl(url: string): Promise<void> {
  const parsed = new URL(url);
  if (!['http:', 'https:'].includes(parsed.protocol) || parsed.username || parsed.password) {
    throw new Error("仅允许无内嵌凭据的 HTTP(S) 外部链接");
  }
  if (!isTauriRuntime()) {
    window.open(parsed.toString(), "_blank", "noopener,noreferrer");
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("open_external_url", { url: parsed.toString() });
}

export async function chooseProjectFolder(): Promise<boolean> {
  if (!isTauriRuntime()) return true;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({ directory: true, multiple: false, title: "选择 Jingwei 项目目录" });
  return typeof selected === "string";
}

export async function validateCredentialReference(reference: string): Promise<string> {
  if (!isTauriRuntime()) {
    if (!/^credential:\/\/windows\/[a-z0-9][a-z0-9._-]{2,80}$/u.test(reference)) throw new Error("凭据引用格式无效");
    return reference;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("validate_credential_reference", { reference });
}

function browserFallbackStatuses(): SidecarStatus[] {
  return [
    { kind: "runtime", state: "unavailable", reason: "浏览器预览未启动 runtime sidecar" },
    { kind: "daemon", state: "unavailable", reason: "浏览器预览未启动 daemon sidecar" },
  ];
}
