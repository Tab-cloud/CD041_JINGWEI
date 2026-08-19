import { beforeEach, describe, expect, it } from "vitest";
import type { EditCommandGroup } from "../../clients/contracts";
import { ClientContractError } from "../../clients/errors";
import { MockDesignClient } from "./MockDesignClient";

describe("MockDesignClient edit contract", () => {
  let client: MockDesignClient;
  let sessionId: string;
  let draftRevision: string;

  beforeEach(async () => {
    client = new MockDesignClient();
    const project = await client.getProject("project-golden-rs485");
    const session = await client.openEditSession({ projectId: project.id, baseRevision: project.draftRevision ?? project.candidateRevision });
    sessionId = session.id;
    draftRevision = session.draftRevision;
  });

  it("keeps preview transient and advances only draft on commit", async () => {
    const group: EditCommandGroup = {
      editSessionId: sessionId,
      baseDraftRevision: draftRevision,
      label: "move symbol",
      commands: [{ kind: "schematic.move_symbol", entityId: "symbol-u1", position: { x: 350, y: 310 } }],
      expectedPostconditions: [],
    };
    const before = await client.getView({ projectId: "project-golden-rs485", revision: draftRevision, view: "schematic", viewport: { x: 0, y: 0, width: 1200, height: 800, zoom: 1 }, visibleLayerIds: ["logical", "findings"], lod: 1 });
    const preview = await client.previewEdit(group);
    const afterPreview = await client.getView({ projectId: "project-golden-rs485", revision: draftRevision, view: "schematic", viewport: { x: 0, y: 0, width: 1200, height: 800, zoom: 1 }, visibleLayerIds: ["logical", "findings"], lod: 1 });
    expect(afterPreview.primitives).toEqual(before.primitives);
    expect(preview.ghostPrimitives[0]?.position).toEqual({ x: 350, y: 310 });

    const result = await client.commitEdit({ ...group, previewId: preview.previewId });
    expect(result.newDraftRevision).not.toBe(draftRevision);
    expect(result.currentProjectRevisionChanged).toBe(false);
    const project = await client.getProject("project-golden-rs485");
    expect(project.currentRevision).toBe("rev-current-004");
    expect(project.draftRevision).toBe(result.newDraftRevision);
  });

  it("rejects locked object edits during preview", async () => {
    const preview = await client.previewEdit({
      editSessionId: sessionId,
      baseDraftRevision: draftRevision,
      label: "move locked connector",
      commands: [{ kind: "pcb.move_footprint", entityId: "footprint-j1", position: { x: 900, y: 400 }, rotation: 90 }],
      expectedPostconditions: [],
    });
    expect(preview.committable).toBe(false);
    expect(preview.conflicts[0]?.code).toBe("LOCKED_OBJECT_CONFLICT");
  });

  it("returns stale-base conflict instead of last-write-wins", async () => {
    const preview = await client.previewEdit({
      editSessionId: sessionId,
      baseDraftRevision: "rev-stale",
      label: "stale",
      commands: [{ kind: "schematic.move_symbol", entityId: "symbol-u1", position: { x: 1, y: 1 } }],
      expectedPostconditions: [],
    });
    expect(preview.committable).toBe(false);
    expect(preview.conflicts.some((conflict) => conflict.code === "STALE_BASE_REVISION")).toBe(true);
  });

  it("creates auditable undo and redo revisions", async () => {
    const group: EditCommandGroup = {
      editSessionId: sessionId,
      baseDraftRevision: draftRevision,
      label: "add via",
      commands: [{ kind: "pcb.add_via", netId: "net-rs485-a", position: { x: 600, y: 300 }, viaClassId: "default" }],
      expectedPostconditions: [],
    };
    const preview = await client.previewEdit(group);
    const committed = await client.commitEdit({ ...group, previewId: preview.previewId });
    const undone = await client.undo(sessionId);
    const redone = await client.redo(sessionId);
    expect(new Set([committed.newDraftRevision, undone.newDraftRevision, redone.newDraftRevision]).size).toBe(3);
    expect(redone.currentProjectRevisionChanged).toBe(false);
  });

  it("blocks stale AI proposals", async () => {
    const proposal = await client.proposeAiPatch({ projectId: "project-golden-rs485", baseRevision: "old", instruction: "change", entityIds: [] });
    expect(proposal.status).toBe("stale");
    await expect(client.acceptPatchHunks({ proposalId: proposal.id, baseRevision: proposal.baseRevision, acceptedHunkIds: ["hunk-track"], rejectedHunkIds: [] })).rejects.toBeInstanceOf(ClientContractError);
  });
});
