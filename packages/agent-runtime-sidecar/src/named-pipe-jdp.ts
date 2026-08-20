import { createConnection, type Socket } from "node:net"
import type {
  DurableEvent,
  ExecutionReconciliation,
  JdpCapabilities,
  JdpFailure,
  JdpRequest,
  JdpSuccess,
} from "@jingwei/agent-protocol"
import { type GeneratedJdpBinding, JdpCommittedWithoutResponseError } from "@jingwei/agent-runtime"

interface BootstrapOptions {
  pipeName: string
  sessionToken: string
  workspaceId: string
  projectId: string
}

interface WorkspaceCapabilities {
  protocolVersions: string[]
  operations: Record<string, "available" | "capability-unavailable">
}

export class NamedPipeJdpBinding implements GeneratedJdpBinding {
  private requestSequence = 0
  private serial: Promise<unknown> = Promise.resolve()

  private constructor(
    private readonly socket: Socket,
    private readonly frames: FrameReader,
    private readonly options: BootstrapOptions,
  ) {}

  static async connect(options: BootstrapOptions): Promise<NamedPipeJdpBinding> {
    const { socket, frames } = await authenticatedConnection(options)
    return new NamedPipeJdpBinding(socket, frames, options)
  }

  async capabilities(): Promise<JdpCapabilities> {
    const result = await this.invoke<Record<string, never>, WorkspaceCapabilities>(
      this.request("workspace.capabilities", {}),
    )
    if (!result.success) throw new Error(`${result.error.code}: ${result.error.summary}`)
    return {
      protocolVersion: "0.1",
      operations: Object.entries(result.data.operations)
        .filter(([, state]) => state === "available")
        .map(([operation]) => operation)
        .sort(),
      reconciliation: result.data.operations["execution.reconcile"] === "available",
    }
  }

  invoke<TInput, TOutput>(request: JdpRequest<TInput>): Promise<JdpSuccess<TOutput> | JdpFailure> {
    return this.exclusive(async () => {
      try {
        writeFrame(this.socket, request)
        return (await this.frames.next()) as JdpSuccess<TOutput> | JdpFailure
      } catch (error) {
        if (request.executionId) {
          throw new JdpCommittedWithoutResponseError(request.executionId, request.idempotencyKey)
        }
        throw error
      }
    })
  }

  async *events(aggregateId: string, afterSequence = 0): AsyncIterable<DurableEvent> {
    const result = await this.invoke<{ afterSequence: number; limit: number }, DurableEvent[]>(
      this.request("task.events", { afterSequence, limit: 1_000 }, aggregateId),
    )
    if (!result.success) throw new Error(`${result.error.code}: ${result.error.summary}`)
    yield* result.data
  }

  async cancel(taskId: string): Promise<void> {
    const result = await this.invokeOutOfBand<{ taskId: string }, unknown>(
      this.request("task.cancel", { taskId }),
    )
    if (!result.success) throw new Error(`${result.error.code}: ${result.error.summary}`)
  }

  async reconcile(executionId: string, idempotencyKey: string): Promise<ExecutionReconciliation> {
    const result = await this.invokeOutOfBand<
      { executionId: string; idempotencyKey: string },
      ExecutionReconciliation
    >(this.request("execution.reconcile", { executionId, idempotencyKey }))
    if (!result.success) {
      return { executionId, idempotencyKey, status: "unknown" }
    }
    return result.data
  }

  close(): void {
    this.socket.end()
  }

  private request<TInput>(
    operation: string,
    input: TInput,
    projectId = this.options.projectId,
  ): JdpRequest<TInput> {
    this.requestSequence += 1
    const suffix = `${process.pid}-${this.requestSequence}-${crypto.randomUUID()}`
    return {
      protocolVersion: "0.1",
      requestId: `request-runtime-${suffix}`,
      idempotencyKey: `idem-runtime-${suffix}`,
      workspaceId: this.options.workspaceId,
      projectId,
      operation,
      deadlineMs: 30_000,
      input,
    }
  }

  private exclusive<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.serial.then(operation, operation)
    this.serial = result.then(
      () => undefined,
      () => undefined,
    )
    return result
  }

  private async invokeOutOfBand<TInput, TOutput>(
    request: JdpRequest<TInput>,
  ): Promise<JdpSuccess<TOutput> | JdpFailure> {
    const { socket, frames } = await authenticatedConnection(this.options)
    try {
      writeFrame(socket, request)
      return (await frames.next()) as JdpSuccess<TOutput> | JdpFailure
    } finally {
      socket.end()
    }
  }
}

class FrameReader {
  private buffer = Buffer.alloc(0)
  private readonly values: unknown[] = []
  private readonly waiters: Array<{
    resolve(value: unknown): void
    reject(error: Error): void
  }> = []
  private failure?: Error

  constructor(socket: Socket) {
    socket.on("data", (chunk: Buffer) => {
      this.buffer = Buffer.concat([this.buffer, chunk])
      this.drain()
    })
    socket.on("error", (error) => this.fail(error))
    socket.on("close", () => this.fail(new Error("Rust daemon named pipe closed")))
  }

  next(): Promise<unknown> {
    if (this.failure) return Promise.reject(this.failure)
    const value = this.values.shift()
    if (value !== undefined) return Promise.resolve(value)
    return new Promise((resolve, reject) => this.waiters.push({ resolve, reject }))
  }

  private drain(): void {
    while (this.buffer.byteLength >= 4) {
      const length = this.buffer.readUInt32LE(0)
      if (length === 0 || length > 4 * 1024 * 1024) {
        this.fail(new Error(`Invalid JDP frame length: ${length}`))
        return
      }
      if (this.buffer.byteLength < 4 + length) return
      const value = JSON.parse(this.buffer.subarray(4, 4 + length).toString("utf8")) as unknown
      this.buffer = this.buffer.subarray(4 + length)
      const waiter = this.waiters.shift()
      if (waiter) waiter.resolve(value)
      else this.values.push(value)
    }
  }

  private fail(error: Error): void {
    this.failure = error
    for (const waiter of this.waiters.splice(0)) waiter.reject(error)
  }
}

async function authenticatedConnection(
  options: BootstrapOptions,
): Promise<{ socket: Socket; frames: FrameReader }> {
  const socket = await connectWithRetry(options.pipeName)
  const frames = new FrameReader(socket)
  writeFrame(socket, {
    protocolVersions: ["0.1"],
    workspaceId: options.workspaceId,
    clientName: "jingwei-agent-runtime-sidecar",
    clientVersion: "0.1.0",
    sessionToken: options.sessionToken,
  })
  const handshakeEnvelope = (await frames.next()) as {
    Ok?: { protocolVersion?: string }
    protocolVersion?: string
  }
  const handshake = handshakeEnvelope.Ok ?? handshakeEnvelope
  if (handshake.protocolVersion !== "0.1") {
    socket.destroy()
    throw new Error("Rust daemon JDP handshake rejected the runtime sidecar")
  }
  return { socket, frames }
}

function writeFrame(socket: Socket, value: unknown): void {
  const body = Buffer.from(JSON.stringify(value), "utf8")
  if (body.byteLength > 4 * 1024 * 1024) throw new Error("JDP request exceeds frame limit")
  const header = Buffer.allocUnsafe(4)
  header.writeUInt32LE(body.byteLength)
  socket.write(Buffer.concat([header, body]))
}

async function connectWithRetry(pipeName: string): Promise<Socket> {
  let lastError: unknown
  for (let attempt = 0; attempt < 300; attempt += 1) {
    try {
      return await new Promise<Socket>((resolve, reject) => {
        const socket = createConnection(pipeName)
        socket.once("connect", () => resolve(socket))
        socket.once("error", reject)
      })
    } catch (error) {
      lastError = error
      await Bun.sleep(100)
    }
  }
  throw new Error(`Rust daemon named pipe unavailable: ${String(lastError)}`)
}
