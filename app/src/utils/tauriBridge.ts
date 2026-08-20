import type { SidecarStatus } from "../store/workbenchState";
import {
  EMPTY_MODEL_CONFIGURATION,
  type ModelConfigurationView,
  type ProviderConnectionTestResult,
  type SaveModelConfigurationInput,
} from "../model/modelConfiguration";

interface TauriWindow extends Window {
  __TAURI_INTERNALS__?: unknown;
}

export interface TransportConfiguration {
  protocolVersion: "0.1";
  workspaceId: string;
  projectId: string;
  mode: "real";
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

export async function getTransportConfiguration(): Promise<TransportConfiguration> {
  if (!isTauriRuntime()) throw new Error("真实 transport 只在 Jingwei 桌面应用中可用");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<TransportConfiguration>("transport_configuration");
}

export async function invokeRuntimeRpc<T>(method: string, params: unknown = {}): Promise<T> {
  if (!isTauriRuntime()) throw new Error("Runtime RPC 只在 Jingwei 桌面应用中可用");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>("runtime_rpc", { method, params });
}

export async function invokeDesignRpc<T>(request: unknown): Promise<T> {
  if (!isTauriRuntime()) throw new Error("JDP transport 只在 Jingwei 桌面应用中可用");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>("design_rpc", { request });
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
    if (!/^credential:\/\/(?:windows|macos|linux)\/[a-z0-9][a-z0-9._-]{2,79}$/u.test(reference)) throw new Error("凭据引用格式无效");
    return reference;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<string>("validate_credential_reference", { reference });
}

export async function getModelConfiguration(): Promise<ModelConfigurationView> {
  if (!isTauriRuntime()) return structuredClone(EMPTY_MODEL_CONFIGURATION);
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ModelConfigurationView>("model_configuration_get");
}

export async function saveModelConfiguration(input: SaveModelConfigurationInput): Promise<ModelConfigurationView> {
  if (!isTauriRuntime()) throw new Error("模型配置只能在原生桌面应用中保存");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ModelConfigurationView>("model_configuration_save", { input });
}

export async function deleteModelCredential(): Promise<ModelConfigurationView> {
  if (!isTauriRuntime()) throw new Error("系统凭据只能在原生桌面应用中管理");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ModelConfigurationView>("model_configuration_delete_credential");
}

export async function testModelConnection(): Promise<ProviderConnectionTestResult> {
  if (!isTauriRuntime()) throw new Error("模型连接测试只能在原生桌面应用中执行");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ProviderConnectionTestResult>("model_configuration_test");
}

function browserFallbackStatuses(): SidecarStatus[] {
  return [
    { kind: "runtime", state: "unavailable", reason: "浏览器预览未启动 runtime sidecar" },
    { kind: "daemon", state: "unavailable", reason: "浏览器预览未启动 daemon sidecar" },
  ];
}
