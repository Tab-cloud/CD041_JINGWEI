import { afterEach, describe, expect, test } from "bun:test"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { RuntimeConflictError } from "@jingwei/agent-protocol"
import {
  createRuntime,
  FakeProvider,
  fakeCapabilities,
  MockJdpClient,
  SessionRunCoordinator,
  SpecialistRunner,
} from "../src/index.ts"

const tempDirectories: string[] = []

afterEach(() => {
  for (const directory of tempDirectories.splice(0)) {
    removeTemporaryDirectory(directory)
  }
})

describe("session and input contracts", () => {
  test("returns the same admitted input for an equivalent request ID and conflicts on changed content", async () => {
    const kit = await runtimeKit()
    try {
      const session = await kit.client.createSession({ projectId: "project", workspaceId: "workspace" })
      const input = {
        sessionId: session.id,
        clientRequestId: "request-1",
        delivery: "queue" as const,
        content: "Create a candidate",
      }
      const first = await kit.client.admitInput(input)
      const duplicate = await kit.client.admitInput(input)
      expect(duplicate).toEqual(first)
      expect(
        kit.events.readAfter(session.id).filter((event) => event.type === "input.admitted"),
      ).toHaveLength(1)
      await expect(kit.client.admitInput({ ...input, content: "Changed content" })).rejects.toBeInstanceOf(
        RuntimeConflictError,
      )
    } finally {
      kit[Symbol.dispose]()
    }
  })

  test("persists admitted input before any run starts and recovers it after reopen", async () => {
    const path = tempDatabase()
    const first = await runtimeKit(path)
    const session = await first.client.createSession({ projectId: "project", workspaceId: "workspace" })
    await first.client.admitInput({
      sessionId: session.id,
      clientRequestId: "durable-before-run",
      delivery: "queue",
      content: "Persist me first",
    })
    first[Symbol.dispose]()

    const second = await runtimeKit(path)
    try {
      expect(second.repository.listPendingInputs(session.id).map((input) => input.content)).toEqual([
        "Persist me first",
      ])
    } finally {
      second[Symbol.dispose]()
    }
  })
})

describe("session run coordinator", () => {
  test("joins duplicate resumes, coalesces wakes, and serializes one session", async () => {
    const coordinator = new SessionRunCoordinator<number>()
    const order: string[] = []
    let release = () => {}
    const gate = new Promise<void>((resolve) => {
      release = resolve
    })
    const first = coordinator.resume("session", async () => {
      order.push("first-start")
      await gate
      order.push("first-end")
      return 1
    })
    const joined = coordinator.resume("session", async () => 99)
    coordinator.wake("session", async () => {
      order.push("wake-old")
      return 2
    })
    coordinator.wake("session", async () => {
      order.push("wake-coalesced")
      return 3
    })
    expect(joined).toBe(first)
    release()
    expect(await first).toBe(3)
    expect(order).toEqual(["first-start", "first-end", "wake-coalesced"])
  })

  test("allows different sessions to execute concurrently", async () => {
    const coordinator = new SessionRunCoordinator<string>()
    let active = 0
    let maximum = 0
    const run = (label: string) =>
      coordinator.resume(label, async () => {
        active++
        maximum = Math.max(maximum, active)
        await Bun.sleep(20)
        active--
        return label
      })
    expect(await Promise.all([run("a"), run("b")])).toEqual(["a", "b"])
    expect(maximum).toBe(2)
  })
})

describe("runtime delivery and provider attempts", () => {
  test("applies steer at the next provider boundary and starts queue only after root completion", async () => {
    const kit = await runtimeKit()
    try {
      const provider = new FakeProvider([
        {
          steps: [
            { type: "delay", milliseconds: 30 },
            { type: "text-delta", text: "first boundary" },
            { type: "finish", reason: "stop" },
          ],
        },
        {
          steps: [
            { type: "text-delta", text: "steer applied" },
            { type: "finish", reason: "stop" },
          ],
        },
        {
          steps: [
            { type: "text-delta", text: "queued run" },
            { type: "finish", reason: "stop" },
          ],
        },
      ])
      kit.providers.register(provider)
      const session = await kit.client.createSession({ projectId: "project", workspaceId: "workspace" })
      await kit.client.admitInput({
        sessionId: session.id,
        clientRequestId: "initial",
        delivery: "queue",
        content: "initial request",
      })
      const running = kit.client.resumeRun(session.id)
      await waitFor(() => provider.calls.length === 1)
      await kit.client.admitInput({
        sessionId: session.id,
        clientRequestId: "steer",
        delivery: "steer",
        content: "change the active approach",
      })
      await kit.client.admitInput({
        sessionId: session.id,
        clientRequestId: "queued",
        delivery: "queue",
        content: "start only after completion",
      })
      const finalRun = await running

      expect(finalRun.output).toBe("queued run")
      expect(
        provider.calls[1]?.messages.some((message) => message.content === "change the active approach"),
      ).toBe(true)
      expect(
        provider.calls[2]?.messages.some((message) => message.content === "start only after completion"),
      ).toBe(true)
      expect(
        provider.calls[2]?.messages.some((message) => message.content === "change the active approach"),
      ).toBe(false)
      expect(kit.events.readAfter(session.id).filter((event) => event.type === "run.completed")).toHaveLength(
        2,
      )
    } finally {
      kit[Symbol.dispose]()
    }
  })

  test("discards partial text and tool calls before fallback to another fault domain", async () => {
    const kit = await runtimeKit()
    try {
      let toolCalls = 0
      const tool = kit.tools.register(
        {
          name: "design.query",
          description: "Query design",
          inputSchema: { type: "object" },
          source: "builtin",
          permission: "read-design",
          effects: ["read"],
          resourceClaims: ["design/project"],
          idempotency: "supported",
          cancellation: "supported",
          timeoutMs: 1_000,
          outputBudget: { inlineBytes: 1_024, overflow: "artifact" },
        },
        async () => ({ calls: ++toolCalls }),
      )
      kit.providers.register(
        new FakeProvider(
          [
            {
              steps: [
                { type: "text-delta", text: "discard-me" },
                {
                  type: "tool-call",
                  id: "partial-call",
                  name: "design.query",
                  input: {},
                  toolIdentity: tool.identity,
                },
                { type: "error", error: new Error("transport reset") },
              ],
            },
          ],
          fakeCapabilities({ provider: "failed", faultDomain: "domain-a" }),
        ),
      )
      kit.providers.register(
        new FakeProvider(
          [
            {
              steps: [
                { type: "text-delta", text: "clean fallback" },
                { type: "finish", reason: "stop" },
              ],
            },
          ],
          fakeCapabilities({ provider: "fallback", faultDomain: "domain-b" }),
        ),
      )
      const session = await kit.client.createSession({ projectId: "project", workspaceId: "workspace" })
      await kit.client.admitInput({
        sessionId: session.id,
        clientRequestId: "partial-failure",
        delivery: "queue",
        content: "test fallback",
      })
      const run = await kit.client.resumeRun(session.id)
      expect(run.output).toBe("clean fallback")
      expect(toolCalls).toBe(0)
      expect(JSON.stringify(kit.events.readAfter(session.id))).not.toContain("discard-me")
      expect(
        kit.events.readAfter(session.id).filter((event) => event.type === "model.attempt.failed"),
      ).toHaveLength(1)
    } finally {
      kit[Symbol.dispose]()
    }
  })

  test("ordinary no-tool completion terminates normally", async () => {
    const kit = await runtimeKit()
    try {
      kit.providers.register(
        new FakeProvider([
          {
            steps: [
              { type: "text-delta", text: "normal answer" },
              { type: "finish", reason: "stop" },
            ],
          },
        ]),
      )
      const session = await kit.client.createSession({ projectId: "project", workspaceId: "workspace" })
      await kit.client.admitInput({
        sessionId: session.id,
        clientRequestId: "no-tool",
        delivery: "queue",
        content: "hello",
      })
      expect((await kit.client.resumeRun(session.id)).status).toBe("completed")
    } finally {
      kit[Symbol.dispose]()
    }
  })

  test("runs specialists as root-only bounded tools and returns their result to main-agent", async () => {
    const kit = await runtimeKit()
    try {
      const specialist = new SpecialistRunner(kit.loop, kit.repository, kit.events)
      const specialistTool = specialist.register(kit.tools, {
        agentId: "pcb-engineer",
        role: "specialist",
        description: "Analyze a bounded PCB question",
        systemPrompt: "Return a structured candidate to main-agent.",
        tools: [],
        contextRefs: ["revision:1"],
        budget: { turns: 2, toolCalls: 0, outputTokens: 1_000 },
      })
      kit.providers.register(
        new FakeProvider([
          {
            steps: [
              {
                type: "tool-call",
                id: "specialist-call",
                name: specialistTool.descriptor.name,
                input: { task: "inspect placement", contextRefs: ["revision:1", "secret:other"] },
                toolIdentity: specialistTool.identity,
              },
              { type: "finish", reason: "tool-calls" },
            ],
          },
          {
            steps: [
              { type: "text-delta", text: "bounded specialist result" },
              { type: "finish", reason: "stop" },
            ],
          },
          {
            steps: [
              { type: "text-delta", text: "main final answer" },
              { type: "finish", reason: "stop" },
            ],
          },
        ]),
      )
      const session = await kit.client.createSession({ projectId: "project", workspaceId: "workspace" })
      await kit.client.admitInput({
        sessionId: session.id,
        clientRequestId: "specialist-root",
        delivery: "queue",
        content: "ask the PCB specialist",
      })
      expect((await kit.client.resumeRun(session.id)).output).toBe("main final answer")
      const started = kit.events
        .readAfter(session.id)
        .find((event) => event.type === "specialist.run.started")?.data as { contextRefs: string[] }
      expect(started.contextRefs).toEqual(["revision:1"])
      expect(kit.events.readAfter(session.id).map((event) => event.type)).toContain(
        "specialist.run.completed",
      )
    } finally {
      kit[Symbol.dispose]()
    }
  })

  test("cancels an active provider turn and persists a cancelled run terminal", async () => {
    const kit = await runtimeKit()
    try {
      const provider = new FakeProvider([
        {
          steps: [
            { type: "delay", milliseconds: 200 },
            { type: "text-delta", text: "must not complete" },
            { type: "finish", reason: "stop" },
          ],
        },
      ])
      kit.providers.register(provider)
      const session = await kit.client.createSession({ projectId: "project", workspaceId: "workspace" })
      await kit.client.admitInput({
        sessionId: session.id,
        clientRequestId: "cancel-active",
        delivery: "queue",
        content: "long request",
      })
      const running = kit.client.resumeRun(session.id)
      await waitFor(() => provider.calls.length === 1)
      const startedEvent = kit.events.readAfter(session.id).find((event) => event.type === "run.started")
      if (!startedEvent) throw new Error("run.started event was not published")
      const runId = (startedEvent.data as { runId: string }).runId
      await kit.client.cancelRun(runId, "test cancellation")
      expect((await running).status).toBe("cancelled")
      expect(kit.repository.getRun(runId)?.status).toBe("cancelled")
      expect(kit.events.readAfter(session.id).map((event) => event.type)).toContain("run.cancelled")
    } finally {
      kit[Symbol.dispose]()
    }
  })

  test("does not fall back to a model that lacks required native tools", async () => {
    const kit = await runtimeKit()
    try {
      kit.tools.register(
        {
          name: "design.query",
          description: "Query",
          inputSchema: { type: "object" },
          source: "builtin",
          permission: "read-design",
          effects: ["read"],
          resourceClaims: ["design/project"],
          idempotency: "supported",
          cancellation: "supported",
          timeoutMs: 1_000,
          outputBudget: { inlineBytes: 1_024, overflow: "artifact" },
        },
        async () => ({ ok: true }),
      )
      const unsupported = new FakeProvider(
        [
          {
            steps: [
              { type: "text-delta", text: "unsupported completion" },
              { type: "finish", reason: "stop" },
            ],
          },
        ],
        fakeCapabilities({ nativeTools: false }),
      )
      kit.providers.register(unsupported)
      const session = await kit.client.createSession({ projectId: "project", workspaceId: "workspace" })
      await kit.client.admitInput({
        sessionId: session.id,
        clientRequestId: "native-tool-required",
        delivery: "queue",
        content: "use the tool",
      })
      const run = await kit.client.resumeRun(session.id)
      expect(run.status).toBe("failed")
      expect(run.error?.summary).toContain("No provider supports required native tools")
      expect(unsupported.calls).toHaveLength(0)
    } finally {
      kit[Symbol.dispose]()
    }
  })
})

async function runtimeKit(path = tempDatabase()) {
  return createRuntime({
    databasePath: path,
    jdpClient: new MockJdpClient(),
    policyRules: [{ id: "test-allow", action: "allow", reason: "contract test" }],
  })
}

function tempDatabase(): string {
  const directory = mkdtempSync(join(tmpdir(), "jingwei-runtime-"))
  tempDirectories.push(directory)
  return join(directory, "runtime.db")
}

async function waitFor(predicate: () => boolean, timeoutMs = 1_000): Promise<void> {
  const deadline = Date.now() + timeoutMs
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error("Condition timed out")
    await Bun.sleep(2)
  }
}

function removeTemporaryDirectory(directory: string): void {
  try {
    rmSync(directory, { recursive: true, force: true, maxRetries: 5, retryDelay: 20 })
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "EBUSY") throw error
  }
}
