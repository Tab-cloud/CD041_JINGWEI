import type { ProviderMessage } from "./provider.ts"

export interface ContextProjection {
  messages: ProviderMessage[]
  omittedMessages: number
  artifactRefs: string[]
}

export class ContextProjector {
  constructor(private readonly maxCharacters = 80_000) {}

  project(messages: ProviderMessage[]): ContextProjection {
    const mustKeep = new Set<number>()
    if (messages.length > 0) mustKeep.add(0)
    for (let index = Math.max(0, messages.length - 12); index < messages.length; index++) mustKeep.add(index)
    messages.forEach((message, index) => {
      if (
        /\b(?:requirement|revision|approval|decision|finding|artifact):[A-Za-z0-9._:-]+\b/i.test(
          message.content,
        )
      ) {
        mustKeep.add(index)
      }
    })

    let used = 0
    const selected: ProviderMessage[] = []
    for (let index = messages.length - 1; index >= 0; index--) {
      const message = messages[index]
      if (!message) continue
      const length = message.content.length
      if (!mustKeep.has(index) && used + length > this.maxCharacters) continue
      selected.push(message)
      used += length
    }
    selected.reverse()
    const artifactRefs = [...new Set(selected.flatMap((message) => extractArtifactRefs(message.content)))]
    return { messages: selected, omittedMessages: messages.length - selected.length, artifactRefs }
  }
}

function extractArtifactRefs(content: string): string[] {
  return [...content.matchAll(/\bartifact:([A-Za-z0-9._:-]+)\b/g)].map((match) => match[1] as string)
}
