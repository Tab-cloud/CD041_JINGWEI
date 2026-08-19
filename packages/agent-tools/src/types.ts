import type { Effect, JsonSchema, TraceContext } from "@jingwei/agent-protocol"

export interface RuntimeToolDescriptor {
  name: string
  description: string
  inputSchema: JsonSchema
  outputSchema?: JsonSchema
  source: "builtin" | "jdp" | "mcp" | "plugin"
  permission: string
  effects: Effect[]
  resourceClaims: string[] | ((input: Record<string, unknown>) => string[])
  idempotency: "required" | "supported" | "unknown"
  cancellation: "supported" | "cooperative" | "unsupported"
  timeoutMs: number
  outputBudget: { inlineBytes: number; overflow: "artifact" | "reject" }
  rootOnly?: boolean
}

export interface ToolExecutionContext {
  executionId: string
  sessionId: string
  runId: string
  toolCallId: string
  idempotencyKey?: string
  projectId: string
  workspaceId: string
  agentId: string
  signal: AbortSignal
  traceContext: TraceContext
}

export type RuntimeToolHandler = (
  input: Record<string, unknown>,
  context: ToolExecutionContext,
) => Promise<unknown>

export interface MaterializedTool {
  identity: string
  generation: number
  descriptor: RuntimeToolDescriptor
  handler: RuntimeToolHandler
}

export interface ToolCallRequest {
  toolCallId: string
  toolName: string
  toolIdentity: string
  input: Record<string, unknown>
  idempotencyKey?: string
  sessionId: string
  runId: string
  projectId: string
  workspaceId: string
  agentId: string
  agentRole: "primary" | "specialist" | "workflow"
  runKind: "root" | "specialist" | "workflow-node"
  traceContext?: TraceContext
  signal?: AbortSignal
}

export interface ToolCallResult {
  executionId: string
  status: "succeeded" | "failed" | "denied" | "unknown"
  output?: unknown
  artifactRefs: string[]
  error?: { code: string; summary: string; retryable: boolean }
}
