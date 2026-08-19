import { mkdirSync } from "node:fs"
import { createServer, type Server, type Socket } from "node:net"
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
  runtimePipeName: string
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
lines.close()
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

const server = createServer((socket) => serveRpcConnection(socket, bootstrap.bootstrapToken))
await listen(server, bootstrap.runtimePipeName)

writeLine({
  protocolVersion: "0.1",
  version: "0.1.0",
  target: currentTarget(),
  health: "healthy",
  rpcEndpoint: bootstrap.runtimePipeName,
})

await waitForShutdown(server)

clearInterval(heartbeat)
await closeServer(server)
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
    case "run.cancel": {
      const input = params as { runId: string; reason?: string }
      return runtime.client.cancelRun(input.runId, input.reason)
    }
    case "approval.list":
      return runtime.client.listApprovals((params as { sessionId?: string } | undefined)?.sessionId)
    case "approval.reply":
      return runtime.client.replyApproval(params as Parameters<typeof runtime.client.replyApproval>[0])
    case "events.read": {
      const input = params as { sessionId: string; afterSequence?: number; limit?: number }
      return runtime.events.readAfter(input.sessionId, input.afterSequence ?? 0, input.limit ?? 1_000)
    }
    case "artifact.get":
      return runtime.client.getArtifact((params as { artifactId: string }).artifactId)
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
    "runtimePipeName",
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

function serveRpcConnection(socket: Socket, expectedToken: string): void {
  let buffer = Buffer.alloc(0)
  let serial = Promise.resolve()
  socket.on("data", (chunk: Buffer) => {
    buffer = Buffer.concat([buffer, chunk])
    while (buffer.byteLength >= 4) {
      const length = buffer.readUInt32LE(0)
      if (length === 0 || length > 4 * 1024 * 1024) {
        socket.destroy(new Error(`Invalid Runtime RPC frame length: ${length}`))
        return
      }
      if (buffer.byteLength < 4 + length) return
      const body = buffer.subarray(4, 4 + length)
      buffer = buffer.subarray(4 + length)
      serial = serial.then(() => handleRpcFrame(socket, body, expectedToken))
    }
  })
}

async function handleRpcFrame(socket: Socket, body: Buffer, expectedToken: string): Promise<void> {
  let request: RpcRequest | undefined
  try {
    request = JSON.parse(body.toString("utf8")) as RpcRequest
    if (request.token !== expectedToken) throw new Error("Runtime RPC authentication failed")
    if (typeof request.id !== "string" || typeof request.method !== "string") {
      throw new Error("Runtime RPC envelope is invalid")
    }
    writeFrame(socket, {
      id: request.id,
      success: true,
      result: await dispatch(request.method, request.params),
    })
  } catch (error) {
    writeFrame(socket, {
      id: request?.id ?? "invalid-request",
      success: false,
      error: { code: "runtime-sidecar-error", summary: errorMessage(error) },
    })
  }
}

function writeFrame(socket: Socket, value: unknown): void {
  const body = Buffer.from(JSON.stringify(value), "utf8")
  if (body.byteLength > 4 * 1024 * 1024) throw new Error("Runtime RPC response exceeds frame limit")
  const header = Buffer.allocUnsafe(4)
  header.writeUInt32LE(body.byteLength)
  socket.write(Buffer.concat([header, body]))
}

function listen(server: Server, endpoint: string): Promise<void> {
  return new Promise((resolve, reject) => {
    server.once("error", reject)
    server.listen(endpoint, () => {
      server.off("error", reject)
      resolve()
    })
  })
}

function waitForShutdown(server: Server): Promise<void> {
  return new Promise((resolve) => {
    process.once("SIGINT", resolve)
    process.once("SIGTERM", resolve)
    server.once("close", resolve)
  })
}

function closeServer(server: Server): Promise<void> {
  return new Promise((resolve) => server.close(() => resolve()))
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
