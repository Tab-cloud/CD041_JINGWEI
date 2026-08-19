import type { DurableEventBus } from "@jingwei/agent-events"
import type { ApprovalService } from "@jingwei/agent-policy"
import type {
  AdmitInput,
  AdmittedInput,
  ApprovalReply,
  ApprovalView,
  ArtifactView,
  CreateSessionInput,
  DurableEvent,
  RuntimeCapabilities,
  RuntimeClient,
  RunView,
  SessionView,
} from "@jingwei/agent-protocol"
import { RuntimeNotFoundError } from "@jingwei/agent-protocol"
import type { ArtifactStore } from "@jingwei/agent-tools"
import { type AgentLoop, runtimeFailure } from "./agent-loop.ts"
import type { ProviderRegistry } from "./provider-registry.ts"
import type { SessionRunCoordinator } from "./run-coordinator.ts"
import type { RuntimeRepository } from "./runtime-repository.ts"
import type { SessionService } from "./session-service.ts"

export interface RuntimeServiceDependencies {
  repository: RuntimeRepository
  events: DurableEventBus
  sessions: SessionService
  coordinator: SessionRunCoordinator<RunView>
  loop: AgentLoop
  approvals: ApprovalService
  artifacts: ArtifactStore
  providers: ProviderRegistry
  jdpCapabilities: RuntimeCapabilities["jdp"]
}

export class AgentRuntimeService implements RuntimeClient {
  constructor(private readonly dependencies: RuntimeServiceDependencies) {}

  async createSession(input: CreateSessionInput): Promise<SessionView> {
    return this.dependencies.sessions.create(input)
  }

  async admitInput(input: AdmitInput): Promise<AdmittedInput> {
    return this.dependencies.sessions.admit(input)
  }

  async resumeRun(sessionId: string): Promise<RunView> {
    if (!this.dependencies.sessions.get(sessionId)) throw new RuntimeNotFoundError("session", sessionId)
    return this.dependencies.coordinator.resume(sessionId, (signal) => this.drainSession(sessionId, signal))
  }

  async cancelRun(runId: string, reason = "Cancelled by user"): Promise<void> {
    const run = this.dependencies.repository.getRun(runId)
    if (!run) throw new RuntimeNotFoundError("run", runId)
    this.dependencies.coordinator.interrupt(run.sessionId, reason)
    this.dependencies.repository.updateRun(runId, {
      status: "cancelled",
      error: runtimeFailure(new DOMException(reason, "AbortError"), "run"),
    })
    this.dependencies.events.append({
      aggregateId: run.sessionId,
      sessionId: run.sessionId,
      runId,
      type: "run.cancelled",
      data: { reason },
    })
  }

  events(sessionId: string, afterSequence = 0): AsyncIterable<DurableEvent> {
    return this.dependencies.events.events(sessionId, afterSequence)
  }

  async listApprovals(sessionId?: string): Promise<ApprovalView[]> {
    return this.dependencies.approvals.list(sessionId)
  }

  async replyApproval(input: ApprovalReply): Promise<void> {
    this.dependencies.approvals.reply(input)
  }

  async getArtifact(artifactId: string): Promise<ArtifactView> {
    const artifact = this.dependencies.artifacts.get(artifactId)
    if (!artifact) throw new RuntimeNotFoundError("artifact", artifactId)
    return artifact
  }

  async capabilities(): Promise<RuntimeCapabilities> {
    return {
      protocolVersion: "0.1",
      deliveries: ["steer", "queue"],
      durableEvents: true,
      approvalScopes: true,
      jdp: structuredClone(this.dependencies.jdpCapabilities),
      providers: this.dependencies.providers.capabilities(),
    }
  }

  private async drainSession(sessionId: string, signal: AbortSignal): Promise<RunView> {
    let lastRun: RunView | undefined
    while (!signal.aborted) {
      const pending = this.dependencies.repository.listPendingInputs(sessionId)
      const recoverable = this.dependencies.repository.findActiveRun(sessionId)
      const recoveredInputs = recoverable ? this.dependencies.repository.listInputsForRun(recoverable.id) : []
      const first = recoveredInputs[0] ?? pending[0]
      if (!first) break
      const run = recoverable ?? this.dependencies.repository.createRun(sessionId, "root", "main-agent")
      if (!first.runId) this.dependencies.repository.consumeInput(first.id, run.id)
      lastRun = this.dependencies.repository.updateRun(run.id, { status: "running" })
      this.emitRun(lastRun, "run.started")
      try {
        const session = this.dependencies.sessions.get(sessionId) as SessionView
        const result = await this.dependencies.loop.run({
          sessionId,
          runId: run.id,
          projectId: session.projectId,
          workspaceId: session.workspaceId,
          agentId: "main-agent",
          agentRole: "primary",
          runKind: "root",
          initialContent: first.content,
          loadSteer: async () => this.consumeSteer(sessionId, run.id),
          signal,
        })
        lastRun = this.dependencies.repository.updateRun(run.id, {
          status: "completed",
          output: result.output,
        })
        this.emitRun(lastRun, "run.completed")
      } catch (error) {
        const failure = runtimeFailure(error, "agent-loop")
        const status = signal.aborted ? "cancelled" : "failed"
        lastRun = this.dependencies.repository.updateRun(run.id, { status, error: failure })
        this.emitRun(lastRun, `run.${status}`)
        if (status === "cancelled") break
      }
    }
    if (!lastRun) {
      const active = this.dependencies.repository.findActiveRun(sessionId)
      if (active) return active
      throw new Error("No admitted input is ready to run")
    }
    return lastRun
  }

  private async consumeSteer(sessionId: string, runId: string): Promise<string[]> {
    const steer = this.dependencies.repository
      .listPendingInputs(sessionId)
      .filter((input) => input.delivery === "steer")
    return steer.map((input) => this.dependencies.repository.consumeInput(input.id, runId).content)
  }

  private emitRun(run: RunView, type: string): void {
    this.dependencies.events.append({
      aggregateId: run.sessionId,
      sessionId: run.sessionId,
      runId: run.id,
      type,
      data: {
        runId: run.id,
        status: run.status,
        kind: run.kind,
        ownerAgentId: run.ownerAgentId,
        error: run.error,
      },
    })
  }
}
