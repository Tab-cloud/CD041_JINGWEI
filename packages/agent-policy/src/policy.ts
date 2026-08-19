import type { Effect, PolicyDecision } from "@jingwei/agent-protocol"

export interface PolicyRule {
  id: string
  action: PolicyDecision
  projectId?: string
  sessionId?: string
  agentId?: string
  toolName?: string
  permission?: string
  effects?: Effect[]
  resources?: string[]
  reason: string
}

export interface AuthorizationRequest {
  projectId: string
  sessionId: string
  agentId: string
  agentRole: "primary" | "specialist" | "workflow"
  runKind: "root" | "specialist" | "workflow-node"
  toolName: string
  permission: string
  effects: Effect[]
  resourceClaims: string[]
  rootOnly?: boolean
}

export interface AuthorizationResult {
  action: PolicyDecision
  reason: string
  matchedRuleIds: string[]
}

export class PolicyEngine {
  constructor(
    private readonly rules: readonly PolicyRule[],
    private readonly defaultAction: PolicyDecision = "deny",
  ) {}

  evaluate(request: AuthorizationRequest): AuthorizationResult {
    if (request.runKind === "root" && request.agentRole !== "primary") {
      return deny("Root runs must be owned by the primary agent", ["authority:root-owner"])
    }
    if (request.rootOnly && request.runKind !== "root") {
      return deny("Root-only orchestration tools are unavailable to specialist runs", ["authority:root-only"])
    }

    const matched = this.rules.filter((rule) => matchesRule(rule, request))
    const denied = matched.filter((rule) => rule.action === "deny")
    if (denied.length > 0)
      return deny(
        joinReasons(denied),
        denied.map((rule) => rule.id),
      )

    const asked = matched.filter((rule) => rule.action === "ask")
    if (asked.length > 0) {
      return { action: "ask", reason: joinReasons(asked), matchedRuleIds: asked.map((rule) => rule.id) }
    }

    const allowed = matched.filter((rule) => rule.action === "allow")
    if (allowed.length > 0) {
      return { action: "allow", reason: joinReasons(allowed), matchedRuleIds: allowed.map((rule) => rule.id) }
    }

    return {
      action: this.defaultAction,
      reason: `No policy rule authorizes ${request.toolName}`,
      matchedRuleIds: [],
    }
  }
}

function matchesRule(rule: PolicyRule, request: AuthorizationRequest): boolean {
  if (rule.projectId && rule.projectId !== request.projectId) return false
  if (rule.sessionId && rule.sessionId !== request.sessionId) return false
  if (rule.agentId && rule.agentId !== request.agentId) return false
  if (rule.toolName && rule.toolName !== request.toolName) return false
  if (rule.permission && rule.permission !== request.permission) return false
  if (rule.effects && !request.effects.some((effect) => rule.effects?.includes(effect))) return false
  if (
    rule.resources &&
    !request.resourceClaims.some((claim) =>
      rule.resources?.some((ruleClaim) => resourceMatches(ruleClaim, claim)),
    )
  ) {
    return false
  }
  return true
}

export function resourceMatches(ruleResource: string, requestedResource: string): boolean {
  if (ruleResource === "*") return true
  if (ruleResource.endsWith("/**")) {
    const prefix = ruleResource.slice(0, -3)
    return requestedResource === prefix || requestedResource.startsWith(`${prefix}/`)
  }
  return ruleResource === requestedResource
}

function joinReasons(rules: PolicyRule[]): string {
  return [...new Set(rules.map((rule) => rule.reason))].join("; ")
}

function deny(reason: string, matchedRuleIds: string[]): AuthorizationResult {
  return { action: "deny", reason, matchedRuleIds }
}
