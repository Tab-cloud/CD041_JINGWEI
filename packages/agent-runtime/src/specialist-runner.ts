import type { DurableEventBus } from "@jingwei/agent-events"
import type { AgentAuthority } from "@jingwei/agent-policy"
import { assertSpecialistBudget, resolveAgentTools } from "@jingwei/agent-policy"
import type { MaterializedTool, ToolRegistry } from "@jingwei/agent-tools"
import { type AgentLoop, runtimeFailure } from "./agent-loop.ts"
import type { RuntimeRepository } from "./runtime-repository.ts"

export interface SpecialistDefinition extends AgentAuthority {
  role: "specialist"
  description: string
  systemPrompt: string
}

export class SpecialistRunner {
  constructor(
    private readonly loop: AgentLoop,
    private readonly repository: RuntimeRepository,
    private readonly events: DurableEventBus,
  ) {}

  register(registry: ToolRegistry, definition: SpecialistDefinition): MaterializedTool {
    assertSpecialistBudget(definition)
    const visibleTools = resolveAgentTools(definition, "specialist")
    return registry.register(
      {
        name: `specialist.${definition.agentId}`,
        description: definition.description,
        inputSchema: {
          type: "object",
          properties: {
            task: { type: "string", minLength: 1, maxLength: 20_000 },
            contextRefs: { type: "array", items: { type: "string" } },
          },
          required: ["task"],
          additionalProperties: false,
        },
        source: "builtin",
        permission: `specialist:${definition.agentId}`,
        effects: ["read"],
        resourceClaims: [],
        idempotency: "supported",
        cancellation: "supported",
        timeoutMs: 10 * 60_000,
        outputBudget: { inlineBytes: 32_768, overflow: "artifact" },
        rootOnly: true,
      },
      async (input, context) => {
        if (context.agentId !== "main-agent") throw new Error("Only main-agent may invoke specialist tools")
        const contextRefs = Array.isArray(input.contextRefs)
          ? input.contextRefs.filter((value): value is string => typeof value === "string")
          : []
        const allowedContext = contextRefs.filter((reference) => definition.contextRefs.includes(reference))
        const run = this.repository.createRun(
          context.sessionId,
          "specialist",
          definition.agentId,
          context.runId,
        )
        this.repository.updateRun(run.id, { status: "running" })
        this.events.append({
          aggregateId: context.sessionId,
          sessionId: context.sessionId,
          runId: run.id,
          type: "specialist.run.started",
          data: {
            runId: run.id,
            parentRunId: context.runId,
            agentId: definition.agentId,
            visibleTools,
            contextRefs: allowedContext,
            budget: definition.budget,
          },
        })
        try {
          const result = await this.loop.run({
            sessionId: context.sessionId,
            runId: run.id,
            projectId: context.projectId,
            workspaceId: context.workspaceId,
            agentId: definition.agentId,
            agentRole: "specialist",
            runKind: "specialist",
            initialContent: String(input.task),
            systemPrompt: `${definition.systemPrompt}\nAuthorized context references: ${allowedContext.join(", ") || "none"}`,
            visibleToolNames: visibleTools,
            signal: context.signal,
            maxTurns: definition.budget.turns,
          })
          this.repository.updateRun(run.id, { status: "completed", output: result.output })
          this.events.append({
            aggregateId: context.sessionId,
            sessionId: context.sessionId,
            runId: run.id,
            type: "specialist.run.completed",
            data: { runId: run.id, parentRunId: context.runId, agentId: definition.agentId },
          })
          return { runId: run.id, agentId: definition.agentId, result: result.output }
        } catch (error) {
          const failure = runtimeFailure(error, "specialist-run")
          this.repository.updateRun(run.id, { status: "failed", error: failure })
          this.events.append({
            aggregateId: context.sessionId,
            sessionId: context.sessionId,
            runId: run.id,
            type: "specialist.run.failed",
            data: { runId: run.id, parentRunId: context.runId, agentId: definition.agentId, error: failure },
          })
          throw error
        }
      },
    )
  }
}
