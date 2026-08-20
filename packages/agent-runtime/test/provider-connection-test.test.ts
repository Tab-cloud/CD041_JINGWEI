import { describe, expect, test } from "bun:test"
import type { ProviderConnectionTestInput } from "@jingwei/agent-protocol"
import { testProviderConnection } from "../src/provider-connection-test.ts"

const input: ProviderConnectionTestInput = {
  provider: "openai-responses",
  baseUrl: "https://compatible.example/v1",
  model: "model-example-1",
  apiKey: "unit-test-secret",
  timeoutMs: 10_000,
}

describe("provider connection test", () => {
  test("executes a bounded Responses request and returns content-free evidence", async () => {
    let requestBody: Record<string, unknown> | undefined
    const result = await testProviderConnection(input, {
      fetch: async (request, init) => {
        const resolved = new Request(request, init)
        expect(resolved.url).toBe("https://compatible.example/v1/responses")
        expect(resolved.headers.get("authorization")).toBe("Bearer unit-test-secret")
        requestBody = (await resolved.json()) as Record<string, unknown>
        return sse([completedEvent(17, 5)])
      },
    })

    expect(result).toMatchObject({
      status: "passed",
      provider: "openai-responses",
      protocol: "openai-responses",
      endpointHost: "compatible.example",
      model: "model-example-1",
      usage: { inputTokens: 17, outputTokens: 5 },
    })
    expect(requestBody).toMatchObject({ model: "model-example-1", stream: true, store: false })
    expect(JSON.stringify(result)).not.toContain("unit-test-secret")
    expect(JSON.stringify(result)).not.toContain("JINGWEI_PROVIDER_TEST_OK")
  })

  test("maps authentication failures without returning upstream content or credentials", async () => {
    const result = await testProviderConnection(input, {
      fetch: async () =>
        new Response(JSON.stringify({ error: { message: "unit-test-secret is invalid" } }), {
          status: 401,
          headers: { "content-type": "application/json" },
        }),
    })

    expect(result).toMatchObject({
      status: "failed",
      error: {
        code: "authentication-failed",
        httpStatus: 401,
        retryable: false,
      },
    })
    expect(JSON.stringify(result)).not.toContain("unit-test-secret")
    expect(JSON.stringify(result)).not.toContain("is invalid")
  })

  test("rejects unsafe endpoints before attempting a network request", async () => {
    let called = false
    const result = await testProviderConnection(
      { ...input, baseUrl: "https://user:secret@example.com/v1" },
      {
        fetch: async () => {
          called = true
          return new Response()
        },
      },
    )

    expect(called).toBe(false)
    expect(result).toMatchObject({ status: "failed", error: { code: "validation-failed" } })
    expect(result.endpointHost).toBe("invalid-endpoint")
  })
})

function completedEvent(inputTokens: number, outputTokens: number) {
  return {
    type: "response.completed",
    sequence_number: 1,
    response: {
      id: "response-test",
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
    { headers: { "content-type": "text/event-stream" } },
  )
}
