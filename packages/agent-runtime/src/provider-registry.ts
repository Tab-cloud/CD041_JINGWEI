import type { ProviderCapabilities } from "@jingwei/agent-protocol"
import type { ProviderAdapter } from "./provider.ts"

export interface ProviderRequirements {
  nativeTools?: boolean
  structuredOutput?: boolean
  image?: boolean
  pdf?: boolean
}

export class ProviderRegistry {
  private readonly providers: ProviderAdapter[] = []

  register(provider: ProviderAdapter): void {
    const key = `${provider.capabilities.provider}:${provider.capabilities.model}`
    if (this.providers.some((item) => `${item.capabilities.provider}:${item.capabilities.model}` === key)) {
      throw new Error(`Provider already registered: ${key}`)
    }
    this.providers.push(provider)
  }

  candidates(requirements: ProviderRequirements = {}): ProviderAdapter[] {
    return this.providers.filter((provider) => supports(provider.capabilities, requirements))
  }

  capabilities(): ProviderCapabilities[] {
    return this.providers.map((provider) => structuredClone(provider.capabilities))
  }
}

function supports(capabilities: ProviderCapabilities, requirements: ProviderRequirements): boolean {
  if (requirements.nativeTools && !capabilities.nativeTools) return false
  if (requirements.structuredOutput && !capabilities.structuredOutput) return false
  if (requirements.image && !capabilities.image) return false
  if (requirements.pdf && !capabilities.pdf) return false
  return true
}
