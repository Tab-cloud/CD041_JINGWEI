import type { ProviderCapabilities } from "@jingwei/agent-protocol"
import type { MaterializedTool } from "@jingwei/agent-tools"

export interface ProviderMessage {
  role: "system" | "user" | "assistant" | "tool"
  content: string
  toolCallId?: string
  name?: string
  toolCalls?: Array<{ id: string; name: string; input: Record<string, unknown> }>
}

export interface ProviderTurnRequest {
  messages: ProviderMessage[]
  tools: MaterializedTool[]
  signal: AbortSignal
  maxOutputTokens?: number
}

export type ProviderChunk =
  | { type: "text-delta"; text: string }
  | { type: "reasoning-delta"; text: string }
  | { type: "tool-call"; id: string; name: string; input: Record<string, unknown>; toolIdentity: string }
  | { type: "usage"; inputTokens: number; outputTokens: number }
  | { type: "finish"; reason: "stop" | "tool-calls" | "length" }

export interface ProviderAdapter {
  readonly capabilities: ProviderCapabilities
  stream(request: ProviderTurnRequest): AsyncIterable<ProviderChunk>
}

export interface CompletedProviderTurn {
  text: string
  reasoning: string
  toolCalls: Array<{ id: string; name: string; input: Record<string, unknown>; toolIdentity: string }>
  usage?: { inputTokens: number; outputTokens: number }
  finishReason: "stop" | "tool-calls" | "length"
}

export async function collectProviderTurn(
  provider: ProviderAdapter,
  request: ProviderTurnRequest,
): Promise<CompletedProviderTurn> {
  let text = ""
  let reasoning = ""
  let finishReason: CompletedProviderTurn["finishReason"] | undefined
  let usage: CompletedProviderTurn["usage"]
  const toolCalls: CompletedProviderTurn["toolCalls"] = []
  for await (const chunk of provider.stream(request)) {
    if (request.signal.aborted)
      throw request.signal.reason ?? new DOMException("Model cancelled", "AbortError")
    if (chunk.type === "text-delta") text += chunk.text
    if (chunk.type === "reasoning-delta") reasoning += chunk.text
    if (chunk.type === "tool-call") {
      toolCalls.push({
        id: chunk.id,
        name: chunk.name,
        input: chunk.input,
        toolIdentity: chunk.toolIdentity,
      })
    }
    if (chunk.type === "usage") usage = { inputTokens: chunk.inputTokens, outputTokens: chunk.outputTokens }
    if (chunk.type === "finish") finishReason = chunk.reason
  }
  if (!finishReason) throw new Error("Provider stream ended without a finish event")
  return {
    text,
    reasoning,
    toolCalls,
    ...(usage ? { usage } : {}),
    finishReason,
  }
}
