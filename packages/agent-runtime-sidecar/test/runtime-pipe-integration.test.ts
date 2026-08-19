import { afterEach, describe, expect, test } from "bun:test"
import { mkdirSync, mkdtempSync } from "node:fs"
import { createConnection, type Socket } from "node:net"
import { tmpdir } from "node:os"
import { join } from "node:path"

const processes: Array<ReturnType<typeof Bun.spawn>> = []

afterEach(() => {
  for (const process of processes.splice(0)) process.kill()
})

describe("Runtime production local RPC", () => {
  test("creates a durable session while a second daemon client commits a real draft patch", async () => {
    if (process.platform !== "win32") return
    const build = Bun.spawnSync(["cargo", "build", "-q", "-p", "jingwei-daemon"], {
      cwd: join(import.meta.dir, "..", "..", ".."),
      stdout: "pipe",
      stderr: "pipe",
    })
    expect(build.exitCode).toBe(0)

    const root = mkdtempSync(join(tmpdir(), "jingwei-runtime-rpc-"))
    const workspace = join(root, "workspace")
    const database = join(root, "runtime", "runtime.sqlite")
    mkdirSync(join(root, "runtime"), { recursive: true })
    const daemonPipeName = String.raw`\\.\pipe\jingwei-daemon-test-${crypto.randomUUID()}`
    const runtimePipeName = String.raw`\\.\pipe\jingwei-runtime-test-${crypto.randomUUID()}`
    const daemonToken = `daemon-${crypto.randomUUID()}-${crypto.randomUUID()}`
    const runtimeToken = `runtime-${crypto.randomUUID()}-${crypto.randomUUID()}`
    const repository = join(import.meta.dir, "..", "..", "..")

    const daemon = Bun.spawn(
      [
        join(repository, "target", "debug", "jingwei-daemon.exe"),
        "serve",
        "--workspace",
        workspace,
        "--workspace-id",
        "ws-desktop",
        "--pipe-name",
        daemonPipeName,
      ],
      { cwd: repository, stdin: "pipe", stdout: "pipe", stderr: "pipe" },
    )
    processes.push(daemon)
    daemon.stdin.write(`${daemonToken}\n`)
    expect(JSON.parse(await readFirstLine(daemon.stdout)).health).toBe("healthy")

    const runtime = Bun.spawn(["bun", "packages/agent-runtime-sidecar/src/main.ts"], {
      cwd: repository,
      stdin: "pipe",
      stdout: "pipe",
      stderr: "pipe",
    })
    processes.push(runtime)
    runtime.stdin.write(
      `${JSON.stringify({
        bootstrapToken: runtimeToken,
        daemonPipeName,
        daemonSessionToken: daemonToken,
        databasePath: database,
        workspaceId: "ws-desktop",
        projectId: "project-desktop",
        runtimePipeName,
      })}\n`,
    )
    const runtimeHandshake = JSON.parse(await readFirstLine(runtime.stdout))
    expect(runtimeHandshake).toMatchObject({ health: "healthy", rpcEndpoint: runtimePipeName })

    const runtimeRpc = await FramedClient.connect(runtimePipeName)
    const health = await runtimeRpc.request({
      id: "runtime-health",
      token: runtimeToken,
      method: "health",
      params: {},
    })
    expect(health).toMatchObject({ success: true, result: { status: "healthy" } })
    const session = await runtimeRpc.request({
      id: "runtime-session",
      token: runtimeToken,
      method: "session.create",
      params: {
        projectId: "project-desktop",
        workspaceId: "ws-desktop",
        title: "Integration session",
        dataPolicy: "local",
      },
    })
    expect(session).toMatchObject({ success: true, result: { projectId: "project-desktop" } })

    const daemonClient = await FramedClient.connect(daemonPipeName)
    await daemonClient.write({
      protocolVersions: ["0.1"],
      workspaceId: "ws-desktop",
      clientName: "integration-workbench",
      clientVersion: "0.1.0",
      sessionToken: daemonToken,
    })
    expect(await daemonClient.read()).toMatchObject({ Ok: { protocolVersion: "0.1" } })
    const created = await jdp(daemonClient, "project.create", undefined, { name: "Real transport gate" })
    expect(created.success).toBe(true)
    const head = created.data.currentRevision as string
    const opened = await jdp(daemonClient, "edit_session.open", undefined, {
      baseRevision: head,
      leaseSeconds: 1_800,
    })
    const queried = await jdp(daemonClient, "design.query", undefined, {
      revision: opened.data.draftHeadRevision,
      kind: "core.project",
      ids: [],
      limit: 10,
      cursor: null,
    })
    const entityId = queried.data.entities[0].id as string
    const group = {
      editSessionId: opened.data.editSessionId,
      baseDraftRevision: opened.data.draftHeadRevision,
      commands: [{ kind: "core.set_property", payload: { entityId, key: "transportGate", value: "passed" } }],
      expectedPostconditions: ["graph.integrity"],
      previewHash: null,
    }
    const preview = await jdp(daemonClient, "edit_command.preview", undefined, group)
    const committed = await jdp(daemonClient, "edit_command.commit", undefined, {
      ...group,
      previewHash: preview.data.previewHash,
    })
    expect(committed.success).toBe(true)
    expect(committed.data.draftRevision).not.toBe(head)
    const project = await jdp(daemonClient, "project.get", undefined, {})
    expect(project.data.currentRevision).toBe(head)

    daemonClient.close()
    runtimeRpc.close()
  }, 30_000)
})

async function jdp(
  client: FramedClient,
  operation: string,
  baseRevision: string | undefined,
  input: unknown,
) {
  return client.request({
    protocolVersion: "0.1",
    requestId: `request-${crypto.randomUUID()}`,
    idempotencyKey: `idem-${crypto.randomUUID()}`,
    workspaceId: "ws-desktop",
    projectId: "project-desktop",
    operation,
    ...(baseRevision ? { baseRevision } : {}),
    deadlineMs: 10_000,
    dryRun: false,
    input,
  }) as Promise<any>
}

class FramedClient {
  private buffer = Buffer.alloc(0)
  private readonly values: unknown[] = []
  private readonly waiters: Array<(value: unknown) => void> = []

  private constructor(private readonly socket: Socket) {
    socket.on("data", (chunk: Buffer) => {
      this.buffer = Buffer.concat([this.buffer, chunk])
      this.drain()
    })
  }

  static async connect(endpoint: string): Promise<FramedClient> {
    const socket = await new Promise<Socket>((resolve, reject) => {
      const candidate = createConnection(endpoint)
      candidate.once("connect", () => resolve(candidate))
      candidate.once("error", reject)
    })
    return new FramedClient(socket)
  }

  request(value: unknown): Promise<any> {
    void this.write(value)
    return this.read() as Promise<any>
  }

  async write(value: unknown): Promise<void> {
    const body = Buffer.from(JSON.stringify(value), "utf8")
    const header = Buffer.allocUnsafe(4)
    header.writeUInt32LE(body.byteLength)
    await new Promise<void>((resolve, reject) =>
      this.socket.write(Buffer.concat([header, body]), (error) => (error ? reject(error) : resolve())),
    )
  }

  read(): Promise<unknown> {
    const value = this.values.shift()
    if (value !== undefined) return Promise.resolve(value)
    return new Promise((resolve) => this.waiters.push(resolve))
  }

  close(): void {
    this.socket.end()
  }

  private drain(): void {
    while (this.buffer.byteLength >= 4) {
      const length = this.buffer.readUInt32LE(0)
      if (this.buffer.byteLength < length + 4) return
      const value = JSON.parse(this.buffer.subarray(4, 4 + length).toString("utf8"))
      this.buffer = this.buffer.subarray(4 + length)
      const waiter = this.waiters.shift()
      if (waiter) waiter(value)
      else this.values.push(value)
    }
  }
}

async function readFirstLine(stream: ReadableStream<Uint8Array>): Promise<string> {
  const reader = stream.getReader()
  const decoder = new TextDecoder()
  let buffer = ""
  for (;;) {
    const { value, done } = await reader.read()
    if (done) throw new Error("sidecar closed before handshake")
    buffer += decoder.decode(value, { stream: true })
    const newline = buffer.indexOf("\n")
    if (newline >= 0) {
      reader.releaseLock()
      return buffer.slice(0, newline).trim()
    }
  }
}
