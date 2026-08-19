import type { DurableEventBus } from "@jingwei/agent-events"
import type { ApprovalService } from "@jingwei/agent-policy"
import type { ToolExecutionRepository, ToolExecutionView } from "@jingwei/agent-tools"
import type { RuntimeRepository } from "./runtime-repository.ts"

export interface RecoveryAction {
  kind:
    | "model-attempt-discarded"
    | "run-requeued"
    | "tool-safe-to-resume"
    | "approval-restored"
    | "tool-reconciled"
    | "tool-quarantined"
  entityId: string
  detail: string
}

export interface RecoveryDependencies {
  repository: RuntimeRepository
  executions: ToolExecutionRepository
  approvals: ApprovalService
  events: DurableEventBus
  reconcile?: (execution: ToolExecutionView) => Promise<"succeeded" | "failed" | "running" | "unknown">
  resumeRecorded?: (execution: ToolExecutionView) => Promise<void>
}

export class RecoveryService {
  readonly processEpoch = crypto.randomUUID()

  constructor(
    private readonly dependencies: RecoveryDependencies,
    private readonly leaseTtlMs = 15_000,
  ) {}

  async recover(): Promise<RecoveryAction[]> {
    if (!this.dependencies.repository.acquireOwnerLease(this.processEpoch, this.leaseTtlMs)) {
      throw new Error("Another live runtime process owns the recovery lease")
    }
    const actions: RecoveryAction[] = []
    this.dependencies.events.append({
      aggregateId: "runtime:recovery",
      type: "recovery.started",
      data: { processEpoch: this.processEpoch },
    })

    for (const attempt of this.dependencies.repository.listIncompleteAttempts()) {
      this.dependencies.repository.updateModelAttempt(attempt.id, {
        status: "discarded",
        error: "Process restarted before provider finish; partial stream was discarded",
      })
      actions.push({
        kind: "model-attempt-discarded",
        entityId: attempt.id,
        detail: "Partial provider output was not committed",
      })
    }

    for (const run of this.dependencies.repository.listRecoverableRuns()) {
      this.dependencies.repository.updateRun(run.id, { status: "queued" })
      actions.push({ kind: "run-requeued", entityId: run.id, detail: `Recovered from ${run.status}` })
    }

    for (const approval of this.dependencies.approvals.list()) {
      if (approval.status === "pending") {
        actions.push({
          kind: "approval-restored",
          entityId: approval.id,
          detail: "Pending approval remains fail-closed",
        })
      }
    }

    for (const execution of this.dependencies.executions.listUnsettled()) {
      if (["recorded", "authorizing"].includes(execution.status)) {
        if (this.dependencies.resumeRecorded) await this.dependencies.resumeRecorded(execution)
        actions.push({
          kind: "tool-safe-to-resume",
          entityId: execution.id,
          detail: "No handler side effect started; reauthorization is required",
        })
        continue
      }
      if (execution.status === "waiting-approval") continue
      const result = this.dependencies.reconcile ? await this.dependencies.reconcile(execution) : "unknown"
      if (result === "succeeded" || result === "failed") {
        this.dependencies.executions.update(execution.id, { status: result })
        actions.push({ kind: "tool-reconciled", entityId: execution.id, detail: result })
      } else {
        this.dependencies.executions.update(execution.id, {
          status: "unknown",
          error: {
            code: "needs-reconciliation",
            summary: "External side effect could not be proven after process restart",
            retryable: false,
          },
        })
        actions.push({
          kind: "tool-quarantined",
          entityId: execution.id,
          detail: "Unknown external state; blind retry is prohibited",
        })
      }
    }

    this.dependencies.events.append({
      aggregateId: "runtime:recovery",
      type: actions.some((action) => action.kind === "tool-quarantined")
        ? "recovery.quarantined"
        : "recovery.completed",
      data: { processEpoch: this.processEpoch, actions },
    })
    return actions
  }

  heartbeat(): boolean {
    return this.dependencies.repository.heartbeatOwnerLease(this.processEpoch, this.leaseTtlMs)
  }
}
