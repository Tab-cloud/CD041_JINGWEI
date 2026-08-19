import { afterEach, describe, expect, test } from "bun:test"
import { ToolRegistry } from "@jingwei/agent-tools"
import { OpenAIResponsesProvider } from "../src/openai-responses-provider.ts"
import { collectProviderTurn, type ProviderMessage } from "../src/provider.ts"

const servers: Bun.Server<unknown>[] = []

afterEach(() => {
  for (const server of servers.splice(0)) server.stop(true)
})

describe("OpenAI Responses provider", () => {
  test("uses the official SDK over SSE and maps function calls back to canonical tool identities", async () => {
    let received: Record<string, unknown> | undefined
    const server = Bun.serve({
      port: 0,
      async fetch(request) {
        expect(request.headers.get("authorization")).toBe("Bearer test-credential")
        received = (await request.json()) as Record<string, unknown>
        return sse([
          {
            type: "response.reasoning_summary_text.delta",
            delta: "bounded reasoning",
            item_id: "reasoning-1",
            output_index: 0,
            summary_index: 0,
            sequence_number: 1,
          },
          {
            type: "response.output_text.delta",
            delta: "I will inspect it.",
            item_id: "message-1",
            output_index: 1,
            content_index: 0,
            logprobs: [],
            sequence_number: 2,
          },
          {
            type: "response.output_item.done",
            item: {
              type: "function_call",
              id: "item-1",
              call_id: "call-1",
              name: "jw_0_design_query",
              arguments: '{"selector":"U1"}',
              status: "completed",
            },
            output_index: 2,
            sequence_number: 3,
          },
          completedEvent(4, 11, 7),
        ])
      },
    })
    servers.push(server)
    const tool = new ToolRegistry().register(
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
    const provider = localProvider(server)
    const completed = await collectProviderTurn(provider, {
      messages: [{ role: "user", content: "Inspect U1" }],
      tools: [tool],
      signal: new AbortController().signal,
    })

    expect(completed).toEqual({
      text: "I will inspect it.",
      reasoning: "bounded reasoning",
      toolCalls: [
        {
          id: "call-1",
          name: "design.query",
          input: { selector: "U1" },
          toolIdentity: tool.identity,
        },
      ],
      usage: { inputTokens: 11, outputTokens: 7 },
      finishReason: "tool-calls",
    })
    expect(received?.model).toBe("test-model")
    expect(received?.store).toBe(false)
    expect(JSON.stringify(received?.tools)).toContain("jw_0_design_query")
    expect(JSON.stringify(received)).not.toContain("test-credential")
  })

  test("replays function-call items before their outputs on a continuation turn", async () => {
    let receivedInput: unknown
    const server = Bun.serve({
      port: 0,
      async fetch(request) {
        const body = (await request.json()) as { input: unknown }
        receivedInput = body.input
        return sse([completedEvent(1, 9, 2)])
      },
    })
    servers.push(server)
    const registry = new ToolRegistry()
    const tool = registry.register(
      {
        name: "design.query",
        description: "Query design objects",
        inputSchema: { type: "object" },
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

    expect(receivedInput).toEqual([
      { role: "user", content: "Inspect U1", type: "message" },
      { role: "assistant", content: "", type: "message" },
      {
        type: "function_call",
        call_id: "call-1",
        name: "jw_0_design_query",
        arguments: '{"selector":"U1"}',
      },
      { type: "function_call_output", call_id: "call-1", output: '{"ok":true}' },
    ])
  })
})

const apiKey = process.env.OPENAI_API_KEY
const liveModel = process.env.JINGWEI_OPENAI_MODEL
const liveTest = apiKey && liveModel ? test : test.skip

liveTest("live: receives a streamed response from the configured commercial OpenAI model", async () => {
  const provider = new OpenAIResponsesProvider({
    apiKey: apiKey as string,
    model: liveModel as string,
    contextTokens: Number(process.env.JINGWEI_OPENAI_CONTEXT_TOKENS ?? 128_000),
    outputTokens: 128,
  })
  const completed = await collectProviderTurn(provider, {
    messages: [{ role: "user", content: "Reply with exactly JINGWEI_LIVE_OK" }],
    tools: [],
    signal: AbortSignal.timeout(60_000),
    maxOutputTokens: 128,
  })
  expect(completed.text).toContain("JINGWEI_LIVE_OK")
  expect(completed.usage?.inputTokens).toBeGreaterThan(0)
  expect(completed.finishReason).toBe("stop")
})

function localProvider(server: Bun.Server<unknown>): OpenAIResponsesProvider {
  return new OpenAIResponsesProvider({
    apiKey: "test-credential",
    model: "test-model",
    contextTokens: 8_192,
    outputTokens: 1_024,
    baseURL: `${server.url}v1`,
    reasoning: true,
    image: false,
    pdf: false,
  })
}

function completedEvent(sequence: number, inputTokens: number, outputTokens: number) {
  return {
    type: "response.completed",
    sequence_number: sequence,
    response: {
      id: "response-1",
      status: "completed",
      error: null,
      usage: {
        input_tokens: inputTokens,
        output_tokens: outputTokens,
        total_tokens: inputTokens + outputTokens,
      },
    },
  }
}

function sse(events: unknown[]): Response {
  return new Response(
    `${events.map((event) => `data: ${JSON.stringify(event)}\n\n`).join("")}data: [DONE]\n\n`,
    {
      headers: { "content-type": "text/event-stream" },
    },
  )
}
