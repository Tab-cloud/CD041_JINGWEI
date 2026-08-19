import { DurableEventBus, MemoryEventStore } from "@jingwei/agent-events"
import type {
  DurableEvent,
  ExecutionReconciliation,
  JdpCapabilities,
  JdpClient,
  JdpFailure,
  JdpRequest,
  JdpSuccess,
} from "@jingwei/agent-protocol"

export interface GeneratedJdpBinding {
  capabilities(): Promise<JdpCapabilities>
  invoke<TInput, TOutput>(request: JdpRequest<TInput>): Promise<JdpSuccess<TOutput> | JdpFailure>
  events(aggregateId: string, afterSequence?: number): AsyncIterable<DurableEvent>
  cancel(taskId: string): Promise<void>
  reconcile(executionId: string, idempotencyKey: string): Promise<ExecutionReconciliation>
}

export class GeneratedBindingJdpClient implements JdpClient {
  constructor(private readonly binding: GeneratedJdpBinding) {}
  capabilities(): Promise<JdpCapabilities> {
    return this.binding.capabilities()
  }
  invoke<TInput, TOutput>(request: JdpRequest<TInput>): Promise<JdpSuccess<TOutput> | JdpFailure> {
    return this.binding.invoke(request)
  }
  events(aggregateId: string, afterSequence = 0): AsyncIterable<DurableEvent> {
    return this.binding.events(aggregateId, afterSequence)
  }
  cancel(taskId: string): Promise<void> {
    return this.binding.cancel(taskId)
  }
  reconcile(executionId: string, idempotencyKey: string): Promise<ExecutionReconciliation> {
    return this.binding.reconcile(executionId, idempotencyKey)
  }
}

export class MockJdpClient implements JdpClient {
  readonly invocations: JdpRequest<unknown>[] = []
  readonly cancellations: string[] = []
  private readonly outcomes = new Map<string, JdpSuccess<unknown> | JdpFailure>()
  private readonly executions = new Map<string, ExecutionReconciliation>()
  private readonly eventBus = new DurableEventBus(new MemoryEventStore())

  constructor(private readonly operations = ["design.query", "design.patch.apply", "design.validate"]) {}

  async capabilities(): Promise<JdpCapabilities> {
    return { protocolVersion: "0.1", operations: [...this.operations], reconciliation: true }
  }

  setOutcome(operation: string, outcome: JdpSuccess<unknown> | JdpFailure): void {
    this.outcomes.set(operation, structuredClone(outcome))
  }

  async invoke<TInput, TOutput>(request: JdpRequest<TInput>): Promise<JdpSuccess<TOutput> | JdpFailure> {
    this.invocations.push(structuredClone(request) as JdpRequest<unknown>)
    const configured = this.outcomes.get(request.operation)
    const result = configured ?? successFor(request)
    const executionId = result.executionId
    if (executionId) {
      this.executions.set(executionId, {
        executionId,
        idempotencyKey: request.idempotencyKey,
        status: result.success ? "succeeded" : "failed",
        result,
      })
    }
    this.eventBus.append({
      aggregateId: request.projectId,
      ...(request.traceContext ? { traceId: request.traceContext.traceId } : {}),
      type: result.success ? "jdp.request.completed" : "jdp.request.failed",
      data: { requestId: request.requestId, executionId },
    })
    return structuredClone(result) as JdpSuccess<TOutput> | JdpFailure
  }

  events(aggregateId: string, afterSequence = 0): AsyncIterable<DurableEvent> {
    return this.eventBus.events(aggregateId, afterSequence)
  }

  async cancel(taskId: string): Promise<void> {
    this.cancellations.push(taskId)
  }

  async reconcile(executionId: string, idempotencyKey: string): Promise<ExecutionReconciliation> {
    const result = this.executions.get(executionId)
    if (!result || result.idempotencyKey !== idempotencyKey) {
      return { executionId, idempotencyKey, status: "not-found" }
    }
    return structuredClone(result)
  }
}

function successFor<TInput>(request: JdpRequest<TInput>): JdpSuccess<unknown> {
  const executionId = crypto.randomUUID()
  return {
    success: true,
    requestId: request.requestId,
    executionId,
    ...(request.baseRevision
      ? { baseRevision: request.baseRevision, newRevision: `revision:${executionId}` }
      : {}),
    data: { mock: true, operation: request.operation },
    artifactRefs: [],
    evidenceRefs: [],
    findings: { errors: 0, warnings: 0, info: 0 },
  }
}
