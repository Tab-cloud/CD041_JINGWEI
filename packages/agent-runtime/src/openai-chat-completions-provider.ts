import type { ProviderCapabilities } from "@jingwei/agent-protocol"
import type { MaterializedTool } from "@jingwei/agent-tools"
import OpenAI from "openai"
import type {
  ChatCompletionMessageParam,
  ChatCompletionMessageToolCall,
  ChatCompletionTool,
} from "openai/resources/chat/completions/completions"
import type { ProviderFetch } from "./openai-responses-provider.ts"
import type { ProviderAdapter, ProviderChunk, ProviderMessage, ProviderTurnRequest } from "./provider.ts"

export interface OpenAIChatCompletionsProviderOptions {
  apiKey: string
  model: string
  contextTokens: number
  outputTokens: number
  baseURL?: string
  timeoutMs?: number
  store?: boolean
  reasoning?: boolean
  faultDomain?: string
  dataPolicy?: Record<string, string | boolean>
  fetch?: ProviderFetch
}

interface ToolAlias {
  alias: string
  tool: MaterializedTool
}

interface PendingToolCall {
  id: string
  name: string
  arguments: string
}

/** OpenAI-compatible Chat Completions adapter for endpoints without /responses. */
export class OpenAIChatCompletionsProvider implements ProviderAdapter {
  readonly capabilities: ProviderCapabilities
  private readonly client: OpenAI
  private readonly model: string
  private readonly maxOutputTokens: number

  constructor(options: OpenAIChatCompletionsProviderOptions) {
    if (!options.apiKey.trim()) throw new Error("OpenAI-compatible API key is required")
    if (!options.model.trim()) throw new Error("OpenAI-compatible model is required")
    if (options.contextTokens <= 0 || options.outputTokens <= 0) {
      throw new Error("OpenAI-compatible token limits must be positive")
    }
    this.client = new OpenAI({
      apiKey: options.apiKey,
      ...(options.baseURL ? { baseURL: options.baseURL } : {}),
      maxRetries: 0,
      timeout: options.timeoutMs ?? 120_000,
      ...(options.fetch ? { fetch: options.fetch } : {}),
    })
    this.model = options.model
    this.maxOutputTokens = options.outputTokens
    this.capabilities = {
      provider: "openai-chat-completions",
      model: options.model,
      nativeTools: true,
      structuredOutput: false,
      streaming: true,
      reasoning: options.reasoning ?? true,
      image: false,
      pdf: false,
      contextTokens: options.contextTokens,
      outputTokens: options.outputTokens,
      cancellation: true,
      retry: "safe",
      idempotency: false,
      faultDomain: options.faultDomain ?? faultDomain(options.baseURL),
      dataPolicy: {
        credentialStorage: "external",
        requestStorage: options.store ?? false,
        protocol: "openai-chat-completions",
        ...(options.dataPolicy ?? {}),
      },
    }
  }

  async *stream(request: ProviderTurnRequest): AsyncIterable<ProviderChunk> {
    const aliases = aliasTools(request.tools)
    const stream = await this.client.chat.completions.create(
      {
        model: this.model,
        messages: toChatMessages(request.messages, aliases),
        ...(aliases.length > 0 ? { tools: aliases.map(toChatTool), tool_choice: "auto" as const } : {}),
        max_completion_tokens: Math.min(
          request.maxOutputTokens ?? this.maxOutputTokens,
          this.maxOutputTokens,
        ),
        stream: true,
        stream_options: { include_usage: true },
      },
      { signal: request.signal },
    )

    const pending = new Map<number, PendingToolCall>()
    let terminal = false
    for await (const chunk of stream) {
      if (chunk.usage) {
        yield {
          type: "usage",
          inputTokens: chunk.usage.prompt_tokens,
          outputTokens: chunk.usage.completion_tokens,
        }
      }
      const choice = chunk.choices[0]
      if (!choice) continue
      if (choice.delta.content) yield { type: "text-delta", text: choice.delta.content }
      for (const delta of choice.delta.tool_calls ?? []) {
        if (delta.type && delta.type !== "function") {
          throw new Error(`OpenAI-compatible endpoint returned unsupported tool type: ${delta.type}`)
        }
        const current = pending.get(delta.index) ?? { id: "", name: "", arguments: "" }
        if (delta.id) current.id = delta.id
        if (delta.function?.name) current.name += delta.function.name
        if (delta.function?.arguments) current.arguments += delta.function.arguments
        pending.set(delta.index, current)
      }
      if (choice.finish_reason) {
        for (const call of [...pending.entries()].sort(([left], [right]) => left - right)) {
          yield materializeToolCall(call[1], aliases)
        }
        const reason =
          choice.finish_reason === "tool_calls"
            ? "tool-calls"
            : choice.finish_reason === "length"
              ? "length"
              : "stop"
        terminal = true
        yield { type: "finish", reason }
      }
    }
    if (!terminal) throw new Error("OpenAI-compatible chat stream ended without a terminal chunk")
  }
}

function toChatMessages(messages: ProviderMessage[], aliases: ToolAlias[]): ChatCompletionMessageParam[] {
  return messages.map((message): ChatCompletionMessageParam => {
    if (message.role === "tool") {
      if (!message.toolCallId) throw new Error("Tool result message is missing toolCallId")
      return { role: "tool", tool_call_id: message.toolCallId, content: message.content }
    }
    if (message.role === "assistant") {
      const toolCalls: ChatCompletionMessageToolCall[] = (message.toolCalls ?? []).map((call) => {
        const selected = aliases.find((entry) => entry.tool.descriptor.name === call.name)
        if (!selected) throw new Error(`Tool history references an unavailable tool: ${call.name}`)
        return {
          type: "function",
          id: call.id,
          function: { name: selected.alias, arguments: JSON.stringify(call.input) },
        }
      })
      return {
        role: "assistant",
        content: message.content || null,
        ...(toolCalls.length > 0 ? { tool_calls: toolCalls } : {}),
      }
    }
    return { role: message.role, content: message.content }
  })
}

function aliasTools(tools: MaterializedTool[]): ToolAlias[] {
  return tools.map((tool, index) => ({
    alias: `jw_${index}_${tool.descriptor.name.replace(/[^A-Za-z0-9_-]/g, "_")}`.slice(0, 64),
    tool,
  }))
}

function toChatTool(entry: ToolAlias): ChatCompletionTool {
  return {
    type: "function",
    function: {
      name: entry.alias,
      description: entry.tool.descriptor.description,
      parameters: entry.tool.descriptor.inputSchema as Record<string, unknown>,
      strict: false,
    },
  }
}

function materializeToolCall(call: PendingToolCall, aliases: ToolAlias[]): ProviderChunk {
  const selected = aliases.find((entry) => entry.alias === call.name)
  if (!selected || !call.id) throw new Error("OpenAI-compatible endpoint returned an invalid tool call")
  let input: unknown
  try {
    input = JSON.parse(call.arguments)
  } catch {
    throw new Error(`OpenAI-compatible tool arguments are invalid JSON: ${selected.tool.descriptor.name}`)
  }
  if (!input || typeof input !== "object" || Array.isArray(input)) {
    throw new Error(`OpenAI-compatible tool arguments must be an object: ${selected.tool.descriptor.name}`)
  }
  return {
    type: "tool-call",
    id: call.id,
    name: selected.tool.descriptor.name,
    input: input as Record<string, unknown>,
    toolIdentity: selected.tool.identity,
  }
}

function faultDomain(baseURL?: string): string {
  try {
    return `openai-chat:${new URL(baseURL ?? "https://api.openai.com/v1").host}`
  } catch {
    return "openai-chat:invalid-endpoint"
  }
}
