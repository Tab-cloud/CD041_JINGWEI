import { type Effect, RuntimeConflictError } from "@jingwei/agent-protocol"

interface Lease {
  executionId: string
  runId: string
  resources: string[]
  write: boolean
}

export class ResourceLeaseManager {
  private readonly leases = new Map<string, Lease>()

  acquire(executionId: string, runId: string, effects: Effect[], resources: string[]): void {
    const write = effects.some((effect) => effect !== "read")
    const conflicts = [...this.leases.values()].filter(
      (lease) =>
        lease.runId !== runId &&
        (write || lease.write) &&
        lease.resources.some((resource) => resources.includes(resource)),
    )
    if (conflicts.length > 0) {
      throw new RuntimeConflictError(
        `Resource claims conflict with active executions: ${conflicts.map((lease) => lease.executionId).join(", ")}`,
        "resource-conflict",
      )
    }
    this.leases.set(executionId, { executionId, runId, resources: [...resources], write })
  }

  release(executionId: string): void {
    this.leases.delete(executionId)
  }

  list(): ReadonlyArray<Lease> {
    return [...this.leases.values()].map((lease) => ({ ...lease, resources: [...lease.resources] }))
  }
}
