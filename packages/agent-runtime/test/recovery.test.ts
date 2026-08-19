import { afterEach, describe, expect, test } from "bun:test"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { createRuntime, MockJdpClient } from "../src/index.ts"

const tempDirectories: string[] = []

afterEach(() => {
  for (const directory of tempDirectories.splice(0)) {
    removeTemporaryDirectory(directory)
  }
})

describe("restart reconciliation", () => {
  test("discards unfinished provider streams and requeues their root run", async () => {
    const kit = await runtimeKit()
    try {
      const session = kit.repository.createSession({ projectId: "project", workspaceId: "workspace" })
      const run = kit.repository.createRun(session.id, "root", "main-agent")
      kit.repository.updateRun(run.id, { status: "running" })
      const attempt = kit.repository.createModelAttempt({
        sessionId: session.id,
        runId: run.id,
        provider: "fake",
        model: "partial",
        faultDomain: "domain",
        status: "streaming",
      })
      const actions = await kit.recovery.recover()
      expect(actions).toEqual(
        expect.arrayContaining([
          expect.objectContaining({ kind: "model-attempt-discarded", entityId: attempt.id }),
          expect.objectContaining({ kind: "run-requeued", entityId: run.id }),
        ]),
      )
      expect(kit.repository.listIncompleteAttempts()).toEqual([])
      expect(kit.repository.getRun(run.id)?.status).toBe("queued")
    } finally {
      kit[Symbol.dispose]()
    }
  })

  test("classifies a recorded pre-handler tool as safe to reauthorize", async () => {
    const kit = await runtimeKit()
    try {
      const now = new Date().toISOString()
      kit.executions.create({
        id: "recorded-execution",
        sessionId: "session",
        runId: "run",
        toolCallId: "call",
        toolName: "design.query",
        toolIdentity: "identity",
        input: {},
        inputHash: "hash",
        effects: ["read"],
        resourceClaims: ["design/project"],
        status: "recorded",
        artifactRefs: [],
        createdAt: now,
        updatedAt: now,
      })
      expect(await kit.recovery.recover()).toContainEqual(
        expect.objectContaining({ kind: "tool-safe-to-resume", entityId: "recorded-execution" }),
      )
    } finally {
      kit[Symbol.dispose]()
    }
  })

  test("keeps durable approval visible across runtime reopen", async () => {
    const path = tempDatabase()
    const first = await runtimeKit(path)
    const approval = first.approvals.request({
      sessionId: "session",
      runId: "run",
      toolCallId: "call",
      toolName: "network.search",
      permission: "network",
      effects: ["network"],
      resourceClaims: ["supplier/example"],
      input: { token: "secret" },
      reason: "External recipient",
    })
    first[Symbol.dispose]()

    const second = await runtimeKit(path)
    try {
      expect(second.approvals.list("session")).toContainEqual(
        expect.objectContaining({ id: approval.id, status: "pending" }),
      )
      second.approvals.reply({ approvalId: approval.id, decision: "deny" })
      expect(second.approvals.list("session")[0]?.status).toBe("denied")
    } finally {
      second[Symbol.dispose]()
    }
  })

  test("quarantines unknown external side effects instead of retrying", async () => {
    const kit = await runtimeKit()
    try {
      const now = new Date().toISOString()
      kit.executions.create({
        id: "unknown-external",
        sessionId: "session",
        runId: "run",
        toolCallId: "call",
        toolName: "mcp.external.write",
        toolIdentity: "identity",
        input: {},
        inputHash: "hash",
        effects: ["network", "workspace-write"],
        resourceClaims: ["external/system"],
        status: "running",
        artifactRefs: [],
        createdAt: now,
        updatedAt: now,
      })
      const actions = await kit.recovery.recover()
      expect(actions).toContainEqual(
        expect.objectContaining({ kind: "tool-quarantined", entityId: "unknown-external" }),
      )
      expect(kit.executions.get("unknown-external")?.status).toBe("unknown")
    } finally {
      kit[Symbol.dispose]()
    }
  })
})

async function runtimeKit(path = tempDatabase()) {
  return createRuntime({
    databasePath: path,
    jdpClient: new MockJdpClient(),
    policyRules: [{ id: "allow", action: "allow", reason: "test" }],
  })
}

function tempDatabase(): string {
  const directory = mkdtempSync(join(tmpdir(), "jingwei-recovery-"))
  tempDirectories.push(directory)
  return join(directory, "runtime.db")
}

function removeTemporaryDirectory(directory: string): void {
  try {
    rmSync(directory, { recursive: true, force: true, maxRetries: 5, retryDelay: 20 })
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "EBUSY") throw error
  }
}
