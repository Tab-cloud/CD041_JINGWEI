import type { ProviderCapabilities } from "@jingwei/agent-protocol"
import type { ProviderAdapter, ProviderChunk, ProviderTurnRequest } from "./provider.ts"

export type FakeProviderStep =
  | ProviderChunk
  | { type: "delay"; milliseconds: number }
  | { type: "error"; error: Error }

export interface FakeProviderScript {
  steps: FakeProviderStep[]
}

export class FakeProvider implements ProviderAdapter {
  readonly calls: ProviderTurnRequest[] = []
  private scriptIndex = 0

  constructor(
    private readonly scripts: FakeProviderScript[],
    readonly capabilities: ProviderCapabilities = fakeCapabilities(),
  ) {}

  async *stream(request: ProviderTurnRequest): AsyncIterable<ProviderChunk> {
    this.calls.push({ ...request, messages: structuredClone(request.messages), tools: [...request.tools] })
    const script = this.scripts[this.scriptIndex++]
    if (!script) throw new Error("FakeProvider script exhausted")
    for (const step of script.steps) {
      if (request.signal.aborted)
        throw request.signal.reason ?? new DOMException("Fake provider cancelled", "AbortError")
      if (step.type === "delay") {
        await Bun.sleep(step.milliseconds)
        continue
      }
      if (step.type === "error") throw step.error
      yield structuredClone(step)
    }
  }
}

export function fakeCapabilities(overrides: Partial<ProviderCapabilities> = {}): ProviderCapabilities {
  return {
    provider: "fake",
    model: "scripted",
    nativeTools: true,
    structuredOutput: true,
    streaming: true,
    reasoning: true,
    image: false,
    pdf: false,
    contextTokens: 32_000,
    outputTokens: 8_000,
    cancellation: true,
    retry: "safe",
    idempotency: true,
    faultDomain: "fake-local",
    dataPolicy: { local: true },
    ...overrides,
  }
}
