import type { TraceContext } from "@jingwei/agent-protocol"
import {
  type Attributes,
  context,
  propagation,
  type Span,
  SpanStatusCode,
  type TracerProvider,
  trace,
} from "@opentelemetry/api"
import { safeTelemetryAttributes } from "./redaction.ts"

export const SPAN_NAMES = [
  "agent.run",
  "model.attempt",
  "tool.execution",
  "approval.wait",
  "mcp.call",
  "jdp.request",
] as const

export type RuntimeSpanName = (typeof SPAN_NAMES)[number]

export interface SpanHandle {
  context: TraceContext
  setAttributes(attributes: Record<string, unknown>): void
  end(status?: "ok" | "error", errorMessage?: string): void
}

export interface RuntimeTracer {
  startSpan(name: RuntimeSpanName, attributes?: Record<string, unknown>, parent?: TraceContext): SpanHandle
}

export class OpenTelemetryRuntimeTracer implements RuntimeTracer {
  constructor(
    private readonly instrumentationName = "jingwei-agent-runtime",
    private readonly tracerProvider: TracerProvider = trace.getTracerProvider(),
  ) {}

  startSpan(
    name: RuntimeSpanName,
    attributes: Record<string, unknown> = {},
    parent?: TraceContext,
  ): SpanHandle {
    const tracer = this.tracerProvider.getTracer(this.instrumentationName)
    const parentContext = parent
      ? trace.setSpanContext(context.active(), {
          traceId: parent.traceId,
          spanId: parent.spanId,
          traceFlags: parent.traceFlags ?? 1,
          isRemote: true,
        })
      : context.active()
    const span = tracer.startSpan(
      name,
      { attributes: safeTelemetryAttributes(attributes) as Attributes },
      parentContext,
    )
    return spanHandle(span)
  }
}

export class RecordingRuntimeTracer implements RuntimeTracer {
  readonly spans: Array<{
    name: RuntimeSpanName
    attributes: Record<string, string | number | boolean>
    context: TraceContext
    status?: "ok" | "error"
    errorMessage?: string
  }> = []

  startSpan(
    name: RuntimeSpanName,
    attributes: Record<string, unknown> = {},
    parent?: TraceContext,
  ): SpanHandle {
    const traceId = parent?.traceId ?? crypto.randomUUID().replaceAll("-", "").padEnd(32, "0").slice(0, 32)
    const record = {
      name,
      attributes: safeTelemetryAttributes(attributes),
      context: { traceId, spanId: randomSpanId(), traceFlags: 1 },
    }
    this.spans.push(record)
    return {
      context: record.context,
      setAttributes: (next) => Object.assign(record.attributes, safeTelemetryAttributes(next)),
      end: (status = "ok", errorMessage) => {
        Object.assign(record, {
          status,
          ...(errorMessage ? { errorMessage: errorMessage.slice(0, 500) } : {}),
        })
      },
    }
  }
}

function spanHandle(span: Span): SpanHandle {
  const spanContext = span.spanContext()
  return {
    context: { traceId: spanContext.traceId, spanId: spanContext.spanId, traceFlags: spanContext.traceFlags },
    setAttributes: (attributes) => span.setAttributes(safeTelemetryAttributes(attributes) as Attributes),
    end: (status = "ok", errorMessage) => {
      span.setStatus({
        code: status === "ok" ? SpanStatusCode.OK : SpanStatusCode.ERROR,
        ...(errorMessage ? { message: errorMessage } : {}),
      })
      span.end()
    },
  }
}

function randomSpanId(): string {
  return crypto.randomUUID().replaceAll("-", "").slice(0, 16)
}

export function injectTraceContext(traceContext: TraceContext): Record<string, string> {
  const carrier: Record<string, string> = {}
  const parent = trace.setSpanContext(context.active(), {
    traceId: traceContext.traceId,
    spanId: traceContext.spanId,
    traceFlags: traceContext.traceFlags ?? 1,
    isRemote: false,
  })
  propagation.inject(parent, carrier)
  return carrier
}
