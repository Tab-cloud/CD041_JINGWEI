import type { DesignViewKind, Viewport } from "../clients/contracts";

const RECOVERY_KEY = "jingwei.workbench.recovery.v0.1";

export interface RecoveryRecord {
  schemaVersion: "0.1";
  projectId: string;
  editSessionId: string;
  lastRuntimeSequence: number;
  lastDesignSequence: number;
  activeView: DesignViewKind;
  viewports: Record<DesignViewKind, Viewport>;
  savedAt: string;
}

export function loadRecovery(): RecoveryRecord | undefined {
  try {
    const value = sessionStorage.getItem(RECOVERY_KEY);
    if (!value) return undefined;
    const parsed = JSON.parse(value) as Partial<RecoveryRecord>;
    if (parsed.schemaVersion !== "0.1" || !parsed.projectId || !parsed.editSessionId || !parsed.viewports) return undefined;
    return parsed as RecoveryRecord;
  } catch {
    return undefined;
  }
}

export function saveRecovery(record: RecoveryRecord): void {
  sessionStorage.setItem(RECOVERY_KEY, JSON.stringify(record));
}

export function clearRecovery(): void {
  sessionStorage.removeItem(RECOVERY_KEY);
}
