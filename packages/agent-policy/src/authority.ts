export const ROOT_ONLY_TOOLS = new Set(["specialist.run", "specialist.parallel", "workflow.start"])

export interface AgentAuthority {
  agentId: string
  role: "primary" | "specialist" | "workflow"
  tools: string[]
  contextRefs: string[]
  budget: { turns: number; toolCalls: number; outputTokens: number }
}

export function resolveAgentTools(
  authority: AgentAuthority,
  runKind: "root" | "specialist" | "workflow-node",
): string[] {
  if (runKind === "root" && authority.role !== "primary") {
    throw new Error(`Root run cannot be owned by ${authority.agentId}`)
  }
  if (runKind !== "root") return authority.tools.filter((tool) => !ROOT_ONLY_TOOLS.has(tool))
  return [...new Set(authority.tools)]
}

export function assertSpecialistBudget(authority: AgentAuthority): void {
  if (authority.role !== "specialist") return
  if (authority.budget.turns < 1 || authority.budget.toolCalls < 0 || authority.budget.outputTokens < 1) {
    throw new Error(`Specialist ${authority.agentId} has an invalid explicit budget`)
  }
}
