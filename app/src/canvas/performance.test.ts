import { describe, expect, it } from "vitest";
import { buildSyntheticDisplayList, cullDisplayList } from "./displayList";

interface Measurement {
  count: number;
  buildMs: number;
  cullMs: number;
  visible: number;
}

function measure(count: number): Measurement {
  const buildStarted = performance.now();
  const primitives = buildSyntheticDisplayList(count);
  const buildMs = performance.now() - buildStarted;
  const cullStarted = performance.now();
  const visible = cullDisplayList(primitives, { x: 100, y: 100, width: 1000, height: 600, zoom: 1 });
  return {
    count,
    buildMs: Number(buildMs.toFixed(2)),
    cullMs: Number((performance.now() - cullStarted).toFixed(2)),
    visible: visible.length,
  };
}

describe("display-list performance acceptance", () => {
  it.each([10_000, 100_000])("culls %i primitives within the desktop budget", (count) => {
    const result = measure(count);
    console.info(`PERF_RESULT ${JSON.stringify(result)}`);
    expect(result.visible).toBeGreaterThan(100);
    expect(result.visible).toBeLessThan(count);
    expect(result.cullMs).toBeLessThan(count === 10_000 ? 100 : 500);
  });
});
