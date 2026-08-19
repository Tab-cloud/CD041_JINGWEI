import { RecordingRuntimeTracer, type RuntimeTracer } from "@jingwei/agent-observability"
import type { JdpClient, JdpFailure, JdpRequest, JdpSuccess, JsonSchema } from "@jingwei/agent-protocol"
import { UnknownToolSideEffectError } from "./errors.ts"
import type { ToolRegistry } from "./registry.ts"
import type { MaterializedTool } from "./types.ts"

export interface GeneratedJdpToolDefinition {
  name: string
  description: string
  operation: string
  inputSchema: JsonSchema
  outputSchema?: JsonSchema
  permission: string
  effects: Array<"read" | "design-write" | "workspace-write" | "process" | "network" | "hardware" | "release">
  resourceClaims: string[]
  write: boolean
  timeoutMs?: number
}

export class JdpInvocationError extends Error {
  constructor(readonly failure: JdpFailure) {
    super(`${failure.error.code}: ${failure.error.summary}`)
    this.name = "JdpInvocationError"
  }
}

export class JdpCommittedWithoutResponseError extends Error {
  constructor(
    readonly executionId: string,
    readonly idempotencyKey: string,
  ) {
    super("JDP commit may have succeeded but the response was not received")
    this.name = "JdpCommittedWithoutResponseError"
  }
}

export function registerGeneratedJdpTool(
  registry: ToolRegistry,
  client: JdpClient,
  definition: GeneratedJdpToolDefinition,
  tracer: RuntimeTracer = new RecordingRuntimeTracer(),
): MaterializedTool {
  return registry.register(
    {
      name: definition.name,
      description: definition.description,
      inputSchema: definition.inputSchema,
      ...(definition.outputSchema ? { outputSchema: definition.outputSchema } : {}),
      source: "jdp",
      permission: definition.permission,
      effects: definition.effects,
      resourceClaims: definition.resourceClaims,
      idempotency: definition.write ? "required" : "supported",
      cancellation: "supported",
      timeoutMs: definition.timeoutMs ?? 120_000,
      outputBudget: { inlineBytes: 32_768, overflow: "artifact" },
    },
    async (input, context) => {
      const idempotencyKey = context.idempotencyKey ?? `${context.runId}:${context.toolCallId}`
      const request: JdpRequest<Record<string, unknown>> = {
        protocolVersion: "0.1",
        requestId: context.toolCallId,
        idempotencyKey,
        workspaceId: context.workspaceId,
        projectId: context.projectId,
        operation: definition.operation,
        dryRun: input.dryRun === true,
        ...(typeof input.baseRevision === "string" ? { baseRevision: input.baseRevision } : {}),
        input,
        traceContext: context.traceContext,
      }
      if (definition.write && !request.baseRevision) throw new Error("JDP write requires baseRevision")
      const span = tracer.startSpan(
        "jdp.request",
        {
          sessionId: context.sessionId,
          runId: context.runId,
          requestId: request.requestId,
          operation: request.operation,
        },
        context.traceContext,
      )
      try {
        const result = await client.invoke<Record<string, unknown>, unknown>(request)
        if (!result.success) throw new JdpInvocationError(result)
        span.end("ok")
        return result
      } catch (error) {
        if (!(error instanceof JdpCommittedWithoutResponseError)) {
          span.end("error", error instanceof Error ? error.message : String(error))
          throw error
        }
        const reconciliation = await client.reconcile(error.executionId, error.idempotencyKey)
        if (reconciliation.status === "succeeded" && reconciliation.result?.success) {
          span.end("ok")
          return reconciliation.result as JdpSuccess<unknown>
        }
        if (reconciliation.status === "failed" && reconciliation.result && !reconciliation.result.success) {
          span.end("error", reconciliation.result.error.summary)
          throw new JdpInvocationError(reconciliation.result)
        }
        span.end("error", "needs-reconciliation")
        throw new UnknownToolSideEffectError("JDP execution could not be reconciled", {
          executionId: error.executionId,
          idempotencyKey: error.idempotencyKey,
        })
      }
    },
  )
}
