import { describe, expect, it } from "vitest";
import type { DisplayPrimitive } from "../clients/contracts";
import { buildSyntheticDisplayList, canvasPointToWorld, cullDisplayList, hitDistance } from "./displayList";

describe("display-list geometry helpers", () => {
  it("culls a 100k primitive fixture to the viewport", () => {
    const primitives = buildSyntheticDisplayList(100_000);
    const started = performance.now();
    const visible = cullDisplayList(primitives, { x: 100, y: 100, width: 1000, height: 600, zoom: 1 });
    const duration = performance.now() - started;
    expect(visible.length).toBeGreaterThan(100);
    expect(visible.length).toBeLessThan(primitives.length / 4);
    expect(duration).toBeLessThan(1500);
  });

  it("hit-tests both boxes and polylines", () => {
    const pad: DisplayPrimitive = { id: "pad", entityId: "pad", kind: "pad", layerId: "F.Cu", position: { x: 20, y: 20 }, width: 10, height: 10, zIndex: 1 };
    const track: DisplayPrimitive = { id: "track", entityId: "track", kind: "track", layerId: "F.Cu", points: [{ x: 0, y: 0 }, { x: 100, y: 0 }], width: 4, zIndex: 1 };
    expect(hitDistance(pad, { x: 22, y: 20 })).toBe(0);
    expect(hitDistance(track, { x: 50, y: 3 })).toBeCloseTo(3);
  });

  it("maps pointer coordinates through viewport zoom", () => {
    expect(canvasPointToWorld({ x: 120, y: 90 }, { left: 20, top: 10 }, { x: 50, y: 30, zoom: 2 })).toEqual({ x: 100, y: 70 });
  });
});
