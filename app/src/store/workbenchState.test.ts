import { describe, expect, it } from "vitest";
import type { DurableEvent } from "../clients/contracts";
import { initialWorkbenchState, workbenchReducer } from "./workbenchState";

describe("workbenchReducer", () => {
  it("advances draft only after a commit result", () => {
    const unchanged = workbenchReducer(initialWorkbenchState, { type: "set-preview", preview: undefined });
    expect(unchanged.project).toBeUndefined();

    const project = {
      id: "project",
      name: "fixture",
      currentRevision: "current-1",
      candidateRevision: "candidate-1",
      draftRevision: "draft-1",
      maturity: "candidate" as const,
      stages: [],
      releaseBlockers: [],
      sheets: [],
      layers: [],
      findings: [],
    };
    const loaded = workbenchReducer(initialWorkbenchState, {
      type: "loaded",
      project,
      runtimeCapabilities: { contractVersion: "0.1", mode: "mock", state: "connected", replay: true, approvals: true },
      designCapabilities: { contractVersion: "0.1", mode: "mock", state: "connected" },
      approvals: [],
      editSession: { id: "edit", baseRevision: "candidate-1", draftRevision: "draft-1", leaseExpiresAt: "later", recovered: false },
      sidecars: [],
    });
    const committed = workbenchReducer(loaded, { type: "commit-result", draftRevision: "draft-2", undoAvailable: true, redoAvailable: false, notice: "committed" });
    expect(committed.project?.draftRevision).toBe("draft-2");
    expect(committed.project?.currentRevision).toBe("current-1");
    expect(committed.editSession?.draftRevision).toBe("draft-2");
  });

  it("deduplicates stale durable events by sequence", () => {
    const event = (sequence: number): DurableEvent => ({ schemaVersion: "0.1", aggregateId: "session", sequence, timestamp: "now", type: "run.started", data: {} });
    const first = workbenchReducer(initialWorkbenchState, { type: "event", event: event(2) });
    const stale = workbenchReducer(first, { type: "event", event: event(1) });
    expect(stale.runtimeEvents).toHaveLength(1);
    expect(stale.lastRuntimeSequence).toBe(2);
  });

  it("restores the durable cursor without moving it backwards", () => {
    const restored = workbenchReducer(initialWorkbenchState, { type: "restore-cursor", lastRuntimeSequence: 6 });
    const stale = workbenchReducer(restored, { type: "restore-cursor", lastRuntimeSequence: 4 });
    expect(stale.lastRuntimeSequence).toBe(6);
  });

  it("separates transient interaction from view selection", () => {
    const withInteraction = workbenchReducer(initialWorkbenchState, { type: "interaction-start", interaction: { point: { x: 10, y: 12 }, entityId: "symbol-u1" } });
    const changedView = workbenchReducer(withInteraction, { type: "set-view", view: "pcb" });
    expect(changedView.interactionStart).toBeUndefined();
    expect(changedView.selectedIds).toEqual([]);
  });
});
