import { describe, expect, test } from "bun:test"
import { DurableEventBus, MemoryEventStore } from "@jingwei/agent-events"
import { RecordingRuntimeTracer } from "@jingwei/agent-observability"
import { ApprovalService, MemoryApprovalRepository, PolicyEngine } from "@jingwei/agent-policy"
import type {
  DurableEvent,
  ExecutionReconciliation,
  JdpCapabilities,
  JdpClient,
  JdpFailure,
  JdpRequest,
  JdpSuccess,
} from "@jingwei/agent-protocol"
import { RuntimeConflictError } from "@jingwei/agent-protocol"
import {
  JdpCommittedWithoutResponseError,
  MemoryArtifactStore,
  MemoryToolExecutionRepository,
  ResourceLeaseManager,
  registerGeneratedJdpTool,
  ToolRegistry,
  ToolSettlementPipeline,
} from "../src/index.ts"

describe("canonical tool settlement", () => {
  test("visible but denied tools never reach their handler", async () => {
    const setup = createSetup("deny")
    let calls = 0
    const tool = setup.registry.register(descriptor(), async () => {
      calls++
      return { ok: true }
    })
    const result = await setup.pipeline.execute(request(tool.identity))
    expect(result.status).toBe("denied")
    expect(calls).toBe(0)
    expect(setup.executions.get(result.executionId)?.status).toBe("denied")
    expect(setup.events.readAfter("session").map((event) => event.type)).toContain("tool.call.recorded")
  })

  test("records running execution before the handler side effect", async () => {
    const setup = createSetup("allow")
    let observedStatus = ""
    const tool = setup.registry.register(descriptor(), async () => {
      observedStatus = setup.executions.findIdempotent("session", "design.query", "run:call")?.status ?? ""
      return { ok: true }
    })
    const result = await setup.pipeline.execute(request(tool.identity))
    expect(result.status).toBe("succeeded")
    expect(observedStatus).toBe("running")
  })

  test("rejects stale identity and invalid schema before execution", async () => {
    const setup = createSetup("allow")
    let calls = 0
    const first = setup.registry.register(descriptor(), async () => {
      calls++
      return { ok: true }
    })
    setup.registry.register({ ...descriptor(), description: "updated" }, async () => ({ ok: true }))
    const stale = await setup.pipeline.execute(request(first.identity))
    expect(stale.error?.code).toBe("stale-tool")

    const current = setup.registry.get("design.query")
    const invalid = await setup.pipeline.execute({
      ...request(current?.identity ?? ""),
      toolCallId: "invalid-call",
      idempotencyKey: "run:invalid-call",
      input: { query: 42 },
    })
    expect(invalid.error?.summary).toContain("schema violation")
    expect(calls).toBe(0)
  })

  test("artifacts oversized output and exposes only bounded redacted data", async () => {
    const setup = createSetup("allow")
    const tool = setup.registry.register(
      { ...descriptor(), outputBudget: { inlineBytes: 128, overflow: "artifact" } },
      async () => ({ token: "sk-private-value-123456789", rows: "x".repeat(2_000) }),
    )
    const result = await setup.pipeline.execute(request(tool.identity))
    expect(result.output).toEqual(expect.objectContaining({ summary: "Tool output stored as artifact" }))
    expect(result.artifactRefs).toHaveLength(1)
    const artifact = setup.artifacts.get(result.artifactRefs[0] as string)
    expect(artifact?.content).not.toContain("sk-private-value")
    expect(artifact?.content).toContain("[REDACTED]")
  })

  test("idempotent retries return the settled execution and reject changed input", async () => {
    const setup = createSetup("allow")
    let calls = 0
    const tool = setup.registry.register(descriptor(), async () => ({ call: ++calls }))
    const first = await setup.pipeline.execute(request(tool.identity))
    const duplicate = await setup.pipeline.execute(request(tool.identity))
    expect(duplicate.executionId).toBe(first.executionId)
    expect(calls).toBe(1)
    await expect(
      setup.pipeline.execute({ ...request(tool.identity), input: { query: "different" } }),
    ).rejects.toBeInstanceOf(RuntimeConflictError)
  })

  test("reconciles a committed JDP request instead of submitting a second revision", async () => {
    const setup = createSetup("allow")
    const client = new CommittedJdpClient()
    const tool = registerGeneratedJdpTool(setup.registry, client, {
      name: "design.patch.apply",
      description: "Apply generated JDP patch",
      operation: "design.patch.apply",
      inputSchema: {
        type: "object",
        properties: { baseRevision: { type: "string" } },
        required: ["baseRevision"],
        additionalProperties: true,
      },
      permission: "write-design",
      effects: ["design-write"],
      resourceClaims: ["design/board"],
      write: true,
    })
    const result = await setup.pipeline.execute({
      ...request(tool.identity),
      toolName: tool.descriptor.name,
      input: { baseRevision: "revision:1", operations: [] },
    })
    expect(result.status).toBe("succeeded")
    expect(client.invocations).toBe(1)
    expect(client.reconciliations).toBe(1)
    expect(client.executionId).toBe(result.executionId)
  })

  test("propagates Runtime cancellation to the exact daemon task identity", async () => {
    const setup = createSetup("allow")
    const client = new CancellableJdpClient()
    const tool = registerGeneratedJdpTool(setup.registry, client, {
      name: "route.start",
      description: "Start a cancellable route",
      operation: "route.start",
      inputSchema: {
        type: "object",
        properties: { baseRevision: { type: "string" } },
        required: ["baseRevision"],
      },
      permission: "write-design",
      effects: ["design-write", "process"],
      resourceClaims: ["design/board"],
      write: true,
      cancellable: true,
      timeoutMs: 1_000,
    })
    const controller = new AbortController()
    const pending = setup.pipeline.execute({
      ...request(tool.identity),
      toolName: tool.descriptor.name,
      input: { baseRevision: "revision:1" },
      signal: controller.signal,
    })
    await Bun.sleep(0)
    controller.abort()
    const result = await pending
    expect(result.status).toBe("failed")
    expect(client.cancelledTaskId).toBe(`task-${result.executionId}`)
    expect(client.executionId).toBe(result.executionId)
  })

  test("telemetry retains identifiers but excludes prompts, secrets, and tool output", async () => {
    const setup = createSetup("allow")
    const tool = setup.registry.register(descriptor(), async () => ({ ok: true }))
    await setup.pipeline.execute(request(tool.identity))
    const attributes = setup.tracer.spans.find((span) => span.name === "tool.execution")?.attributes
    expect(attributes).toEqual(expect.objectContaining({ sessionId: "session", toolName: "design.query" }))
    expect(JSON.stringify(attributes)).not.toContain("private")
  })

  test("resource claims allow parallel reads but fence conflicting writes", () => {
    const leases = new ResourceLeaseManager()
    leases.acquire("read-a", "run-a", ["read"], ["design/board"])
    leases.acquire("read-b", "run-b", ["read"], ["design/board"])
    expect(() => leases.acquire("write", "run-c", ["design-write"], ["design/board"])).toThrow(
      RuntimeConflictError,
    )
    leases.release("read-a")
    leases.release("read-b")
    expect(() => leases.acquire("write", "run-c", ["design-write"], ["design/board"])).not.toThrow()
  })
})

function createSetup(action: "allow" | "deny") {
  const eventStore = new MemoryEventStore()
  const events = new DurableEventBus(eventStore)
  const registry = new ToolRegistry()
  const executions = new MemoryToolExecutionRepository()
  const artifacts = new MemoryArtifactStore()
  const tracer = new RecordingRuntimeTracer()
  const policy = new PolicyEngine([
    {
      id: `policy-${action}`,
      action,
      projectId: "project",
      reason: action === "allow" ? "test allow" : "test deny",
    },
  ])
  const approvals = new ApprovalService(new MemoryApprovalRepository(), events)
  const pipeline = new ToolSettlementPipeline({
    registry,
    policy,
    approvals,
    executions,
    artifacts,
    events,
    tracer,
  })
  return { events, registry, executions, artifacts, tracer, pipeline }
}

function descriptor() {
  return {
    name: "design.query",
    description: "Query design",
    inputSchema: {
      type: "object",
      properties: { query: { type: "string" } },
      required: ["query"],
      additionalProperties: false,
    },
    source: "builtin" as const,
    permission: "read-design",
    effects: ["read" as const],
    resourceClaims: ["design/project"],
    idempotency: "supported" as const,
    cancellation: "supported" as const,
    timeoutMs: 1_000,
    outputBudget: { inlineBytes: 1_024, overflow: "artifact" as const },
  }
}

function request(identity: string) {
  return {
    toolCallId: "call",
    toolName: "design.query",
    toolIdentity: identity,
    input: { query: "private design summary" },
    idempotencyKey: "run:call",
    sessionId: "session",
    runId: "run",
    projectId: "project",
    workspaceId: "workspace",
    agentId: "main-agent",
    agentRole: "primary" as const,
    runKind: "root" as const,
  }
}

class CommittedJdpClient implements JdpClient {
  invocations = 0
  reconciliations = 0
  executionId?: string
  async capabilities(): Promise<JdpCapabilities> {
    return { protocolVersion: "0.1", operations: ["design.patch.apply"], reconciliation: true }
  }
  async invoke<TInput, TOutput>(request: JdpRequest<TInput>): Promise<JdpSuccess<TOutput> | JdpFailure> {
    this.invocations++
    if (!request.executionId) throw new Error("write request is missing the Runtime execution identity")
    this.executionId = request.executionId
    throw new JdpCommittedWithoutResponseError(request.executionId, request.idempotencyKey)
  }
  async *events(): AsyncIterable<DurableEvent> {}
  async cancel(): Promise<void> {}
  async reconcile(executionId: string, idempotencyKey: string): Promise<ExecutionReconciliation> {
    this.reconciliations++
    return {
      executionId,
      idempotencyKey,
      status: "succeeded",
      result: {
        success: true,
        requestId: "call",
        executionId,
        baseRevision: "revision:1",
        newRevision: "revision:2",
        data: { reconciled: true },
        artifactRefs: [],
        evidenceRefs: [],
        findings: { errors: 0, warnings: 0, info: 0 },
      },
    }
  }
}

class CancellableJdpClient implements JdpClient {
  executionId?: string
  cancelledTaskId?: string
  private settle?: (response: JdpFailure) => void

  async capabilities(): Promise<JdpCapabilities> {
    return { protocolVersion: "0.1", operations: ["route.start", "task.cancel"], reconciliation: true }
  }

  invoke<TInput, TOutput>(request: JdpRequest<TInput>): Promise<JdpSuccess<TOutput> | JdpFailure> {
    if (request.executionId) this.executionId = request.executionId
    return new Promise((resolve) => {
      this.settle = resolve
    })
  }

  async *events(): AsyncIterable<DurableEvent> {}

  async cancel(taskId: string): Promise<void> {
    this.cancelledTaskId = taskId
    this.settle?.({
      success: false,
      requestId: "call",
      ...(this.executionId ? { executionId: this.executionId } : {}),
      error: {
        code: "task_cancelled",
        stage: "task-control",
        summary: "task cancellation reached a confirmed terminal state",
        fingerprint: "task-cancelled-test",
        retryable: false,
        objectRefs: [],
        evidenceRefs: [],
        suggestedActions: [],
      },
    })
  }

  async reconcile(executionId: string, idempotencyKey: string): Promise<ExecutionReconciliation> {
    return { executionId, idempotencyKey, status: "not-found" }
  }
}
