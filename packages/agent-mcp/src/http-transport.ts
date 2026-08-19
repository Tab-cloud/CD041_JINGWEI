import type { McpServerConfig, McpTransport, McpTransportFactory } from "./types.ts"

export class HttpMcpTransport implements McpTransport {
  private requestId = 0
  private sessionId?: string
  private protocolVersion?: string

  constructor(
    private readonly endpoint: string,
    private readonly headers: Record<string, string>,
    private readonly maxResponseBytes: number,
  ) {}

  async request<T>(
    method: string,
    params: Record<string, unknown>,
    options: { signal?: AbortSignal; timeoutMs: number },
  ): Promise<T> {
    const controller = linkedController(options.signal, options.timeoutMs)
    try {
      const response = await fetch(this.endpoint, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          accept: "application/json, text/event-stream",
          ...this.headers,
          ...(this.sessionId ? { "mcp-session-id": this.sessionId } : {}),
          ...(this.protocolVersion ? { "mcp-protocol-version": this.protocolVersion } : {}),
        },
        body: JSON.stringify({ jsonrpc: "2.0", id: ++this.requestId, method, params }),
        signal: controller.signal,
      })
      if (!response.ok) throw new Error(`MCP HTTP ${response.status}`)
      const sessionId = response.headers.get("mcp-session-id")
      if (sessionId) this.sessionId = sessionId
      const text = await readBounded(response, this.maxResponseBytes)
      const envelope = parseEnvelope<T>(text, response.headers.get("content-type"))
      if (envelope.error) throw new Error(`MCP ${envelope.error.code}: ${envelope.error.message}`)
      if (method === "initialize") {
        const version = (envelope.result as { protocolVersion?: unknown } | undefined)?.protocolVersion
        if (typeof version === "string") this.protocolVersion = version
      }
      return envelope.result as T
    } finally {
      controller.abort()
    }
  }

  async notify(method: string, params: Record<string, unknown> = {}): Promise<void> {
    const response = await fetch(this.endpoint, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        accept: "application/json, text/event-stream",
        ...this.headers,
        ...(this.sessionId ? { "mcp-session-id": this.sessionId } : {}),
        ...(this.protocolVersion ? { "mcp-protocol-version": this.protocolVersion } : {}),
      },
      body: JSON.stringify({ jsonrpc: "2.0", method, params }),
    })
    if (!response.ok) throw new Error(`MCP notification HTTP ${response.status}`)
  }

  close(): void {}
}

function parseEnvelope<T>(
  text: string,
  contentType: string | null,
): { result?: T; error?: { code: number; message: string } } {
  if (!contentType?.includes("text/event-stream")) return JSON.parse(text)
  const data = text
    .split(/\r?\n/)
    .filter((line) => line.startsWith("data:"))
    .map((line) => line.slice(5).trim())
    .filter((line) => line && line !== "[DONE]")
  if (data.length === 0) throw new Error("MCP SSE response did not contain a JSON-RPC envelope")
  return JSON.parse(data.at(-1) as string)
}

export class DefaultMcpTransportFactory implements McpTransportFactory {
  create(config: McpServerConfig, credentials: { headers?: Record<string, string> }): McpTransport {
    if (config.transport !== "http") {
      throw new Error("stdio transport requires a platform-specific factory")
    }
    return new HttpMcpTransport(config.endpoint, credentials.headers ?? {}, config.outputLimitBytes)
  }
}

async function readBounded(response: Response, limit: number): Promise<string> {
  const reader = response.body?.getReader()
  if (!reader) return ""
  const chunks: Uint8Array[] = []
  let total = 0
  while (true) {
    const { done, value } = await reader.read()
    if (done) break
    total += value.byteLength
    if (total > limit) {
      await reader.cancel()
      throw new Error(`MCP response exceeds ${limit} bytes`)
    }
    chunks.push(value)
  }
  const bytes = new Uint8Array(total)
  let offset = 0
  for (const chunk of chunks) {
    bytes.set(chunk, offset)
    offset += chunk.byteLength
  }
  return new TextDecoder().decode(bytes)
}

function linkedController(parent: AbortSignal | undefined, timeoutMs: number): AbortController {
  const controller = new AbortController()
  const timer = setTimeout(
    () => controller.abort(new DOMException("MCP call timed out", "TimeoutError")),
    timeoutMs,
  )
  const abort = () => controller.abort(parent?.reason)
  parent?.addEventListener("abort", abort, { once: true })
  controller.signal.addEventListener(
    "abort",
    () => {
      clearTimeout(timer)
      parent?.removeEventListener("abort", abort)
    },
    { once: true },
  )
  if (parent?.aborted) abort()
  return controller
}
