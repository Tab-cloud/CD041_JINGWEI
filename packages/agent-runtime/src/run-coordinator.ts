interface Lane<T> {
  active: Promise<T>
  controller: AbortController
  pendingWake: (() => Promise<T>) | undefined
}

export class SessionRunCoordinator<T> {
  private readonly lanes = new Map<string, Lane<T>>()

  resume(sessionId: string, continuation: (signal: AbortSignal) => Promise<T>): Promise<T> {
    const lane = this.lanes.get(sessionId)
    if (lane) return lane.active
    return this.start(sessionId, continuation)
  }

  wake(sessionId: string, continuation: (signal: AbortSignal) => Promise<T>): Promise<T> {
    const lane = this.lanes.get(sessionId)
    if (!lane) return this.start(sessionId, continuation)
    lane.pendingWake = () => continuation(lane.controller.signal)
    return lane.active
  }

  interrupt(sessionId: string, reason = "Run interrupted"): boolean {
    const lane = this.lanes.get(sessionId)
    if (!lane) return false
    lane.controller.abort(new DOMException(reason, "AbortError"))
    return true
  }

  isActive(sessionId: string): boolean {
    return this.lanes.has(sessionId)
  }

  private start(sessionId: string, continuation: (signal: AbortSignal) => Promise<T>): Promise<T> {
    const controller = new AbortController()
    const lane = {} as Lane<T>
    lane.controller = controller
    lane.pendingWake = undefined
    lane.active = this.drain(sessionId, lane, continuation)
    this.lanes.set(sessionId, lane)
    return lane.active
  }

  private async drain(
    sessionId: string,
    lane: Lane<T>,
    first: (signal: AbortSignal) => Promise<T>,
  ): Promise<T> {
    let continuation: (() => Promise<T>) | undefined = () => first(lane.controller.signal)
    let result: T | undefined
    try {
      while (continuation) {
        result = await continuation()
        continuation = lane.pendingWake
        lane.pendingWake = undefined
      }
      return result as T
    } finally {
      if (this.lanes.get(sessionId) === lane) this.lanes.delete(sessionId)
    }
  }
}
