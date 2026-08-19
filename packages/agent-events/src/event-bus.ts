import type { DurableEvent } from "@jingwei/agent-protocol"
import type { DurableEventStore, EventDraft } from "./store.ts"

export class DurableEventBus {
  private readonly versions = new Map<string, number>()
  private readonly waiters = new Map<string, Set<() => void>>()

  constructor(
    readonly store: DurableEventStore,
    private readonly externalPollMs = 100,
  ) {}

  append<T>(draft: EventDraft<T>): DurableEvent<T> {
    const event = this.store.append(draft)
    this.versions.set(draft.aggregateId, (this.versions.get(draft.aggregateId) ?? 0) + 1)
    for (const wake of this.waiters.get(draft.aggregateId) ?? []) wake()
    this.waiters.delete(draft.aggregateId)
    return event
  }

  readAfter(aggregateId: string, sequence = 0, limit = 1_000): DurableEvent[] {
    return this.store.readAfter(aggregateId, sequence, limit)
  }

  async *events(aggregateId: string, afterSequence = 0, signal?: AbortSignal): AsyncIterable<DurableEvent> {
    let cursor = afterSequence
    while (!signal?.aborted) {
      const history = this.store.readAfter(aggregateId, cursor)
      if (history.length > 0) {
        for (const event of history) {
          if (event.sequence <= cursor) continue
          cursor = event.sequence
          yield event
        }
        continue
      }

      const version = this.versions.get(aggregateId) ?? 0
      const raced = this.store.readAfter(aggregateId, cursor)
      if (raced.length > 0) continue
      await this.waitForChange(aggregateId, version, signal)
    }
  }

  private waitForChange(aggregateId: string, version: number, signal?: AbortSignal): Promise<void> {
    if ((this.versions.get(aggregateId) ?? 0) !== version) return Promise.resolve()
    return new Promise((resolve) => {
      let settled = false
      const finish = () => {
        if (settled) return
        settled = true
        clearTimeout(timer)
        signal?.removeEventListener("abort", finish)
        this.waiters.get(aggregateId)?.delete(finish)
        resolve()
      }
      const timer = setTimeout(finish, this.externalPollMs)
      const waiters = this.waiters.get(aggregateId) ?? new Set()
      waiters.add(finish)
      this.waiters.set(aggregateId, waiters)
      signal?.addEventListener("abort", finish, { once: true })
      if (signal?.aborted) finish()
    })
  }
}
