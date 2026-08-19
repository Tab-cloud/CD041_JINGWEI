import type { ProviderCapabilities } from "@jingwei/agent-protocol"
import type { MaterializedTool } from "@jingwei/agent-tools"
import OpenAI from "openai"
import type {
  FunctionTool,
  ResponseCreateParamsStreaming,
  ResponseInput,
  ResponseStreamEvent,
} from "openai/resources/responses/responses"
import type { ProviderAdapter, ProviderChunk, ProviderMessage, ProviderTurnRequest } from "./provider.ts"

export interface OpenAIResponsesProviderOptions {
  apiKey: string
  model: string
  contextTokens: number
  outputTokens: number
  baseURL?: string
  organization?: string
  project?: string
  timeoutMs?: number
  store?: boolean
  reasoning?: boolean
  image?: boolean
  pdf?: boolean
  structuredOutput?: boolean
  faultDomain?: string
  dataPolicy?: Record<string, string | boolean>
}

interface ToolAlias {
  alias: string
  tool: MaterializedTool
}

/**
 * OpenAI Responses API adapter using the official JavaScript SDK.
 *
 * Credentials are constructor-only and never copied into capabilities, events, or
 * provider errors. SDK retries are disabled so the runtime owns failover after a
 * partial stream.
 */
export class OpenAIResponsesProvider implements ProviderAdapter {
  readonly capabilities: ProviderCapabilities
  private readonly client: OpenAI
  private readonly model: string
  private readonly maxOutputTokens: number
  private readonly store: boolean

  constructor(options: OpenAIResponsesProviderOptions) {
    if (!options.apiKey.trim()) throw new Error("OpenAI API key is required")
    if (!options.model.trim()) throw new Error("OpenAI model is required")
    if (options.contextTokens <= 0 || options.outputTokens <= 0) {
      throw new Error("OpenAI context and output token limits must be positive")
    }
    this.client = new OpenAI({
      apiKey: options.apiKey,
      ...(options.baseURL ? { baseURL: options.baseURL } : {}),
      ...(options.organization ? { organization: options.organization } : {}),
      ...(options.project ? { project: options.project } : {}),
      maxRetries: 0,
      timeout: options.timeoutMs ?? 120_000,
    })
    this.model = options.model
    this.maxOutputTokens = options.outputTokens
    this.store = options.store ?? false
    this.capabilities = {
      provider: "openai",
      model: options.model,
      nativeTools: true,
      structuredOutput: options.structuredOutput ?? true,
      streaming: true,
      reasoning: options.reasoning ?? true,
      image: options.image ?? true,
      pdf: options.pdf ?? true,
      contextTokens: options.contextTokens,
      outputTokens: options.outputTokens,
      cancellation: true,
      retry: "safe",
      idempotency: false,
      faultDomain: options.faultDomain ?? faultDomain(options.baseURL),
      dataPolicy: {
        credentialStorage: "external",
        requestStorage: this.store,
        ...(options.dataPolicy ?? {}),
      },
    }
  }

  async *stream(request: ProviderTurnRequest): AsyncIterable<ProviderChunk> {
    const aliases = aliasTools(request.tools)
    const response = await this.client.responses.create(
      {
        model: this.model,
        input: toResponseInput(request.messages, aliases),
        tools: aliases.map(toFunctionTool),
        max_output_tokens: Math.min(request.maxOutputTokens ?? this.maxOutputTokens, this.maxOutputTokens),
        store: this.store,
        stream: true,
      } satisfies ResponseCreateParamsStreaming,
      { signal: request.signal },
    )

    let toolCalls = 0
    let terminal = false
    for await (const event of response) {
      for (const chunk of translateEvent(event, aliases)) {
        if (chunk.type === "tool-call") toolCalls++
        if (chunk.type === "finish") terminal = true
        yield chunk
      }
      if (event.type === "response.completed") {
        terminal = true
        yield { type: "finish", reason: toolCalls > 0 ? "tool-calls" : "stop" }
      }
      if (event.type === "response.incomplete") {
        terminal = true
        yield { type: "finish", reason: "length" }
      }
    }
    if (!terminal) throw new Error("OpenAI response stream ended without a terminal event")
  }
}

function translateEvent(event: ResponseStreamEvent, aliases: ToolAlias[]): ProviderChunk[] {
  if (event.type === "response.output_text.delta") {
    return [{ type: "text-delta", text: event.delta }]
  }
  if (event.type === "response.reasoning_summary_text.delta") {
    return [{ type: "reasoning-delta", text: event.delta }]
  }
  if (
    event.type === "response.output_item.done" &&
    event.item.type === "function_call" &&
    "name" in event.item &&
    "call_id" in event.item &&
    "arguments" in event.item
  ) {
    const item = event.item
    const selected = aliases.find((entry) => entry.alias === item.name)
    if (!selected) throw new Error(`OpenAI returned an unadvertised tool: ${item.name}`)
    return [
      {
        type: "tool-call",
        id: item.call_id,
        name: selected.tool.descriptor.name,
        input: parseToolArguments(item.arguments, selected.tool.descriptor.name),
        toolIdentity: selected.tool.identity,
      },
    ]
  }
  if (event.type === "response.completed" || event.type === "response.incomplete") {
    return [
      {
        type: "usage",
        inputTokens: event.response.usage?.input_tokens ?? 0,
        outputTokens: event.response.usage?.output_tokens ?? 0,
      },
    ]
  }
  if (event.type === "response.failed") {
    throw new Error(event.response.error?.message ?? "OpenAI response failed")
  }
  if (event.type === "error") throw new Error(event.message)
  return []
}

function toResponseInput(messages: ProviderMessage[], aliases: ToolAlias[]): ResponseInput {
  const input: ResponseInput = []
  for (const message of messages) {
    if (message.role === "tool") {
      if (!message.toolCallId) throw new Error("Tool result message is missing toolCallId")
      input.push({ type: "function_call_output", call_id: message.toolCallId, output: message.content })
      continue
    }
    input.push({
      role: message.role,
      content: message.content,
      type: "message",
    })
    for (const call of message.toolCalls ?? []) {
      const selected = aliases.find((entry) => entry.tool.descriptor.name === call.name)
      if (!selected) throw new Error(`Tool history references an unavailable tool: ${call.name}`)
      input.push({
        type: "function_call",
        call_id: call.id,
        name: selected.alias,
        arguments: JSON.stringify(call.input),
      })
    }
  }
  return input
}

function aliasTools(tools: MaterializedTool[]): ToolAlias[] {
  return tools.map((tool, index) => ({
    alias: `jw_${index}_${tool.descriptor.name.replace(/[^A-Za-z0-9_-]/g, "_")}`.slice(0, 64),
    tool,
  }))
}

function toFunctionTool(entry: ToolAlias): FunctionTool {
  return {
    type: "function",
    name: entry.alias,
    description: entry.tool.descriptor.description,
    parameters: entry.tool.descriptor.inputSchema as Record<string, unknown>,
    strict: false,
    ...(entry.tool.descriptor.outputSchema
      ? { output_schema: entry.tool.descriptor.outputSchema as Record<string, unknown> }
      : {}),
  }
}

function parseToolArguments(value: string, toolName: string): Record<string, unknown> {
  let parsed: unknown
  try {
    parsed = JSON.parse(value)
  } catch {
    throw new Error(`OpenAI returned invalid JSON arguments for ${toolName}`)
  }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error(`OpenAI returned non-object arguments for ${toolName}`)
  }
  return parsed as Record<string, unknown>
}

function faultDomain(baseURL?: string): string {
  if (!baseURL) return "openai:api.openai.com"
  try {
    return `openai:${new URL(baseURL).host}`
  } catch {
    throw new Error("OpenAI baseURL must be an absolute URL")
  }
}
