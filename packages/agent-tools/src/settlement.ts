import { createHash } from "node:crypto"
import type { DurableEventBus } from "@jingwei/agent-events"
import {
  RecordingRuntimeTracer,
  type RuntimeTracer,
  redactRecord,
  scrubText,
} from "@jingwei/agent-observability"
import type { ApprovalService, PolicyEngine } from "@jingwei/agent-policy"
import { RuntimeConflictError } from "@jingwei/agent-protocol"
import Ajv from "ajv"
import type { ArtifactStore } from "./artifact-store.ts"
import { ToolOutputTooLargeError, UnknownToolSideEffectError } from "./errors.ts"
import type { ToolExecutionRepository, ToolExecutionView } from "./execution-store.ts"
import { canonicalJson, type ToolRegistry } from "./registry.ts"
import { ResourceLeaseManager } from "./resource-leases.ts"
import type { ToolCallRequest, ToolCallResult } from "./types.ts"

export interface ToolSettlementDependencies {
  registry: ToolRegistry
  policy: PolicyEngine
  approvals: ApprovalService
  executions: ToolExecutionRepository
  artifacts: ArtifactStore
  events: DurableEventBus
  tracer?: RuntimeTracer
  resourceLeases?: ResourceLeaseManager
}

export class ToolSettlementPipeline {
  private readonly ajv = new Ajv({ allErrors: true, strict: false })
  private readonly tracer: RuntimeTracer
  private readonly resourceLeases: ResourceLeaseManager

  constructor(private readonly dependencies: ToolSettlementDependencies) {
    this.tracer = dependencies.tracer ?? new RecordingRuntimeTracer()
    this.resourceLeases = dependencies.resourceLeases ?? new ResourceLeaseManager()
  }

  async execute(request: ToolCallRequest): Promise<ToolCallResult> {
    const tool = this.dependencies.registry.get(request.toolName)
    if (!tool) return this.failedWithoutExecution("tool-not-found", `Tool not found: ${request.toolName}`)

    const inputHash = createHash("sha256").update(canonicalJson(request.input)).digest("hex")
    const duplicate = request.idempotencyKey
      ? this.dependencies.executions.findIdempotent(
          request.sessionId,
          request.toolName,
          request.idempotencyKey,
        )
      : undefined
    if (duplicate) {
      if (duplicate.inputHash !== inputHash) {
        throw new RuntimeConflictError(
          "Idempotency key was reused with different input",
          "request-content-mismatch",
        )
      }
      return resultFromExecution(duplicate)
    }

    if (tool.descriptor.idempotency === "required" && !request.idempotencyKey) {
      return this.failedWithoutExecution("idempotency-required", "This tool requires an idempotency key")
    }

    const resources = resolveResources(tool.descriptor.resourceClaims, request.input)
    const now = new Date().toISOString()
    let execution = this.dependencies.executions.create({
      id: crypto.randomUUID(),
      sessionId: request.sessionId,
      runId: request.runId,
      toolCallId: request.toolCallId,
      toolName: request.toolName,
      toolIdentity: request.toolIdentity,
      input: structuredClone(request.input),
      inputHash,
      ...(request.idempotencyKey ? { idempotencyKey: request.idempotencyKey } : {}),
      effects: [...tool.descriptor.effects],
      resourceClaims: resources,
      status: "recorded",
      artifactRefs: [],
      createdAt: now,
      updatedAt: now,
    })
    this.emit(execution, "tool.call.recorded")

    try {
      validateSchema(this.ajv, tool.descriptor.inputSchema, request.input, "input")
      if (tool.identity !== request.toolIdentity) {
        throw new RuntimeConflictError("Advertised tool identity is stale", "stale-tool")
      }

      execution = this.dependencies.executions.update(execution.id, { status: "authorizing" })
      const authorization = this.dependencies.policy.evaluate({
        projectId: request.projectId,
        sessionId: request.sessionId,
        agentId: request.agentId,
        agentRole: request.agentRole,
        runKind: request.runKind,
        toolName: request.toolName,
        permission: tool.descriptor.permission,
        effects: tool.descriptor.effects,
        resourceClaims: resources,
        ...(tool.descriptor.rootOnly === undefined ? {} : { rootOnly: tool.descriptor.rootOnly }),
      })
      execution = this.dependencies.executions.update(execution.id, { authorization: { ...authorization } })
      if (authorization.action === "deny") return this.deny(execution, authorization.reason)

      if (
        authorization.action === "ask" &&
        !this.dependencies.approvals.hasGrant(request.sessionId, tool.descriptor.permission, resources)
      ) {
        execution = this.dependencies.executions.update(execution.id, { status: "waiting-approval" })
        const approval = this.dependencies.approvals.request({
          sessionId: request.sessionId,
          runId: request.runId,
          toolCallId: request.toolCallId,
          toolName: request.toolName,
          permission: tool.descriptor.permission,
          effects: tool.descriptor.effects,
          resourceClaims: resources,
          input: request.input,
          reason: authorization.reason,
        })
        const resolved = await this.dependencies.approvals.wait(approval.id, request.signal)
        if (resolved.status !== "approved") return this.deny(execution, `Approval ${resolved.status}`)
      }

      this.resourceLeases.acquire(execution.id, request.runId, tool.descriptor.effects, resources)
      execution = this.dependencies.executions.update(execution.id, { status: "running" })
      this.emit(execution, "tool.execution.started")
      const span = this.tracer.startSpan(
        "tool.execution",
        {
          sessionId: request.sessionId,
          runId: request.runId,
          toolName: request.toolName,
          executionId: execution.id,
          effects: tool.descriptor.effects.join(","),
        },
        request.traceContext,
      )
      const controller = linkedAbortController(request.signal, tool.descriptor.timeoutMs)
      try {
        const output = await tool.handler(request.input, {
          executionId: execution.id,
          sessionId: request.sessionId,
          runId: request.runId,
          toolCallId: request.toolCallId,
          ...(request.idempotencyKey ? { idempotencyKey: request.idempotencyKey } : {}),
          projectId: request.projectId,
          workspaceId: request.workspaceId,
          agentId: request.agentId,
          signal: controller.signal,
          traceContext: span.context,
        })
        if (tool.descriptor.outputSchema)
          validateSchema(this.ajv, tool.descriptor.outputSchema, output, "output")
        const bounded = this.boundOutput(execution, output, tool.descriptor.outputBudget)
        execution = this.dependencies.executions.update(execution.id, {
          status: "succeeded",
          output: bounded.output,
          artifactRefs: bounded.artifactRefs,
        })
        this.emit(execution, "tool.execution.completed")
        span.end("ok")
        this.resourceLeases.release(execution.id)
        return resultFromExecution(execution)
      } catch (error) {
        span.end("error", error instanceof Error ? error.message : String(error))
        throw error
      } finally {
        controller.abort()
      }
    } catch (error) {
      if (error instanceof UnknownToolSideEffectError) {
        execution = this.dependencies.executions.update(execution.id, {
          status: "unknown",
          error: { code: "unknown-side-effect", summary: scrubText(error.message), retryable: false },
        })
        this.emit(execution, "tool.execution.unknown")
        return resultFromExecution(execution)
      }
      this.resourceLeases.release(execution.id)
      const code = error instanceof RuntimeConflictError ? error.conflict : "tool-execution-failed"
      execution = this.dependencies.executions.update(execution.id, {
        status: "failed",
        error: {
          code,
          summary: scrubText(error instanceof Error ? error.message : String(error)).slice(0, 1_000),
          retryable: false,
        },
      })
      this.emit(execution, "tool.execution.failed")
      return resultFromExecution(execution)
    }
  }

  private deny(execution: ToolExecutionView, reason: string): ToolCallResult {
    const denied = this.dependencies.executions.update(execution.id, {
      status: "denied",
      error: { code: "permission-denied", summary: reason.slice(0, 1_000), retryable: false },
    })
    this.emit(denied, "tool.execution.denied")
    return resultFromExecution(denied)
  }

  private boundOutput(
    execution: ToolExecutionView,
    output: unknown,
    budget: { inlineBytes: number; overflow: "artifact" | "reject" },
  ): { output: unknown; artifactRefs: string[] } {
    const serialized = JSON.stringify(redactOutput(output))
    const byteLength = Buffer.byteLength(serialized)
    if (byteLength <= budget.inlineBytes) return { output: JSON.parse(serialized), artifactRefs: [] }
    if (budget.overflow === "reject") throw new ToolOutputTooLargeError(byteLength)
    const artifact = this.dependencies.artifacts.create(execution.sessionId, "application/json", serialized)
    this.dependencies.events.append({
      aggregateId: execution.sessionId,
      sessionId: execution.sessionId,
      runId: execution.runId,
      type: "artifact.created",
      data: {
        id: artifact.id,
        mediaType: artifact.mediaType,
        byteLength: artifact.byteLength,
        sha256: artifact.sha256,
      },
    })
    return {
      output: { summary: "Tool output stored as artifact", artifactId: artifact.id, byteLength },
      artifactRefs: [artifact.id],
    }
  }

  private emit(execution: ToolExecutionView, type: string): void {
    this.dependencies.events.append({
      aggregateId: execution.sessionId,
      sessionId: execution.sessionId,
      runId: execution.runId,
      type,
      data: {
        executionId: execution.id,
        toolCallId: execution.toolCallId,
        toolName: execution.toolName,
        status: execution.status,
        effects: execution.effects,
        resourceClaims: execution.resourceClaims,
        artifactRefs: execution.artifactRefs,
        error: execution.error,
      },
    })
  }

  private failedWithoutExecution(code: string, summary: string): ToolCallResult {
    return { executionId: "", status: "failed", artifactRefs: [], error: { code, summary, retryable: false } }
  }
}

function validateSchema(ajv: Ajv, schema: object, value: unknown, label: string): void {
  const validate = ajv.compile(schema)
  if (!validate(value)) throw new Error(`Tool ${label} schema violation: ${ajv.errorsText(validate.errors)}`)
}

function resolveResources(
  claims: string[] | ((input: Record<string, unknown>) => string[]),
  input: Record<string, unknown>,
): string[] {
  return [...new Set(typeof claims === "function" ? claims(input) : claims)]
}

function linkedAbortController(parent: AbortSignal | undefined, timeoutMs: number): AbortController {
  const controller = new AbortController()
  const timer = setTimeout(
    () => controller.abort(new DOMException("Tool timed out", "TimeoutError")),
    timeoutMs,
  )
  const abort = () => controller.abort(parent?.reason)
  parent?.addEventListener("abort", abort, { once: true })
  controller.signal.addEventListener(
    "abort",
    () => {
      clearTimeout(timer)
      parent?.removeEventListener("abort", abort)
    },
    { once: true },
  )
  if (parent?.aborted) abort()
  return controller
}

function redactOutput(output: unknown): unknown {
  if (output && typeof output === "object" && !Array.isArray(output)) {
    return redactRecord(output as Record<string, unknown>)
  }
  if (typeof output === "string") return scrubText(output)
  return output
}

function resultFromExecution(execution: ToolExecutionView): ToolCallResult {
  const status = ["succeeded", "failed", "denied", "unknown"].includes(execution.status)
    ? (execution.status as ToolCallResult["status"])
    : "unknown"
  return {
    executionId: execution.id,
    status,
    ...(execution.output !== undefined ? { output: execution.output } : {}),
    artifactRefs: [...execution.artifactRefs],
    ...(execution.error ? { error: execution.error } : {}),
  }
}
