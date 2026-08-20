import type {
  AdmitInput,
  AdmittedInput,
  ApprovalReply,
  ApprovalView,
  ArtifactView,
  CreateSessionInput,
  DurableEvent,
  RunView,
  RuntimeCapabilities,
  RuntimeClient,
  SessionView,
} from "../../clients/contracts";
import { ClientContractError } from "../../clients/errors";
import { approvalFixtures, initialTimeline, runtimeEventFixtures } from "./fixtures";
import { loadMockServerState, saveMockServerState } from "./mockServerPersistence";

export type RuntimeMockScenario = "normal" | "provider-failure" | "tool-failure" | "disconnect-once" | "unavailable";

interface PersistedRuntimeServer {
  schemaVersion: "0.1";
  approvals: ApprovalView[];
  eventsBySession: Array<[string, DurableEvent[]]>;
  admittedByRequestId: Array<[string, AdmittedInput]>;
  runs: Array<[string, RunView]>;
}

export class MockRuntimeClient implements RuntimeClient {
  readonly contractVersion = "0.1" as const;
  private readonly scenario: RuntimeMockScenario;
  private readonly approvals = structuredClone(approvalFixtures);
  private readonly eventsBySession = new Map<string, DurableEvent[]>();
  private readonly admittedByRequestId = new Map<string, AdmittedInput>();
  private readonly runs = new Map<string, RunView>();
  private disconnected = false;

  constructor(scenario: RuntimeMockScenario = "normal") {
    this.scenario = scenario;
    const persisted = scenario === "normal" ? loadMockServerState<PersistedRuntimeServer>("runtime") : undefined;
    if (persisted?.schemaVersion === "0.1") {
      this.approvals.splice(0, this.approvals.length, ...structuredClone(persisted.approvals));
      for (const [sessionId, events] of persisted.eventsBySession) this.eventsBySession.set(sessionId, structuredClone(events));
      for (const [requestId, admitted] of persisted.admittedByRequestId) this.admittedByRequestId.set(requestId, structuredClone(admitted));
      for (const [sessionId, run] of persisted.runs) this.runs.set(sessionId, structuredClone(run));
    } else {
      this.eventsBySession.set("session-main", structuredClone(runtimeEventFixtures));
    }
  }

  async createSession(input: CreateSessionInput): Promise<SessionView> {
    this.assertAvailable();
    const id = input.title ? `session-${slug(input.title)}` : "session-new";
    this.eventsBySession.set(id, []);
    this.persistServerState();
    return { id, title: input.title, projectId: input.projectId, status: "idle", lastSequence: 0 };
  }

  async admitInput(input: AdmitInput): Promise<AdmittedInput> {
    this.assertAvailable();
    const existing = this.admittedByRequestId.get(input.clientRequestId);
    if (existing) return structuredClone(existing);

    const event = this.appendEvent(input.sessionId, "input.admitted", {
      text: input.text,
      attachmentRefs: input.attachmentRefs,
      delivery: input.delivery,
    });
    const admitted = { inputId: `input-${event.sequence}`, admittedSequence: event.sequence, delivery: input.delivery };
    this.admittedByRequestId.set(input.clientRequestId, admitted);
    this.persistServerState();
    return structuredClone(admitted);
  }

  async resumeRun(sessionId: string): Promise<RunView> {
    this.assertAvailable();
    const existing = this.runs.get(sessionId);
    if (existing?.status === "running") return structuredClone(existing);

    const timeline = structuredClone(initialTimeline);
    let status: RunView["status"] = "waiting";
    let output = "已读取受影响网络。候选走线需要人工审批后才会写入 draft。";

    if (this.scenario === "provider-failure") {
      timeline.push({
        id: "model-failed",
        kind: "model",
        title: "模型尝试 2",
        detail: "provider stream 在完成前中断，草稿已丢弃",
        status: "failed",
        startedAt: new Date().toISOString(),
        error: { code: "PROVIDER_STREAM_INTERRUPTED", summary: "响应流未形成可提交终态", suggestedAction: "更换故障域后重新尝试" },
      });
      status = "failed";
      output = "模型流中断；没有工具调用或设计 revision 被提交。";
    }
    if (this.scenario === "tool-failure") {
      timeline.push({
        id: "tool-failed",
        kind: "tool",
        title: "pcb.route_critical",
        detail: "JDP 返回受保护对象冲突",
        target: "footprint-j1",
        effect: "design-write",
        status: "failed",
        startedAt: new Date().toISOString(),
        error: { code: "LOCKED_OBJECT_CONFLICT", summary: "J1 为人工锁定对象", suggestedAction: "缩小选区或请求用户修改锁定策略" },
      });
      status = "failed";
      output = "候选未写入；请保留 J1 并重新生成局部方案。";
    }

    const run: RunView = { id: "run-1", sessionId, title: "RS-485 局部修复", status, timeline, output };
    this.runs.set(sessionId, run);
    this.appendEvent(sessionId, status === "failed" ? "run.failed" : "run.blocked", { runId: run.id, status });
    this.persistServerState();
    return structuredClone(run);
  }

  async cancelRun(runId: string, reason = "用户取消"): Promise<void> {
    const entry = [...this.runs.entries()].find(([, run]) => run.id === runId);
    if (!entry) throw new ClientContractError("RUN_NOT_FOUND", "找不到要取消的 run");
    entry[1].status = "cancelled";
    this.appendEvent(entry[0], "run.cancelled", { runId, reason });
    this.persistServerState();
  }

  async *events(sessionId: string, afterSequence = 0): AsyncIterable<DurableEvent> {
    this.assertAvailable();
    const events = this.eventsBySession.get(sessionId) ?? [];
    for (const event of events) {
      if (event.sequence <= afterSequence) continue;
      yield structuredClone(event);
      if (this.scenario === "disconnect-once" && !this.disconnected) {
        this.disconnected = true;
        throw new ClientContractError("EVENT_STREAM_DISCONNECTED", "mock 事件流已断开", true, "按最后 sequence 回放并重连");
      }
    }
  }

  async listApprovals(sessionId?: string): Promise<ApprovalView[]> {
    return structuredClone(this.approvals.filter((item) => !sessionId || item.sessionId === sessionId));
  }

  async replyApproval(input: ApprovalReply): Promise<void> {
    const approval = this.approvals.find((item) => item.id === input.approvalId);
    if (!approval) throw new ClientContractError("APPROVAL_NOT_FOUND", "审批请求不存在或已过期");
    if (approval.status !== "pending") throw new ClientContractError("APPROVAL_ALREADY_RESOLVED", "审批已经处理");
    approval.status = input.decision === "approve-once" ? "approved" : "denied";
    this.appendEvent(approval.sessionId, "approval.resolved", { approvalId: approval.id, decision: input.decision });
    this.persistServerState();
  }

  async getArtifact(artifactId: string): Promise<ArtifactView> {
    return {
      id: artifactId,
      name: "局部 DRC 证据摘要",
      mediaType: "application/json",
      revision: "rev-candidate-007",
      summary: "Mock artifact reference；真实集成时由 runtime artifact endpoint 提供。",
    };
  }

  async capabilities(): Promise<RuntimeCapabilities> {
    if (this.scenario === "unavailable") {
      return { contractVersion: "0.1", mode: "mock", state: "unavailable", replay: true, approvals: true, providers: [], reason: "mock runtime unavailable scenario" };
    }
    return { contractVersion: "0.1", mode: "mock", state: "connected", replay: true, approvals: true, providers: [{ provider: "fixture", model: "mock-runtime" }], reason: "versioned development fixture" };
  }

  private appendEvent(sessionId: string, type: string, data: unknown): DurableEvent {
    const list = this.eventsBySession.get(sessionId) ?? [];
    const event: DurableEvent = {
      schemaVersion: "0.1",
      aggregateId: sessionId,
      sequence: (list.at(-1)?.sequence ?? 0) + 1,
      timestamp: new Date().toISOString(),
      sessionId,
      type,
      data,
    };
    list.push(event);
    this.eventsBySession.set(sessionId, list);
    return event;
  }

  private assertAvailable(): void {
    if (this.scenario === "unavailable") {
      throw new ClientContractError("RUNTIME_UNAVAILABLE", "Agent runtime 当前不可用", true, "检查独立 sidecar 健康状态");
    }
  }

  private persistServerState(): void {
    saveMockServerState("runtime", {
      schemaVersion: "0.1",
      approvals: this.approvals,
      eventsBySession: [...this.eventsBySession.entries()],
      admittedByRequestId: [...this.admittedByRequestId.entries()],
      runs: [...this.runs.entries()],
    } satisfies PersistedRuntimeServer);
  }
}

function slug(value: string): string {
  return value.trim().toLowerCase().replace(/[^a-z0-9\u4e00-\u9fff]+/gu, "-").replace(/^-|-$/gu, "") || "untitled";
}
