import type { DurableEventBus } from "@jingwei/agent-events"
import { RecordingRuntimeTracer, type RuntimeTracer, redactRecord } from "@jingwei/agent-observability"
import type { ApprovalReply, ApprovalScope, ApprovalView, Effect } from "@jingwei/agent-protocol"
import type { ApprovalRepository } from "./approval-store.ts"
import { resourceMatches } from "./policy.ts"

export interface ApprovalRequestInput {
  sessionId: string
  runId: string
  toolCallId: string
  toolName: string
  permission: string
  effects: Effect[]
  resourceClaims: string[]
  input: Record<string, unknown>
  reason: string
  timeoutMs?: number
}

export class ApprovalService {
  private readonly waiters = new Map<string, Set<(approval: ApprovalView) => void>>()
  private readonly grants: ApprovalScope[] = []
  private readonly tracer: RuntimeTracer

  constructor(
    private readonly repository: ApprovalRepository,
    private readonly events: DurableEventBus,
    private readonly defaultTimeoutMs = 5 * 60_000,
    tracer?: RuntimeTracer,
  ) {
    this.tracer = tracer ?? new RecordingRuntimeTracer()
  }

  request(input: ApprovalRequestInput): ApprovalView {
    const createdAt = new Date()
    const expiresAt = new Date(createdAt.getTime() + (input.timeoutMs ?? this.defaultTimeoutMs))
    const approval: ApprovalView = {
      id: crypto.randomUUID(),
      sessionId: input.sessionId,
      runId: input.runId,
      toolCallId: input.toolCallId,
      toolName: input.toolName,
      permission: input.permission,
      effects: [...input.effects],
      resourceClaims: [...input.resourceClaims],
      redactedInput: redactRecord(input.input),
      reason: input.reason,
      scope: {
        sessionId: input.sessionId,
        permission: input.permission,
        resourceClaims: [...input.resourceClaims],
        expiresAt: expiresAt.toISOString(),
      },
      status: "pending",
      createdAt: createdAt.toISOString(),
      expiresAt: expiresAt.toISOString(),
    }

    this.repository.create(approval)
    this.events.append({
      aggregateId: input.sessionId,
      sessionId: input.sessionId,
      runId: input.runId,
      type: "approval.requested",
      data: approval,
    })
    return approval
  }

  async wait(approvalId: string, signal?: AbortSignal): Promise<ApprovalView> {
    const existing = this.repository.get(approvalId)
    if (!existing) throw new Error(`Approval not found: ${approvalId}`)
    if (existing.status !== "pending") return existing
    if (new Date(existing.expiresAt).getTime() <= Date.now()) return this.expire(approvalId)

    const span = this.tracer.startSpan("approval.wait", {
      sessionId: existing.sessionId,
      runId: existing.runId,
      approvalId: existing.id,
      toolName: existing.toolName,
    })
    return new Promise((resolve, reject) => {
      const settle = (approval: ApprovalView) => {
        signal?.removeEventListener("abort", abort)
        span.end(approval.status === "approved" ? "ok" : "error", approval.status)
        resolve(approval)
      }
      const abort = () => {
        this.waiters.get(approvalId)?.delete(settle)
        span.end("error", "cancelled")
        reject(new DOMException("Approval wait cancelled", "AbortError"))
      }
      const waiters = this.waiters.get(approvalId) ?? new Set()
      waiters.add(settle)
      this.waiters.set(approvalId, waiters)
      signal?.addEventListener("abort", abort, { once: true })
      if (signal?.aborted) abort()
    })
  }

  reply(input: ApprovalReply): ApprovalView {
    const current = this.repository.get(input.approvalId)
    if (!current) throw new Error(`Approval not found: ${input.approvalId}`)
    if (new Date(current.expiresAt).getTime() <= Date.now() && current.status === "pending") {
      return this.expire(current.id)
    }

    const status = input.decision === "deny" ? "denied" : "approved"
    const resolved = this.repository.resolve(current.id, status, new Date().toISOString())
    if (input.decision === "allow-scope" && status === "approved") {
      const requestedExpiry = input.scopeExpiresAt
        ? new Date(input.scopeExpiresAt).getTime()
        : Number.POSITIVE_INFINITY
      const approvalExpiry = new Date(current.expiresAt).getTime()
      this.grants.push({
        ...current.scope,
        expiresAt: new Date(Math.min(requestedExpiry, approvalExpiry)).toISOString(),
      })
    }
    this.publishResolution(resolved)
    return resolved
  }

  list(sessionId?: string): ApprovalView[] {
    this.expirePending()
    return this.repository.list(sessionId)
  }

  hasGrant(sessionId: string, permission: string, resourceClaims: string[]): boolean {
    const now = Date.now()
    return this.grants.some((grant) => {
      if (grant.sessionId !== sessionId || grant.permission !== permission) return false
      if (grant.expiresAt && new Date(grant.expiresAt).getTime() <= now) return false
      return resourceClaims.every((claim) =>
        grant.resourceClaims.some((allowed) => resourceMatches(allowed, claim)),
      )
    })
  }

  private expirePending(): void {
    for (const approval of this.repository.list()) {
      if (approval.status === "pending" && new Date(approval.expiresAt).getTime() <= Date.now())
        this.expire(approval.id)
    }
  }

  private expire(id: string): ApprovalView {
    const approval = this.repository.resolve(id, "expired", new Date().toISOString())
    this.publishResolution(approval)
    return approval
  }

  private publishResolution(approval: ApprovalView): void {
    this.events.append({
      aggregateId: approval.sessionId,
      sessionId: approval.sessionId,
      runId: approval.runId,
      type: `approval.${approval.status}`,
      data: { approvalId: approval.id, status: approval.status, decidedAt: approval.decidedAt },
    })
    for (const resolve of this.waiters.get(approval.id) ?? []) resolve(approval)
    this.waiters.delete(approval.id)
  }
}
