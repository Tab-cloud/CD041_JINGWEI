import { describe, expect, it } from "vitest";
import { ClientContractError } from "../../clients/errors";
import { MockRuntimeClient } from "./MockRuntimeClient";

describe("MockRuntimeClient durability", () => {
  it("deduplicates admission by client request id", async () => {
    const client = new MockRuntimeClient();
    const input = { sessionId: "session-main", text: "test", attachmentRefs: [], delivery: "queue" as const, clientRequestId: "request-1" };
    const first = await client.admitInput(input);
    const second = await client.admitInput(input);
    expect(second).toEqual(first);
  });

  it("replays only events after the cursor", async () => {
    const client = new MockRuntimeClient();
    const sequences: number[] = [];
    for await (const event of client.events("session-main", 2)) sequences.push(event.sequence);
    expect(sequences).toEqual([3, 4]);
  });

  it("surfaces a disconnect once so callers can resume by sequence", async () => {
    const client = new MockRuntimeClient("disconnect-once");
    const sequences: number[] = [];
    await expect(async () => {
      for await (const event of client.events("session-main", 0)) sequences.push(event.sequence);
    }).rejects.toBeInstanceOf(ClientContractError);
    for await (const event of client.events("session-main", sequences.at(-1))) sequences.push(event.sequence);
    expect(sequences).toEqual([1, 2, 3, 4]);
  });

  it("reports provider failure without fake completion", async () => {
    const client = new MockRuntimeClient("provider-failure");
    const run = await client.resumeRun("session-main");
    expect(run.status).toBe("failed");
    expect(run.timeline.some((item) => item.error?.code === "PROVIDER_STREAM_INTERRUPTED")).toBe(true);
  });
});
