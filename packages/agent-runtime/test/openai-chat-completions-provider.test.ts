import { afterEach, describe, expect, test } from "bun:test"
import { ToolRegistry } from "@jingwei/agent-tools"
import { OpenAIChatCompletionsProvider } from "../src/openai-chat-completions-provider.ts"
import { collectProviderTurn, type ProviderMessage } from "../src/provider.ts"

const servers: Bun.Server<unknown>[] = []

afterEach(() => {
  for (const server of servers.splice(0)) server.stop(true)
})

describe("OpenAI-compatible Chat Completions provider", () => {
  test("streams text and maps fragmented function calls to canonical tool identities", async () => {
    let received: Record<string, unknown> | undefined
    let pathname = ""
    const server = Bun.serve({
      port: 0,
      async fetch(request) {
        pathname = new URL(request.url).pathname
        expect(request.headers.get("authorization")).toBe("Bearer test-chat-credential")
        received = (await request.json()) as Record<string, unknown>
        return chatSse([
          chatChunk({ role: "assistant", content: "I will inspect it." }),
          chatChunk({
            tool_calls: [{ index: 0, id: "call-1", type: "function", function: { name: "jw_0_design_" } }],
          }),
          chatChunk({
            tool_calls: [{ index: 0, function: { name: "query", arguments: '{"selector"' } }],
          }),
          chatChunk({ tool_calls: [{ index: 0, function: { arguments: ':"U1"}' } }] }, "tool_calls"),
          chatUsageChunk(19, 7),
        ])
      },
    })
    servers.push(server)
    const tool = registerQueryTool()
    const completed = await collectProviderTurn(localProvider(server), {
      messages: [{ role: "user", content: "Inspect U1" }],
      tools: [tool],
      signal: new AbortController().signal,
    })

    expect(pathname).toBe("/v1/chat/completions")
    expect(completed).toEqual({
      text: "I will inspect it.",
      reasoning: "",
      toolCalls: [
        {
          id: "call-1",
          name: "design.query",
          input: { selector: "U1" },
          toolIdentity: tool.identity,
        },
      ],
      usage: { inputTokens: 19, outputTokens: 7 },
      finishReason: "tool-calls",
    })
    expect(received).toMatchObject({ model: "test-chat-model", stream: true })
    expect(received?.stream_options).toEqual({ include_usage: true })
    expect(JSON.stringify(received?.tools)).toContain("jw_0_design_query")
    expect(JSON.stringify(received)).not.toContain("test-chat-credential")
  })

  test("replays assistant function calls before tool results on continuation turns", async () => {
    let receivedMessages: unknown
    const server = Bun.serve({
      port: 0,
      async fetch(request) {
        const body = (await request.json()) as { messages: unknown }
        receivedMessages = body.messages
        return chatSse([chatChunk({ content: "done" }, "stop")])
      },
    })
    servers.push(server)
    const tool = registerQueryTool()
    const messages: ProviderMessage[] = [
      { role: "user", content: "Inspect U1" },
      {
        role: "assistant",
        content: "",
        toolCalls: [{ id: "call-1", name: "design.query", input: { selector: "U1" } }],
      },
      { role: "tool", content: '{"ok":true}', toolCallId: "call-1", name: "design.query" },
    ]

    await collectProviderTurn(localProvider(server), {
      messages,
      tools: [tool],
      signal: new AbortController().signal,
    })

    expect(receivedMessages).toEqual([
      { role: "user", content: "Inspect U1" },
      {
        role: "assistant",
        content: null,
        tool_calls: [
          {
            type: "function",
            id: "call-1",
            function: { name: "jw_0_design_query", arguments: '{"selector":"U1"}' },
          },
        ],
      },
      { role: "tool", tool_call_id: "call-1", content: '{"ok":true}' },
    ])
  })
})

function registerQueryTool() {
  return new ToolRegistry().register(
    {
      name: "design.query",
      description: "Query design objects",
      inputSchema: {
        type: "object",
        properties: { selector: { type: "string" } },
        required: ["selector"],
        additionalProperties: false,
      },
      source: "jdp",
      permission: "read",
      effects: ["read"],
      resourceClaims: ["project:test"],
      idempotency: "supported",
      cancellation: "supported",
      timeoutMs: 1_000,
      outputBudget: { inlineBytes: 4_096, overflow: "artifact" },
    },
    async () => ({ ok: true }),
  )
}

function localProvider(server: Bun.Server<unknown>): OpenAIChatCompletionsProvider {
  return new OpenAIChatCompletionsProvider({
    apiKey: "test-chat-credential",
    model: "test-chat-model",
    contextTokens: 8_192,
    outputTokens: 1_024,
    baseURL: `${server.url}v1`,
  })
}

function chatChunk(delta: Record<string, unknown>, finishReason: string | null = null) {
  return {
    id: "chatcmpl-test",
    object: "chat.completion.chunk",
    created: 1,
    model: "test-chat-model",
    choices: [{ index: 0, delta, finish_reason: finishReason }],
  }
}

function chatUsageChunk(promptTokens: number, completionTokens: number) {
  return {
    id: "chatcmpl-test",
    object: "chat.completion.chunk",
    created: 1,
    model: "test-chat-model",
    choices: [],
    usage: {
      prompt_tokens: promptTokens,
      completion_tokens: completionTokens,
      total_tokens: promptTokens + completionTokens,
    },
  }
}

function chatSse(events: unknown[]): Response {
  return new Response(
    `${events.map((event) => `data: ${JSON.stringify(event)}\n\n`).join("")}data: [DONE]\n\n`,
    { headers: { "content-type": "text/event-stream" } },
  )
}
