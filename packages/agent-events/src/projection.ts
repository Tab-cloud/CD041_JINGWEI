import type { DurableEvent } from "@jingwei/agent-protocol"

export function rebuildProjection<TState>(
  initial: TState,
  events: Iterable<DurableEvent>,
  reducer: (state: TState, event: DurableEvent) => TState,
): TState {
  let state = initial
  let lastSequence = 0
  for (const event of events) {
    if (event.sequence !== lastSequence + 1) {
      throw new Error(`Event sequence gap: expected ${lastSequence + 1}, received ${event.sequence}`)
    }
    state = reducer(state, event)
    lastSequence = event.sequence
  }
  return state
}
