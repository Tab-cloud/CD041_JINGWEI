import { beforeEach, describe, expect, it, vi } from "vitest";
import { DesignTransportClient } from "./DesignTransportClient";

const bridge = vi.hoisted(() => ({
  invokeDesignRpc: vi.fn<(request: unknown) => Promise<unknown>>(),
  getTransportConfiguration: vi.fn(async () => ({
    protocolVersion: "0.1" as const,
    workspaceId: "workspace-test",
    projectId: "project-test",
    mode: "real" as const,
  })),
}));

vi.mock("../utils/tauriBridge", () => bridge);

describe("DesignTransportClient native logical adapter", () => {
  beforeEach(() => bridge.invokeDesignRpc.mockReset());

  it("maps native daemon display primitives without replacing stable entity IDs", async () => {
    bridge.invokeDesignRpc.mockResolvedValue({
      success: true,
      requestId: "request-view",
      executionId: "execution-view",
      data: {
        revision: "revision-logical",
        artifactRefs: [],
        primitives: [
          { kind: "glyph", position: { x: 220.98, y: 210.82 }, name: "U1", entityId: "symbol:u1" },
          { kind: "line", from: { x: 220.98, y: 210.82 }, to: { x: 469.9, y: 210.82 }, entityId: "wire:I2C_SCL" },
        ],
      },
    });
    const client = new DesignTransportClient();
    const display = await client.getView({
      projectId: "project-test",
      revision: "revision-logical",
      view: "schematic",
      viewport: { x: 0, y: 0, width: 1_000, height: 700, zoom: 1 },
      visibleLayerIds: ["logical"],
      lod: 2,
    });

    expect(display.primitives).toEqual([
      expect.objectContaining({ kind: "symbol", entityId: "symbol:u1", label: "U1" }),
      expect.objectContaining({ kind: "wire", entityId: "wire:I2C_SCL" }),
    ]);
    const request = jdpCall(0);
    expect(request.operation).toBe("design.view");
    expect(request.input.provider).toBe("logical.schematic-view");
  });

  it("routes native hit testing and semantic move preview through JDP", async () => {
    bridge.invokeDesignRpc
      .mockResolvedValueOnce({
        success: true,
        requestId: "request-hit",
        executionId: "execution-hit",
        data: { hits: [{ entityId: "symbol:u1", distance: 0, primitiveKind: "glyph" }] },
      })
      .mockResolvedValueOnce({
        success: true,
        requestId: "request-preview",
        executionId: "execution-preview",
        data: { previewHash: "preview-native", validation: { valid: true, findings: [] } },
      });
    const client = new DesignTransportClient();
    await expect(client.hitTest({
      projectId: "project-test",
      revision: "revision-logical",
      view: "schematic",
      point: { x: 220.98, y: 210.82 },
      tolerance: 2,
      visibleLayerIds: ["logical"],
    })).resolves.toEqual(expect.objectContaining({ entityIds: ["symbol:u1"] }));
    await expect(client.previewEdit({
      editSessionId: "edit-session",
      baseDraftRevision: "revision-logical",
      label: "移动 U1",
      commands: [{ kind: "schematic.move_symbol", entityId: "symbol:u1", position: { x: 320, y: 215 } }],
      expectedPostconditions: ["graph.integrity", "native-erc"],
    })).resolves.toEqual(expect.objectContaining({ previewId: "preview-native", committable: true }));

    const previewRequest = jdpCall(1);
    expect(previewRequest.operation).toBe("edit_command.preview");
    expect(previewRequest.input.commands).toEqual([{
      kind: "schematic.move-symbol",
      payload: { symbolId: "symbol:u1", position: { x: 320, y: 215 } },
    }]);
  });
});

function jdpCall(index: number): { operation: string; input: Record<string, unknown> } {
  const value = bridge.invokeDesignRpc.mock.calls[index]?.[0];
  if (typeof value !== "object" || value === null || !("operation" in value) || !("input" in value)) {
    throw new Error(`missing JDP call ${index}`);
  }
  return value as { operation: string; input: Record<string, unknown> };
}
