export interface McpServerConfig {
  id: string
  transport: "http" | "stdio"
  endpoint: string
  credentialRef?: string
  enabledTools: string[]
  disabledTools: string[]
  timeoutMs: number
  outputLimitBytes: number
  callsPerMinute: number
}

export interface McpRemoteTool {
  name: string
  description?: string
  inputSchema?: Record<string, unknown>
  outputSchema?: Record<string, unknown>
  annotations?: Record<string, unknown>
}

export interface McpTransport {
  request<T>(
    method: string,
    params: Record<string, unknown>,
    options: { signal?: AbortSignal; timeoutMs: number },
  ): Promise<T>
  notify(method: string, params?: Record<string, unknown>): Promise<void>
  close(): void
}

export interface McpCredentialProvider {
  resolve(reference: string): Promise<{ headers?: Record<string, string>; env?: Record<string, string> }>
}

export interface McpTransportFactory {
  create(
    config: McpServerConfig,
    credentials: { headers?: Record<string, string>; env?: Record<string, string> },
  ): McpTransport
}

export interface McpServerView {
  id: string
  status: "stopped" | "connecting" | "connected" | "error"
  error?: string
  instructions?: { value: string; trust: "untrusted" }
  tools: Array<{ remoteName: string; registeredName: string; identity: string }>
  credentialRef?: string
}
