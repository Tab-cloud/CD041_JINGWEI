import { afterEach, describe, expect, test } from "bun:test"
import { createOtlpTracing } from "../src/otlp.ts"

const servers: Bun.Server<unknown>[] = []

afterEach(() => {
  for (const server of servers.splice(0)) server.stop(true)
})

describe("OTLP export", () => {
  test("exports a sanitized Runtime jdp.request span with supplied parent context", async () => {
    const requests: Array<{ headers: Headers; payload: OtlpPayload }> = []
    const collector = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      async fetch(request) {
        requests.push({ headers: request.headers, payload: (await request.json()) as OtlpPayload })
        return Response.json({ partialSuccess: {} })
      },
    })
    servers.push(collector)
    const service = createOtlpTracing({
      url: `${collector.url}v1/traces`,
      headers: { "x-collector-tenant": "test-tenant" },
      scheduledDelayMs: 60_000,
    })
    const parentTraceId = "a".repeat(32)
    const parentSpanId = "b".repeat(16)
    const span = service.tracer.startSpan(
      "jdp.request",
      {
        sessionId: "session-1",
        runId: "run-1",
        operation: "design.query",
        apiKey: "must-not-export",
        prompt: "private design prompt",
      },
      { traceId: parentTraceId, spanId: parentSpanId, traceFlags: 1 },
    )
    span.end("ok")
    await service.forceFlush()
    await service.shutdown()

    expect(requests).toHaveLength(1)
    expect(requests[0]?.headers.get("content-type")).toContain("application/json")
    expect(requests[0]?.headers.get("x-collector-tenant")).toBe("test-tenant")
    const exported = requests[0]?.payload.resourceSpans?.[0]?.scopeSpans?.[0]?.spans?.[0]
    expect(exported?.name).toBe("jdp.request")
    expect(exported?.traceId).toBe(parentTraceId)
    expect(exported?.parentSpanId).toBe(parentSpanId)
    expect(exported?.attributes.map((attribute) => attribute.key).sort()).toEqual([
      "operation",
      "runId",
      "sessionId",
    ])
    expect(JSON.stringify(requests[0]?.payload)).not.toContain("must-not-export")
    expect(JSON.stringify(requests[0]?.payload)).not.toContain("private design prompt")
  })
})

interface OtlpPayload {
  resourceSpans?: Array<{
    scopeSpans?: Array<{
      spans?: Array<{
        name: string
        traceId: string
        spanId: string
        parentSpanId?: string
        attributes: Array<{ key: string; value: Record<string, unknown> }>
      }>
    }>
  }>
}
