import { mkdirSync } from "node:fs"
import { dirname } from "node:path"
import { createInterface } from "node:readline"
import { createRuntime, GeneratedBindingJdpClient } from "@jingwei/agent-runtime"
import { NamedPipeJdpBinding } from "./named-pipe-jdp.ts"

interface BootstrapConfig {
  bootstrapToken: string
  daemonPipeName: string
  daemonSessionToken: string
  databasePath: string
  workspaceId: string
  projectId: string
}

interface RpcRequest {
  id: string
  token: string
  method: string
  params?: unknown
}

const lines = createInterface({ input: process.stdin, crlfDelay: Number.POSITIVE_INFINITY })
const iterator = lines[Symbol.asyncIterator]()
const bootstrapLine = await iterator.next()
if (bootstrapLine.done) throw new Error("Runtime sidecar bootstrap config is missing")
const bootstrap = parseBootstrap(bootstrapLine.value)
mkdirSync(dirname(bootstrap.databasePath), { recursive: true })

const binding = await NamedPipeJdpBinding.connect({
  pipeName: bootstrap.daemonPipeName,
  sessionToken: bootstrap.daemonSessionToken,
  workspaceId: bootstrap.workspaceId,
  projectId: bootstrap.projectId,
})
const runtime = await createRuntime({
  databasePath: bootstrap.databasePath,
  jdpClient: new GeneratedBindingJdpClient(binding),
  policyRules: [],
})
await runtime.recovery.recover()
const heartbeat = setInterval(() => runtime.recovery.heartbeat(), 5_000)

writeLine({
  protocolVersion: "0.1",
  version: "0.1.0",
  target: currentTarget(),
  health: "healthy",
})

for (;;) {
  const next = await iterator.next()
  if (next.done) break
  if (!next.value.trim()) continue
  let request: RpcRequest | undefined
  try {
    request = JSON.parse(next.value) as RpcRequest
    if (request.token !== bootstrap.bootstrapToken) throw new Error("Runtime RPC authentication failed")
    const result = await dispatch(request.method, request.params)
    writeLine({ id: request.id, success: true, result })
  } catch (error) {
    writeLine({
      id: request?.id ?? "invalid-request",
      success: false,
      error: { code: "runtime-sidecar-error", summary: errorMessage(error) },
    })
  }
}

clearInterval(heartbeat)
binding.close()
runtime[Symbol.dispose]()

async function dispatch(method: string, params: unknown): Promise<unknown> {
  switch (method) {
    case "health":
      return { status: "healthy", processId: process.pid }
    case "capabilities":
      return runtime.client.capabilities()
    case "session.create":
      return runtime.client.createSession(params as Parameters<typeof runtime.client.createSession>[0])
    case "input.admit":
      return runtime.client.admitInput(params as Parameters<typeof runtime.client.admitInput>[0])
    case "run.resume":
      return runtime.client.resumeRun((params as { sessionId: string }).sessionId)
    case "approval.list":
      return runtime.client.listApprovals((params as { sessionId?: string } | undefined)?.sessionId)
    case "approval.reply":
      return runtime.client.replyApproval(params as Parameters<typeof runtime.client.replyApproval>[0])
    default:
      throw new Error(`Unsupported Runtime RPC method: ${method}`)
  }
}

function parseBootstrap(line: string): BootstrapConfig {
  const value = JSON.parse(line) as Partial<BootstrapConfig>
  for (const key of [
    "bootstrapToken",
    "daemonPipeName",
    "daemonSessionToken",
    "databasePath",
    "workspaceId",
    "projectId",
  ] as const) {
    if (typeof value[key] !== "string" || value[key].length < 3) {
      throw new Error(`Runtime sidecar bootstrap field is invalid: ${key}`)
    }
  }
  const config = value as BootstrapConfig
  if (config.bootstrapToken.length < 32 || config.daemonSessionToken.length < 32) {
    throw new Error("Runtime sidecar bootstrap tokens are too short")
  }
  return config
}

function currentTarget(): string {
  if (process.platform === "win32" && process.arch === "x64") return "x86_64-pc-windows-msvc"
  if (process.platform === "win32" && process.arch === "arm64") return "aarch64-pc-windows-msvc"
  if (process.platform === "darwin" && process.arch === "arm64") return "aarch64-apple-darwin"
  if (process.platform === "linux" && process.arch === "x64") return "x86_64-unknown-linux-gnu"
  return "unsupported-target"
}

function writeLine(value: unknown): void {
  process.stdout.write(`${JSON.stringify(value)}\n`)
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message.slice(0, 500) : String(error).slice(0, 500)
}
