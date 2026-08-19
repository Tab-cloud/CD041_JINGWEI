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
} from "./contracts";
import { getTransportConfiguration, invokeRuntimeRpc } from "../utils/tauriBridge";

interface RuntimeSession {
  id: string;
  projectId: string;
  title: string;
}

interface RuntimeAdmission {
  id: string;
  sequence: number;
  delivery: "steer" | "queue";
}

interface RuntimeRun {
  id: string;
  sessionId: string;
  status: string;
  output?: string;
  error?: { code: string; summary: string; suggestedActions?: Array<{ description: string }> };
  createdAt: string;
  updatedAt: string;
}

interface RuntimeApproval {
  id: string;
  sessionId: string;
  toolName: string;
  permission: string;
  effects: string[];
  resourceClaims: string[];
  redactedInput: Record<string, unknown>;
  reason: string;
  expiresAt: string;
  status: ApprovalView["status"] | "cancelled";
}

export class RuntimeTransportClient implements RuntimeClient {
  async createSession(input: CreateSessionInput): Promise<SessionView> {
    const configuration = await getTransportConfiguration();
    const session = await invokeRuntimeRpc<RuntimeSession>("session.create", {
      projectId: input.projectId,
      workspaceId: configuration.workspaceId,
      title: input.title,
      dataPolicy: "local",
    });
    return { id: session.id, title: session.title, projectId: session.projectId, status: "idle", lastSequence: 0 };
  }

  async admitInput(input: AdmitInput): Promise<AdmittedInput> {
    const admitted = await invokeRuntimeRpc<RuntimeAdmission>("input.admit", {
      sessionId: input.sessionId,
      clientRequestId: input.clientRequestId,
      delivery: input.delivery,
      content: input.text,
      attachmentRefs: input.attachmentRefs,
    });
    return { inputId: admitted.id, admittedSequence: admitted.sequence, delivery: admitted.delivery };
  }

  async resumeRun(sessionId: string): Promise<RunView> {
    return mapRun(await invokeRuntimeRpc<RuntimeRun>("run.resume", { sessionId }));
  }

  async cancelRun(runId: string, reason?: string): Promise<void> {
    await invokeRuntimeRpc("run.cancel", { runId, reason });
  }

  async *events(sessionId: string, afterSequence = 0): AsyncIterable<DurableEvent> {
    const events = await invokeRuntimeRpc<DurableEvent[]>("events.read", {
      sessionId,
      afterSequence,
      limit: 1_000,
    });
    for (const event of events) yield event;
  }

  async listApprovals(sessionId?: string): Promise<ApprovalView[]> {
    const approvals = await invokeRuntimeRpc<RuntimeApproval[]>("approval.list", { sessionId });
    return approvals.map((approval) => ({
      id: approval.id,
      sessionId: approval.sessionId,
      toolName: approval.toolName,
      target: approval.resourceClaims.join(", "),
      reason: approval.reason,
      effect: approval.effects.join(", "),
      redactedInput: JSON.stringify(approval.redactedInput),
      scope: approval.permission,
      expiresAt: approval.expiresAt,
      status: approval.status === "cancelled" ? "denied" : approval.status,
    }));
  }

  async replyApproval(input: ApprovalReply): Promise<void> {
    await invokeRuntimeRpc("approval.reply", {
      approvalId: input.approvalId,
      decision: input.decision === "approve-once" ? "allow-once" : "deny",
    });
  }

  async getArtifact(artifactId: string): Promise<ArtifactView> {
    const artifact = await invokeRuntimeRpc<{
      id: string;
      mediaType: string;
      byteLength: number;
      sha256: string;
    }>("artifact.get", { artifactId });
    return {
      id: artifact.id,
      name: artifact.id,
      mediaType: artifact.mediaType,
      revision: "runtime-artifact",
      summary: `${artifact.byteLength} bytes · SHA-256 ${artifact.sha256}`,
    };
  }

  async capabilities(): Promise<RuntimeCapabilities> {
    const capabilities = await invokeRuntimeRpc<{
      protocolVersion: string;
      durableEvents: boolean;
      approvalScopes: boolean;
      jdp: { reconciliation: boolean };
    }>("capabilities");
    if (capabilities.protocolVersion !== "0.1") {
      return {
        contractVersion: "0.1",
        mode: "real",
        state: "version-mismatch",
        replay: false,
        approvals: false,
        reason: `Runtime protocol ${capabilities.protocolVersion} is unsupported`,
      };
    }
    return {
      contractVersion: "0.1",
      mode: "real",
      state: "connected",
      replay: capabilities.durableEvents,
      approvals: capabilities.approvalScopes,
      reason: capabilities.jdp.reconciliation
        ? "真实 Runtime RPC 与 daemon reconciliation 已连接"
        : "真实 Runtime RPC 已连接；daemon reconciliation 尚未闭合",
    };
  }
}

function mapRun(run: RuntimeRun): RunView {
  const status: RunView["status"] = (() => {
    if (run.status === "completed") return "completed";
    if (run.status === "failed") return "failed";
    if (run.status === "cancelled") return "cancelled";
    if (run.status.startsWith("waiting") || run.status === "quarantined") return "waiting";
    return "running";
  })();
  return {
    id: run.id,
    sessionId: run.sessionId,
    title: "Main Agent",
    status,
    timeline: run.error
      ? [{
          id: `runtime-${run.id}`,
          kind: "run",
          title: "Runtime run",
          detail: run.error.summary,
          status: "failed",
          startedAt: run.createdAt,
          error: {
            code: run.error.code,
            summary: run.error.summary,
            suggestedAction: run.error.suggestedActions?.[0]?.description ?? "检查结构化失败后重试",
          },
        }]
      : [],
    output: run.output,
  };
}
