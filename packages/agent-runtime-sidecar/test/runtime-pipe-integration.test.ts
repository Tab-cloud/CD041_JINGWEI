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
  test("creates a durable session and reconciles a committed draft after the response is lost", async () => {
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
    const providerTestSecret = "runtime-pipe-secret-must-not-return"
    const providerTest = await runtimeRpc.request({
      id: "runtime-provider-test",
      token: runtimeToken,
      method: "provider.test",
      params: {
        provider: "openai-responses",
        baseUrl: "http://unsafe.example/v1",
        model: "test-model",
        apiKey: providerTestSecret,
        timeoutMs: 10_000,
      },
    })
    expect(providerTest).toMatchObject({
      success: true,
      result: { status: "failed", error: { code: "validation-failed" } },
    })
    expect(JSON.stringify(providerTest)).not.toContain(providerTestSecret)
    const configured = await runtimeRpc.request({
      id: "runtime-provider-configure",
      token: runtimeToken,
      method: "provider.configure",
      params: {
        provider: "openai-chat-completions",
        baseUrl: "https://compatible.example/v1",
        model: "chat-model-example",
        apiKey: providerTestSecret,
        timeoutMs: 10_000,
      },
    })
    expect(configured).toMatchObject({
      success: true,
      result: {
        configured: true,
        providers: [{ provider: "openai-chat-completions", model: "chat-model-example" }],
      },
    })
    expect(JSON.stringify(configured)).not.toContain(providerTestSecret)
    const capabilitiesAfterConfigure = await runtimeRpc.request({
      id: "runtime-capabilities-configured",
      token: runtimeToken,
      method: "capabilities",
      params: {},
    })
    expect(capabilitiesAfterConfigure).toMatchObject({
      success: true,
      result: { providers: [{ provider: "openai-chat-completions", model: "chat-model-example" }] },
    })
    const rejectedReplacement = await runtimeRpc.request({
      id: "runtime-provider-invalid-replacement",
      token: runtimeToken,
      method: "provider.configure",
      params: {
        provider: "openai-chat-completions",
        baseUrl: "http://unsafe.example/v1",
        model: "replacement-model",
        apiKey: providerTestSecret,
        timeoutMs: 10_000,
      },
    })
    expect(rejectedReplacement).toMatchObject({ success: false })
    expect(JSON.stringify(rejectedReplacement)).not.toContain(providerTestSecret)
    const capabilitiesAfterRejectedReplacement = await runtimeRpc.request({
      id: "runtime-capabilities-atomic",
      token: runtimeToken,
      method: "capabilities",
      params: {},
    })
    expect(capabilitiesAfterRejectedReplacement).toMatchObject({
      success: true,
      result: { providers: [{ provider: "openai-chat-completions", model: "chat-model-example" }] },
    })
    const cleared = await runtimeRpc.request({
      id: "runtime-provider-clear",
      token: runtimeToken,
      method: "provider.clear",
      params: {},
    })
    expect(cleared).toMatchObject({ success: true, result: { configured: false, providers: [] } })
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
    const executionId = `execution-lost-response-${crypto.randomUUID()}`
    const idempotencyKey = `idem-lost-response-${crypto.randomUUID()}`
    const commitRequest = {
      protocolVersion: "0.1",
      requestId: `request-lost-response-${crypto.randomUUID()}`,
      idempotencyKey,
      workspaceId: "ws-desktop",
      projectId: "project-desktop",
      operation: "edit_command.commit",
      executionId,
      deadlineMs: 10_000,
      dryRun: false,
      input: { ...group, previewHash: preview.data.previewHash },
    }
    await daemonClient.write(commitRequest)
    daemonClient.destroy()

    const recoveryClient = await connectedDaemonClient(daemonPipeName, daemonToken)
    const reconciled = await waitForReconciliation(recoveryClient, executionId, idempotencyKey)
    expect(reconciled.data.status).toBe("succeeded")
    expect(reconciled.data.responseHash).toMatch(/^[0-9a-f]{64}$/)
    expect(reconciled.data.result.executionId).toBe(executionId)
    expect(reconciled.data.result.data.draftRevision).not.toBe(head)

    const replayed = await recoveryClient.request(commitRequest)
    expect(replayed).toEqual(reconciled.data.result)
    const project = await jdp(recoveryClient, "project.get", undefined, {})
    expect(project.data.currentRevision).toBe(head)

    recoveryClient.close()
    runtimeRpc.close()
  }, 30_000)
})

async function connectedDaemonClient(pipeName: string, token: string): Promise<FramedClient> {
  const client = await FramedClient.connect(pipeName)
  await client.write({
    protocolVersions: ["0.1"],
    workspaceId: "ws-desktop",
    clientName: "integration-recovery-client",
    clientVersion: "0.1.0",
    sessionToken: token,
  })
  expect(await client.read()).toMatchObject({ Ok: { protocolVersion: "0.1" } })
  return client
}

async function waitForReconciliation(
  client: FramedClient,
  executionId: string,
  idempotencyKey: string,
): Promise<any> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const response = await jdp(client, "execution.reconcile", undefined, {
      executionId,
      idempotencyKey,
    })
    if (response.success && response.data.status !== "not-found") return response
    await Bun.sleep(20)
  }
  throw new Error("committed execution did not become reconcilable")
}

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

  destroy(): void {
    this.socket.destroy()
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
