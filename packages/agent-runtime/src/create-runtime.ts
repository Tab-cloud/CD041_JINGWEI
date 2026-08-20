import { DurableEventBus, SqliteEventStore } from "@jingwei/agent-events"
import { OpenTelemetryRuntimeTracer, type RuntimeTracer } from "@jingwei/agent-observability"
import {
  ApprovalService,
  PolicyEngine,
  type PolicyRule,
  SqliteApprovalRepository,
} from "@jingwei/agent-policy"
import type { JdpClient } from "@jingwei/agent-protocol"
import {
  SqliteArtifactStore,
  SqliteToolExecutionRepository,
  ToolRegistry,
  ToolSettlementPipeline,
} from "@jingwei/agent-tools"
import { AgentLoop } from "./agent-loop.ts"
import { ProviderRegistry } from "./provider-registry.ts"
import { RecoveryService } from "./recovery-service.ts"
import { SessionRunCoordinator } from "./run-coordinator.ts"
import { SqliteRuntimeRepository } from "./runtime-repository.ts"
import { AgentRuntimeService } from "./runtime-service.ts"
import { SessionService } from "./session-service.ts"

export interface RuntimeFactoryOptions {
  databasePath: string
  jdpClient: JdpClient
  policyRules: PolicyRule[]
  tracer?: RuntimeTracer
}

export interface RuntimeKit extends Disposable {
  client: AgentRuntimeService
  providers: ProviderRegistry
  tools: ToolRegistry
  policy: PolicyEngine
  approvals: ApprovalService
  events: DurableEventBus
  recovery: RecoveryService
  loop: AgentLoop
  settlement: ToolSettlementPipeline
  repository: SqliteRuntimeRepository
  executions: SqliteToolExecutionRepository
}

export async function createRuntime(options: RuntimeFactoryOptions): Promise<RuntimeKit> {
  const repository = new SqliteRuntimeRepository(options.databasePath)
  const eventStore = new SqliteEventStore(options.databasePath)
  const approvalRepository = new SqliteApprovalRepository(options.databasePath)
  const executions = new SqliteToolExecutionRepository(options.databasePath)
  const artifacts = new SqliteArtifactStore(options.databasePath)
  const events = new DurableEventBus(eventStore)
  const policy = new PolicyEngine(options.policyRules)
  const approvals = new ApprovalService(approvalRepository, events)
  const tools = new ToolRegistry()
  const tracer = options.tracer ?? new OpenTelemetryRuntimeTracer()
  const settlement = new ToolSettlementPipeline({
    registry: tools,
    policy,
    approvals,
    executions,
    artifacts,
    events,
    tracer,
  })
  const providers = new ProviderRegistry()
  const loop = new AgentLoop({ providers, tools, settlement, repository, events, tracer })
  const sessions = new SessionService(repository, events)
  const coordinator = new SessionRunCoordinator<import("@jingwei/agent-protocol").RunView>()
  const jdpCapabilities = await options.jdpClient.capabilities()
  const client = new AgentRuntimeService({
    repository,
    events,
    sessions,
    coordinator,
    loop,
    approvals,
    artifacts,
    providers,
    jdpCapabilities,
  })
  const recovery = new RecoveryService({ repository, executions, approvals, events })
  return {
    client,
    providers,
    tools,
    policy,
    approvals,
    events,
    recovery,
    loop,
    settlement,
    repository,
    executions,
    [Symbol.dispose]: () => {
      recovery.release()
      artifacts.close()
      executions.close()
      approvalRepository.close()
      eventStore.close()
      repository.close()
    },
  }
}
