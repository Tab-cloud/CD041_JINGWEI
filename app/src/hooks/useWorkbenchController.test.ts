import { afterEach, describe, expect, it, vi } from "vitest";
import type { DesignClient, RuntimeClient } from "../clients/contracts";
import { waitForTransportReadiness } from "./useWorkbenchController";

afterEach(() => {
  vi.useRealTimers();
});

describe("installed workbench transport readiness", () => {
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
