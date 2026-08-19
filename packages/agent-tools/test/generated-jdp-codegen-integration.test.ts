import { afterAll, beforeAll, describe, expect, test } from "bun:test"
import { createHash } from "node:crypto"
import { mkdir, readdir, readFile, rm } from "node:fs/promises"
import { dirname, join, resolve } from "node:path"
import { pathToFileURL } from "node:url"
import type {
  DurableEvent,
  ExecutionReconciliation,
  JdpCapabilities,
  JdpClient,
  JdpFailure,
  JdpRequest,
  JdpSuccess,
} from "@jingwei/agent-protocol"
import { type RustGeneratedJdpTool, registerRustGeneratedJdpCatalog } from "../src/generated-jdp-catalog.ts"
import { ToolRegistry } from "../src/registry.ts"

const workspace = resolve(import.meta.dir, "../../..")
const output = join(workspace, "target", `task01-generated-binding-${crypto.randomUUID()}`)
let catalog: RustGeneratedJdpTool[]

beforeAll(async () => {
  await mkdir(dirname(output), { recursive: true })
  await generate(output)
  catalog = JSON.parse(await readFile(join(output, "tool-catalog.json"), "utf8"))
})

afterAll(async () => {
  await rm(output, { recursive: true, force: true })
})

describe("Rust generated JDP binding integration", () => {
  test("is reproducible and exports the protocol constants consumed by TypeScript", async () => {
    const first = await directoryDigest(output)
    await generate(output)
    const second = await directoryDigest(output)
    expect(second).toBe(first)
    const generated = await import(
      `${pathToFileURL(join(output, "src", "generated.ts")).href}?v=${Date.now()}`
    )
    expect(generated.JDP_PROTOCOL_VERSION).toBe("0.1")
    expect(generated.JIR_SCHEMA_VERSION).toBe("0.2")
    expect(catalog.length).toBeGreaterThanOrEqual(20)
  })

  test("registers and invokes a generated write tool through the production JDP adapter", async () => {
    const registry = new ToolRegistry()
    const client = new RecordingJdpClient()
    const tools = registerRustGeneratedJdpCatalog(registry, client, catalog)
    expect(tools).toHaveLength(catalog.length)
    const apply = registry.get("design.patch.apply")
    expect(apply?.descriptor.permission).toBe("jdp:candidate-write")
    expect(apply?.descriptor.effects).toEqual(["design-write"])

    const result = await apply?.handler(
      { baseRevision: "revision-1", operations: [] },
      {
        executionId: "execution-1",
        sessionId: "session-1",
        runId: "run-1",
        toolCallId: "call-1",
        idempotencyKey: "idempotency-1",
        projectId: "project-1",
        workspaceId: "workspace-1",
        agentId: "main-agent",
        signal: new AbortController().signal,
        traceContext: { traceId: "a".repeat(32), spanId: "b".repeat(16), traceFlags: 1 },
      },
    )
    expect(result).toMatchObject({ success: true, requestId: "call-1" })
    expect(client.requests).toEqual([
      {
        protocolVersion: "0.1",
        requestId: "call-1",
        executionId: "execution-1",
        idempotencyKey: "idempotency-1",
        workspaceId: "workspace-1",
        projectId: "project-1",
        operation: "design.patch.apply",
        deadlineMs: 30_000,
        dryRun: false,
        baseRevision: "revision-1",
        input: { baseRevision: "revision-1", operations: [] },
        traceContext: { traceId: "a".repeat(32), spanId: "b".repeat(16), traceFlags: 1 },
      },
    ])
  })
})

async function generate(directory: string): Promise<void> {
  const process = Bun.spawn(
    ["cargo", "run", "--quiet", "-p", "jingwei-jdp", "--bin", "jingwei-codegen", "--", directory],
    { cwd: workspace, stdout: "pipe", stderr: "pipe" },
  )
  const [exitCode, stderr] = await Promise.all([
    process.exited,
    new Response(process.stderr as ReadableStream<Uint8Array>).text(),
  ])
  if (exitCode !== 0) throw new Error(`Rust JDP codegen failed: ${stderr.slice(-4_000)}`)
}

async function directoryDigest(directory: string): Promise<string> {
  const hash = createHash("sha256")
  const files = await walk(directory)
  for (const file of files.sort()) {
    hash.update(file.slice(directory.length + 1).replaceAll("\\", "/"))
    hash.update(await readFile(file))
  }
  return hash.digest("hex")
}

async function walk(directory: string): Promise<string[]> {
  const output: string[] = []
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) output.push(...(await walk(path)))
    if (entry.isFile()) output.push(path)
  }
  return output
}

class RecordingJdpClient implements JdpClient {
  readonly requests: JdpRequest<unknown>[] = []

  async capabilities(): Promise<JdpCapabilities> {
    return { protocolVersion: "0.1", operations: catalog.map((tool) => tool.name), reconciliation: true }
  }

  async invoke<TInput, TOutput>(request: JdpRequest<TInput>): Promise<JdpSuccess<TOutput> | JdpFailure> {
    this.requests.push(structuredClone(request) as JdpRequest<unknown>)
    return {
      success: true,
      requestId: request.requestId,
      executionId: "execution-1",
      ...(request.baseRevision ? { baseRevision: request.baseRevision, newRevision: "revision-2" } : {}),
      data: { accepted: true } as TOutput,
      artifactRefs: [],
      evidenceRefs: [],
      findings: { errors: 0, warnings: 0, info: 0 },
    }
  }

  async *events(): AsyncIterable<DurableEvent> {}
  async cancel(): Promise<void> {}
  async reconcile(executionId: string, idempotencyKey: string): Promise<ExecutionReconciliation> {
    return { executionId, idempotencyKey, status: "not-found" }
  }
}
