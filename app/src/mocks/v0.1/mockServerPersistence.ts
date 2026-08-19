const MOCK_SERVER_PREFIX = "jingwei.mock-server.v0.1";

interface TauriWindow extends Window {
  __TAURI_INTERNALS__?: unknown;
}

export function loadMockServerState<T>(domain: "design" | "runtime"): T | undefined {
  if (!canPersistMockServer()) return undefined;
  try {
    const serialized = sessionStorage.getItem(`${MOCK_SERVER_PREFIX}.${domain}`);
    return serialized ? JSON.parse(serialized) as T : undefined;
  } catch {
    return undefined;
  }
}

export function saveMockServerState(domain: "design" | "runtime", state: unknown): void {
  if (!canPersistMockServer()) return;
  sessionStorage.setItem(`${MOCK_SERVER_PREFIX}.${domain}`, JSON.stringify(state));
}

function canPersistMockServer(): boolean {
  return typeof window !== "undefined" && Boolean((window as TauriWindow).__TAURI_INTERNALS__);
}
