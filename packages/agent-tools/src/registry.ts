import { createHash } from "node:crypto"
import type { MaterializedTool, RuntimeToolDescriptor, RuntimeToolHandler } from "./types.ts"

export class ToolRegistry {
  private readonly tools = new Map<string, MaterializedTool>()

  register(descriptor: RuntimeToolDescriptor, handler: RuntimeToolHandler): MaterializedTool {
    validateDescriptor(descriptor)
    const generation = (this.tools.get(descriptor.name)?.generation ?? 0) + 1
    const identity = createHash("sha256")
      .update(canonicalJson({ generation, descriptor: descriptorForIdentity(descriptor) }))
      .digest("hex")
    const materialized = { identity, generation, descriptor: freezeDescriptor(descriptor), handler }
    this.tools.set(descriptor.name, materialized)
    return materialized
  }

  remove(name: string): void {
    this.tools.delete(name)
  }

  get(name: string): MaterializedTool | undefined {
    return this.tools.get(name)
  }

  list(): MaterializedTool[] {
    return [...this.tools.values()]
  }
}

function validateDescriptor(descriptor: RuntimeToolDescriptor): void {
  if (!/^[A-Za-z0-9_.:-]{1,160}$/.test(descriptor.name))
    throw new Error(`Invalid tool name: ${descriptor.name}`)
  if (descriptor.timeoutMs < 1 || !Number.isFinite(descriptor.timeoutMs))
    throw new Error("Tool timeout must be positive")
  if (descriptor.outputBudget.inlineBytes < 64) throw new Error("Tool inline output budget is too small")
  if (descriptor.effects.length === 0) throw new Error("A tool must declare at least one effect")
}

function descriptorForIdentity(descriptor: RuntimeToolDescriptor): Record<string, unknown> {
  return {
    ...descriptor,
    resourceClaims: typeof descriptor.resourceClaims === "function" ? "dynamic" : descriptor.resourceClaims,
  }
}

function freezeDescriptor(descriptor: RuntimeToolDescriptor): RuntimeToolDescriptor {
  return Object.freeze({
    ...descriptor,
    effects: Object.freeze([...descriptor.effects]),
    resourceClaims:
      typeof descriptor.resourceClaims === "function"
        ? descriptor.resourceClaims
        : Object.freeze([...descriptor.resourceClaims]),
    outputBudget: Object.freeze({ ...descriptor.outputBudget }),
  }) as RuntimeToolDescriptor
}

export function canonicalJson(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`
  if (value && typeof value === "object") {
    return `{${Object.entries(value as Record<string, unknown>)
      .filter(([, item]) => item !== undefined)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, item]) => `${JSON.stringify(key)}:${canonicalJson(item)}`)
      .join(",")}}`
  }
  return JSON.stringify(value)
}
