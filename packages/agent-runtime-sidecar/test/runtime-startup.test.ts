import { describe, expect, test } from "bun:test"
import { RuntimeOwnerLeaseConflictError } from "@jingwei/agent-runtime"
import { recoverWhenOwnerLeaseAvailable } from "../src/runtime-startup.ts"

describe("Runtime sidecar startup", () => {
  test("waits for a killed predecessor lease instead of crashing", async () => {
    let attempts = 0
    let clock = 0
    const result = await recoverWhenOwnerLeaseAvailable(
      {
        recover: async () => {
          attempts += 1
          if (attempts < 3) throw new RuntimeOwnerLeaseConflictError()
          return []
        },
      },
      {
        timeoutMs: 1_000,
        retryDelayMs: 100,
        now: () => clock,
        sleep: async (milliseconds) => {
          clock += milliseconds
        },
      },
    )

    expect(result).toEqual([])
    expect(attempts).toBe(3)
    expect(clock).toBe(200)
  })

  test("fails closed when a live owner keeps the lease", async () => {
    let clock = 0
    await expect(
      recoverWhenOwnerLeaseAvailable(
        { recover: async () => Promise.reject(new RuntimeOwnerLeaseConflictError()) },
        {
          timeoutMs: 200,
          retryDelayMs: 100,
          now: () => clock,
          sleep: async (milliseconds) => {
            clock += milliseconds
          },
        },
      ),
    ).rejects.toThrow("remained owned after 200 ms")
  })
})
