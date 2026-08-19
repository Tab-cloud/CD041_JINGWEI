import type { DurableEventBus } from "@jingwei/agent-events"
import { RecordingRuntimeTracer, type RuntimeTracer, scrubText } from "@jingwei/agent-observability"
import type { RuntimeFailure } from "@jingwei/agent-protocol"
import type { ToolRegistry, ToolSettlementPipeline } from "@jingwei/agent-tools"
import { ContextProjector } from "./context-projector.ts"
import { type CompletedProviderTurn, collectProviderTurn, type ProviderMessage } from "./provider.ts"
import type { ProviderRegistry } from "./provider-registry.ts"
import type { RuntimeRepository } from "./runtime-repository.ts"

export interface AgentLoopInput {
  sessionId: string
  runId: string
  projectId: string
  workspaceId: string
  agentId: string
  agentRole: "primary" | "specialist" | "workflow"
  runKind: "root" | "specialist" | "workflow-node"
  initialContent: string
  systemPrompt?: string
  visibleToolNames?: string[]
  loadSteer?: () => Promise<string[]>
  signal: AbortSignal
  maxTurns?: number
}

export interface AgentLoopResult {
  output: string
  messages: ProviderMessage[]
  turns: number
}

export interface AgentLoopDependencies {
  providers: ProviderRegistry
  tools: ToolRegistry
  settlement: ToolSettlementPipeline
  repository: RuntimeRepository
  events: DurableEventBus
  projector?: ContextProjector
  tracer?: RuntimeTracer
}

export class AgentLoop {
  private readonly projector: ContextProjector
  private readonly tracer: RuntimeTracer

  constructor(private readonly dependencies: AgentLoopDependencies) {
    this.projector = dependencies.projector ?? new ContextProjector()
    this.tracer = dependencies.tracer ?? new RecordingRuntimeTracer()
  }

  async run(input: AgentLoopInput): Promise<AgentLoopResult> {
    if (input.runKind === "root" && (input.agentRole !== "primary" || input.agentId !== "main-agent")) {
      throw new Error("Root run must be owned by the primary main-agent")
    }
    const visibleTools = this.dependencies.tools
      .list()
      .filter((tool) => !input.visibleToolNames || input.visibleToolNames.includes(tool.descriptor.name))
    const messages: ProviderMessage[] = [
      ...(input.systemPrompt ? [{ role: "system" as const, content: input.systemPrompt }] : []),
      { role: "user", content: input.initialContent },
    ]
    const runSpan = this.tracer.startSpan("agent.run", {
      sessionId: input.sessionId,
      runId: input.runId,
      projectId: input.projectId,
      workspaceId: input.workspaceId,
    })
    const maxTurns = input.maxTurns ?? 32

    try {
      for (let turn = 1; turn <= maxTurns; turn++) {
        if (input.signal.aborted) throw input.signal.reason ?? new DOMException("Run cancelled", "AbortError")
        const projection = this.projector.project(messages)
        const completed = await this.providerTurn(input, projection.messages, visibleTools)
        messages.push({
          role: "assistant",
          content: completed.text,
          ...(completed.toolCalls.length > 0
            ? {
                toolCalls: completed.toolCalls.map((call) => ({
                  id: call.id,
                  name: call.name,
                  input: call.input,
                })),
              }
            : {}),
        })

        if (completed.toolCalls.length > 0) {
          for (const call of completed.toolCalls) {
            const result = await this.dependencies.settlement.execute({
              toolCallId: call.id,
              toolName: call.name,
              toolIdentity: call.toolIdentity,
              input: call.input,
              idempotencyKey: `${input.runId}:${call.id}`,
              sessionId: input.sessionId,
              runId: input.runId,
              projectId: input.projectId,
              workspaceId: input.workspaceId,
              agentId: input.agentId,
              agentRole: input.agentRole,
              runKind: input.runKind,
              traceContext: runSpan.context,
              signal: input.signal,
            })
            messages.push({
              role: "tool",
              name: call.name,
              toolCallId: call.id,
              content: JSON.stringify(result),
            })
          }
          await appendSteer(messages, input.loadSteer)
          continue
        }

        const steered = await appendSteer(messages, input.loadSteer)
        if (steered) continue
        runSpan.end("ok")
        return { output: completed.text, messages, turns: turn }
      }
      throw new Error(`Agent loop exceeded ${maxTurns} turns`)
    } catch (error) {
      runSpan.end("error", error instanceof Error ? error.message : String(error))
      throw error
    }
  }

  private async providerTurn(
    input: AgentLoopInput,
    messages: ProviderMessage[],
    tools: ReturnType<ToolRegistry["list"]>,
  ): Promise<CompletedProviderTurn> {
    const candidates = this.dependencies.providers.candidates({ nativeTools: tools.length > 0 })
    if (candidates.length === 0) {
      throw new Error(
        tools.length > 0 ? "No provider supports required native tools" : "No provider is configured",
      )
    }
    const failedDomains = new Set<string>()
    let lastError: unknown
    for (const provider of candidates) {
      const capabilities = provider.capabilities
      if (failedDomains.has(capabilities.faultDomain)) continue
      const attempt = this.dependencies.repository.createModelAttempt({
        sessionId: input.sessionId,
        runId: input.runId,
        provider: capabilities.provider,
        model: capabilities.model,
        faultDomain: capabilities.faultDomain,
        status: "created",
      })
      this.emitAttempt(input, attempt.id, "model.attempt.started", capabilities)
      this.dependencies.repository.updateModelAttempt(attempt.id, { status: "streaming" })
      const span = this.tracer.startSpan("model.attempt", {
        sessionId: input.sessionId,
        runId: input.runId,
        provider: capabilities.provider,
        model: capabilities.model,
        faultDomain: capabilities.faultDomain,
      })
      try {
        const completed = await collectProviderTurn(provider, { messages, tools, signal: input.signal })
        this.dependencies.repository.updateModelAttempt(attempt.id, {
          status: "completed",
          ...(completed.usage
            ? { inputTokens: completed.usage.inputTokens, outputTokens: completed.usage.outputTokens }
            : {}),
        })
        this.emitAttempt(input, attempt.id, "model.attempt.completed", capabilities)
        span.end("ok")
        return completed
      } catch (error) {
        lastError = error
        failedDomains.add(capabilities.faultDomain)
        const summary = scrubText(error instanceof Error ? error.message : String(error)).slice(0, 1_000)
        this.dependencies.repository.updateModelAttempt(attempt.id, { status: "discarded", error: summary })
        this.emitAttempt(input, attempt.id, "model.attempt.failed", capabilities, summary)
        span.end("error", summary)
      }
    }
    throw lastError instanceof Error ? lastError : new Error("All provider fault domains failed")
  }

  private emitAttempt(
    input: AgentLoopInput,
    attemptId: string,
    type: string,
    capabilities: { provider: string; model: string; faultDomain: string },
    error?: string,
  ): void {
    this.dependencies.events.append({
      aggregateId: input.sessionId,
      sessionId: input.sessionId,
      runId: input.runId,
      type,
      data: {
        attemptId,
        provider: capabilities.provider,
        model: capabilities.model,
        faultDomain: capabilities.faultDomain,
        ...(error ? { error } : {}),
      },
    })
  }
}

async function appendSteer(
  messages: ProviderMessage[],
  loadSteer?: () => Promise<string[]>,
): Promise<boolean> {
  if (!loadSteer) return false
  const inputs = await loadSteer()
  for (const content of inputs) messages.push({ role: "user", content })
  return inputs.length > 0
}

export function runtimeFailure(error: unknown, stage: string): RuntimeFailure {
  const summary = scrubText(error instanceof Error ? error.message : String(error)).slice(0, 1_000)
  return {
    code: error instanceof DOMException && error.name === "AbortError" ? "cancelled" : "runtime-failed",
    stage,
    summary,
    fingerprint: Bun.hash(`${stage}:${summary}`).toString(16),
    retryable: false,
    objectRefs: [],
    evidenceRefs: [],
    suggestedActions: [
      { kind: "repair", description: "Inspect durable run, attempt, and tool events before retrying" },
    ],
  }
}
