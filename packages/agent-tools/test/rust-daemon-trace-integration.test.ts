import { afterEach, expect, test } from "bun:test"
import { type ChildProcessWithoutNullStreams, spawn } from "node:child_process"
import { mkdtemp, readFile, rm } from "node:fs/promises"
import { createConnection, type Socket } from "node:net"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import type { JdpRequest } from "@jingwei/jdp-bindings"

const temporaryDirectories: string[] = []

afterEach(async () => {
  await Promise.all(
    temporaryDirectories.splice(0).map((directory) => rm(directory, { force: true, recursive: true })),
  )
})

test("formal TypeScript binding propagates trace context into a Rust daemon child span", async () => {
  if (process.platform !== "win32") return

  const workspaceRoot = await mkdtemp(join(tmpdir(), "jingwei-trace-bridge-"))
  temporaryDirectories.push(workspaceRoot)
  const traceOutput = join(workspaceRoot, "daemon-spans.json")
  const pipeName = `\\\\.\\pipe\\jingwei-trace-${crypto.randomUUID()}`
  const repositoryRoot = resolve(import.meta.dir, "../../..")
  const sessionToken = `test-${crypto.randomUUID()}-${crypto.randomUUID()}`
  const child = spawn(
    "cargo",
    [
      "run",
      "--quiet",
      "-p",
      "jingwei-daemon",
      "--example",
      "trace_bridge_server",
      "--",
      workspaceRoot,
      pipeName,
      traceOutput,
    ],
    { cwd: repositoryRoot },
  )
  child.stdin.end(sessionToken)

  try {
    await waitForReady(child)
    const socket = await connectWithRetry(pipeName)
    const frames = framedReader(socket)
    writeFrame(socket, {
      protocolVersions: ["0.1"],
      workspaceId: "ws-typescript-rust-trace",
      clientName: "bun-cross-runtime-test",
      clientVersion: "0.1.0",
      sessionToken,
    })
    expect(JSON.stringify(await frames.next())).toContain('"protocolVersion":"0.1"')

    const request: JdpRequest<Record<string, never>> = {
      protocolVersion: "0.1",
      requestId: "request-typescript-rust-trace",
      idempotencyKey: "idem-typescript-rust-trace",
      workspaceId: "ws-typescript-rust-trace",
      projectId: "project-typescript-rust-trace",
      operation: "workspace.capabilities",
      deadlineMs: 5_000,
      traceContext: {
        traceId: "0123456789abcdef0123456789abcdef",
        spanId: "fedcba9876543210",
        traceFlags: 1,
      },
      input: {},
    }
    writeFrame(socket, request)
    expect((await frames.next()) as { success?: boolean }).toMatchObject({ success: true })
    socket.end()

    const exitCode = await waitForExit(child)
    expect(exitCode).toBe(0)
    const spans = JSON.parse(await readFile(traceOutput, "utf8")) as Array<{
      traceId: string
      spanId: string
      parentSpanId: string
      operation: string
      status: string
    }>
    expect(spans).toHaveLength(1)
    expect(spans[0]).toMatchObject({
      traceId: request.traceContext?.traceId,
      parentSpanId: request.traceContext?.spanId,
      operation: request.operation,
      status: "ok",
    })
    expect(spans[0]?.spanId).toMatch(/^[0-9a-f]{16}$/)
    expect(spans[0]?.spanId).not.toBe(request.traceContext?.spanId)
  } finally {
    if (child.exitCode === null) child.kill()
  }
}, 30_000)

function writeFrame(socket: Socket, value: unknown): void {
  const body = Buffer.from(JSON.stringify(value), "utf8")
  const header = Buffer.allocUnsafe(4)
  header.writeUInt32LE(body.byteLength)
  socket.write(Buffer.concat([header, body]))
}

function framedReader(socket: Socket): { next(): Promise<unknown> } {
  let buffer = Buffer.alloc(0)
  const values: unknown[] = []
  const waiters: Array<(value: unknown) => void> = []
  socket.on("data", (chunk: Buffer) => {
    buffer = Buffer.concat([buffer, chunk])
    while (buffer.byteLength >= 4) {
      const length = buffer.readUInt32LE(0)
      if (buffer.byteLength < 4 + length) return
      const value = JSON.parse(buffer.subarray(4, 4 + length).toString("utf8")) as unknown
      buffer = buffer.subarray(4 + length)
      const waiter = waiters.shift()
      if (waiter) waiter(value)
      else values.push(value)
    }
  })
  return {
    next: () => {
      const value = values.shift()
      return value === undefined
        ? new Promise((resolveValue) => waiters.push(resolveValue))
        : Promise.resolve(value)
    },
  }
}

async function connectWithRetry(pipeName: string): Promise<Socket> {
  for (let attempt = 0; attempt < 40; attempt += 1) {
    try {
      return await new Promise<Socket>((resolveSocket, reject) => {
        const socket = createConnection(pipeName)
        socket.once("connect", () => resolveSocket(socket))
        socket.once("error", reject)
      })
    } catch {
      await Bun.sleep(50)
    }
  }
  throw new Error("Rust daemon named pipe did not become available")
}

async function waitForReady(child: ChildProcessWithoutNullStreams): Promise<void> {
  let output = ""
  await new Promise<void>((resolveReady, reject) => {
    const timeout = setTimeout(() => reject(new Error("Rust daemon helper did not become ready")), 20_000)
    child.stdout.on("data", (chunk: Buffer) => {
      output += chunk.toString("utf8")
      if (output.includes("READY")) {
        clearTimeout(timeout)
        resolveReady()
      }
    })
    child.once("exit", (code) => {
      clearTimeout(timeout)
      reject(new Error(`Rust daemon helper exited early with ${code}`))
    })
  })
}

async function waitForExit(child: ChildProcessWithoutNullStreams): Promise<number | null> {
  if (child.exitCode !== null) return child.exitCode
  return await new Promise((resolveExit) => child.once("exit", resolveExit))
}
