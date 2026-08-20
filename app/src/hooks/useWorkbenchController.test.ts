import { afterEach, describe, expect, it, vi } from "vitest";
import type { DesignClient, RuntimeClient } from "../clients/contracts";
import { waitForSidecarSettlement, waitForTransportReadiness } from "./useWorkbenchController";

afterEach(() => {
  vi.useRealTimers();
});

describe("installed workbench transport readiness", () => {
  it("allows a killed Runtime lease to expire before declaring startup failure", async () => {
    let clock = 0;
    const refresh = vi.fn()
      .mockResolvedValueOnce([{ kind: "runtime", state: "starting" }])
      .mockResolvedValueOnce([{ kind: "runtime", state: "healthy" }]);

    const sidecars = await waitForSidecarSettlement(
      async () => [{ kind: "runtime", state: "starting" }],
      refresh,
      {
        timeoutMs: 20_000,
        pollIntervalMs: 5_000,
        now: () => clock,
        sleep: async (milliseconds) => { clock += milliseconds; },
      },
    );

    expect(sidecars).toEqual([{ kind: "runtime", state: "healthy" }]);
    expect(clock).toBe(10_000);
  });

  it("retries read-only capability probes before any project or session mutation", async () => {
    vi.useFakeTimers();
    const runtimeCapabilities = vi.fn()
      .mockRejectedValueOnce("named pipe did not become available")
      .mockResolvedValue({ contractVersion: "0.1", mode: "real", state: "connected" });
    const designCapabilities = vi.fn()
      .mockResolvedValue({ contractVersion: "0.1", mode: "real", state: "connected" });
    const readiness = waitForTransportReadiness(
      { capabilities: runtimeCapabilities } as unknown as RuntimeClient,
      { capabilities: designCapabilities } as unknown as DesignClient,
    );

    await vi.runAllTimersAsync();

    await expect(readiness).resolves.toMatchObject({
      runtimeCapabilities: { state: "connected" },
      designCapabilities: { state: "connected" },
    });
    expect(runtimeCapabilities).toHaveBeenCalledTimes(2);
    expect(designCapabilities).toHaveBeenCalledTimes(2);
  });
});
