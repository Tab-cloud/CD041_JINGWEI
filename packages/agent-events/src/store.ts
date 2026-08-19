import type { DurableEvent } from "@jingwei/agent-protocol"

export interface EventDraft<T = unknown> {
  aggregateId: string
  sessionId?: string
  runId?: string
  taskId?: string
  traceId?: string
  type: string
  data: T
  timestamp?: string
}

export interface DurableEventStore {
  append<T>(draft: EventDraft<T>): DurableEvent<T>
  readAfter(aggregateId: string, sequence?: number, limit?: number): DurableEvent[]
  latestSequence(aggregateId: string): number
}

export class MemoryEventStore implements DurableEventStore {
  private readonly eventsByAggregate = new Map<string, DurableEvent[]>()

  append<T>(draft: EventDraft<T>): DurableEvent<T> {
    const events = this.eventsByAggregate.get(draft.aggregateId) ?? []
    const event: DurableEvent<T> = {
      schemaVersion: "0.1",
      aggregateId: draft.aggregateId,
      sequence: (events.at(-1)?.sequence ?? 0) + 1,
      timestamp: draft.timestamp ?? new Date().toISOString(),
      ...(draft.sessionId ? { sessionId: draft.sessionId } : {}),
      ...(draft.runId ? { runId: draft.runId } : {}),
      ...(draft.taskId ? { taskId: draft.taskId } : {}),
      ...(draft.traceId ? { traceId: draft.traceId } : {}),
      type: draft.type,
      data: structuredClone(draft.data),
    }
    events.push(event)
    this.eventsByAggregate.set(draft.aggregateId, events)
    return structuredClone(event)
  }

  readAfter(aggregateId: string, sequence = 0, limit = 1_000): DurableEvent[] {
    return (this.eventsByAggregate.get(aggregateId) ?? [])
      .filter((event) => event.sequence > sequence)
      .slice(0, limit)
      .map((event) => structuredClone(event))
  }

  latestSequence(aggregateId: string): number {
    return this.eventsByAggregate.get(aggregateId)?.at(-1)?.sequence ?? 0
  }
}
