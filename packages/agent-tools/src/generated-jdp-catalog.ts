import type { RuntimeTracer } from "@jingwei/agent-observability"
import type { Effect, JdpClient, JsonSchema } from "@jingwei/agent-protocol"
import { registerGeneratedJdpTool } from "./jdp-adapter.ts"
import type { ToolRegistry } from "./registry.ts"
import type { MaterializedTool } from "./types.ts"

export interface RustGeneratedJdpTool {
  name: string
  description: string
  inputSchema: JsonSchema
  outputSchema: JsonSchema
  permission:
    | "read"
    | "candidate-write"
    | "review-decision"
    | "external-process"
    | "waiver"
    | "release-candidate"
    | "production-release"
  effect: "read-only" | "revision-write" | "draft-write" | "task-control" | "artifact-read" | "artifact-write"
  resourceClaimTemplate: string[]
  idempotent: boolean
  requiresBaseRevision: boolean
  cancellable: boolean
  defaultTimeoutMs: number
  protocolVersion: string
  schemaVersion: string
}

/** Registers the Rust-generated JDP tool catalog through the canonical settlement registry. */
export function registerRustGeneratedJdpCatalog(
  registry: ToolRegistry,
  client: JdpClient,
  catalog: RustGeneratedJdpTool[],
  tracer?: RuntimeTracer,
): MaterializedTool[] {
  return catalog.map((tool) => {
    assertCompatible(tool)
    return registerGeneratedJdpTool(
      registry,
      client,
      {
        name: tool.name,
        description: tool.description,
        operation: tool.name,
        inputSchema: tool.inputSchema,
        outputSchema: tool.outputSchema,
        permission: `jdp:${tool.permission}`,
        effects: effectsFor(tool.effect),
        resourceClaims: [...tool.resourceClaimTemplate],
        write:
          tool.effect === "revision-write" ||
          tool.effect === "draft-write" ||
          tool.effect === "artifact-write",
        requiresBaseRevision: tool.requiresBaseRevision,
        cancellable: tool.cancellable,
        timeoutMs: tool.defaultTimeoutMs,
      },
      tracer,
    )
  })
}

function assertCompatible(tool: RustGeneratedJdpTool): void {
  if (tool.protocolVersion !== "0.1" || tool.schemaVersion !== "0.2") {
    throw new Error(
      `Unsupported generated JDP catalog version for ${tool.name}: ${tool.protocolVersion}/${tool.schemaVersion}`,
    )
  }
  if (!tool.idempotent) throw new Error(`JDP tool must be idempotent: ${tool.name}`)
  if (!Number.isSafeInteger(tool.defaultTimeoutMs) || tool.defaultTimeoutMs <= 0) {
    throw new Error(`Invalid generated JDP timeout: ${tool.name}`)
  }
}

function effectsFor(effect: RustGeneratedJdpTool["effect"]): Effect[] {
  if (effect === "read-only" || effect === "artifact-read") return ["read"]
  if (effect === "artifact-write") return ["workspace-write"]
  if (effect === "revision-write" || effect === "draft-write" || effect === "task-control") {
    return ["design-write"]
  }
  const exhaustive: never = effect
  throw new Error(`Unsupported generated JDP effect: ${exhaustive}`)
}
