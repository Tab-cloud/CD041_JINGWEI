import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-http"
import { BatchSpanProcessor, NodeTracerProvider } from "@opentelemetry/sdk-trace-node"
import { OpenTelemetryRuntimeTracer } from "./tracing.ts"

export interface OtlpTracingOptions {
  url: string
  headers?: Record<string, string>
  timeoutMs?: number
  instrumentationName?: string
  scheduledDelayMs?: number
  maxExportBatchSize?: number
}

export interface OtlpTracingService {
  tracer: OpenTelemetryRuntimeTracer
  forceFlush(): Promise<void>
  shutdown(): Promise<void>
}

/** Creates an isolated OTel SDK provider that exports runtime spans over OTLP/HTTP JSON. */
export function createOtlpTracing(options: OtlpTracingOptions): OtlpTracingService {
  const endpoint = new URL(options.url)
  if (endpoint.protocol !== "http:" && endpoint.protocol !== "https:") {
    throw new Error("OTLP trace endpoint must use HTTP or HTTPS")
  }
  const exporter = new OTLPTraceExporter({
    url: endpoint.toString(),
    ...(options.headers ? { headers: options.headers } : {}),
    timeoutMillis: options.timeoutMs ?? 10_000,
  })
  const processor = new BatchSpanProcessor(exporter, {
    scheduledDelayMillis: options.scheduledDelayMs ?? 1_000,
    maxExportBatchSize: options.maxExportBatchSize ?? 128,
  })
  const provider = new NodeTracerProvider({ spanProcessors: [processor] })
  return {
    tracer: new OpenTelemetryRuntimeTracer(options.instrumentationName ?? "jingwei-agent-runtime", provider),
    forceFlush: () => provider.forceFlush(),
    shutdown: () => provider.shutdown(),
  }
}
