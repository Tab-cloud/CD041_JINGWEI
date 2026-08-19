import type {
  AiPatchRequest,
  CheckpointRequest,
  CheckpointView,
  CrossProbeRequest,
  CrossProbeResult,
  DesignCapabilities,
  DesignClient,
  DesignQuery,
  DisplayList,
  DurableEvent,
  EditCommandGroup,
  EditCommitResult,
  EditPreview,
  EditSessionView,
  HitTestRequest,
  HitTestResult,
  OpenEditSessionRequest,
  PatchDecision,
  PatchProposalView,
  ProjectView,
  PublishDraftRequest,
  RevisionView,
  SemanticEditCommand,
  ViewRequest,
} from "./contracts";
import { ClientContractError } from "./errors";
import { getTransportConfiguration, invokeDesignRpc } from "../utils/tauriBridge";

interface JdpSuccess<T> {
  success: true;
  requestId: string;
  executionId: string;
  newRevision?: string;
  data: T;
}

interface JdpFailure {
  success: false;
  error: { code: string; summary: string; retryable: boolean; suggestedActions?: Array<{ description: string }> };
}

interface ProjectInfo {
  projectId: string;
  name: string;
  currentRevision: string;
}

interface QueryOutput {
  revision: string;
  entities: Array<{ id: string; kind: string; properties: unknown; provenance?: { sourceKind?: string } }>;
  edges: Array<{ kind: string; from: string; to: string }>;
}

export class DesignTransportClient implements DesignClient {
  async getProject(_projectId: string): Promise<ProjectView> {
    void _projectId;
    const projectId = await this.projectId();
    let project: ProjectInfo;
    try {
      project = await this.jdp<ProjectInfo>(projectId, "project.get", {});
    } catch (error) {
      if (!(error instanceof ClientContractError) || error.code !== "project_not_found") throw error;
      project = await this.jdp<ProjectInfo>(projectId, "project.create", { name: "精卫 AI 原生设计" });
    }
    return {
      id: project.projectId,
      name: project.name,
      currentRevision: project.currentRevision,
      candidateRevision: project.currentRevision,
      maturity: "draft",
      stages: [
        { id: "draft", label: "Draft", status: "current" },
        { id: "candidate", label: "ECAD Candidate", status: "blocked" },
        { id: "engineering", label: "Engineering Verified", status: "blocked" },
        { id: "manufacturing", label: "Manufacturing Candidate", status: "blocked" },
        { id: "prototype", label: "Prototype Verified", status: "blocked" },
        { id: "release", label: "Production Released", status: "blocked" },
      ],
      releaseBlockers: ["尚未形成黄金板 logical/physical candidate", "工程、制造、样板与组织签核未完成"],
      sheets: [],
      layers: [
        { id: "logical", name: "Logical", color: "#4f8cff", visible: true, category: "logical" },
        { id: "F.Cu", name: "F.Cu", color: "#d14b54", visible: true, category: "copper" },
        { id: "B.Cu", name: "B.Cu", color: "#4b72d1", visible: true, category: "copper" },
        { id: "Edge.Cuts", name: "Edge.Cuts", color: "#b0b0b0", visible: true, category: "outline" },
      ],
      findings: [],
    };
  }

  async query<T>(request: DesignQuery): Promise<T> {
    const output = await this.jdp<QueryOutput>(request.projectId, "design.query", {
      revision: request.revision,
      kind: null,
      ids: request.entityIds ?? [],
      limit: 1_000,
      cursor: null,
    });
    if (request.query === "entity-properties") {
      return Object.fromEntries(
        output.entities.map((entity) => [entity.id, {
          identity: entity.id,
          kind: entity.kind,
          provenance: entity.provenance?.sourceKind ?? "JIR",
          ...stringProperties(entity.properties),
        }]),
      ) as T;
    }
    return output as T;
  }

  async getView(request: ViewRequest): Promise<DisplayList> {
    await this.assertRevisionExists(request.projectId, request.revision);
    return {
      revision: request.revision,
      view: request.view,
      primitives: [],
      totalPrimitives: 0,
      incrementalToken: `${request.view}:${request.revision}:empty-native-provider`,
    };
  }

  async hitTest(request: HitTestRequest): Promise<HitTestResult> {
    await this.assertRevisionExists(request.projectId, request.revision);
    return { entityIds: [], latencyMs: 0 };
  }

  async crossProbe(request: CrossProbeRequest): Promise<CrossProbeResult> {
    if (request.entityIds.length === 0) return { entityIds: [], relatedEntityIds: [], netIds: [], focus: {} };
    const explanation = await this.jdp<{
      incoming: Array<{ from: string; to: string }>;
      outgoing: Array<{ from: string; to: string }>;
    }>(request.projectId, "design.explain", {
      revision: request.revision,
      entityId: request.entityIds[0],
    });
    const relatedEntityIds = [...new Set(
      [...explanation.incoming, ...explanation.outgoing]
        .flatMap((edge) => [edge.from, edge.to])
        .filter((id) => !request.entityIds.includes(id)),
    )];
    return { entityIds: request.entityIds, relatedEntityIds, netIds: [], focus: {} };
  }

  async openEditSession(request: OpenEditSessionRequest): Promise<EditSessionView> {
    const info = await this.jdp<{
      editSessionId: string;
      baseProjectRevision: string;
      draftHeadRevision: string;
      leaseExpiresAt: string;
    }>(request.projectId, "edit_session.open", { baseRevision: request.baseRevision, leaseSeconds: 1_800 });
    return {
      id: info.editSessionId,
      baseRevision: info.baseProjectRevision,
      draftRevision: info.draftHeadRevision,
      leaseExpiresAt: info.leaseExpiresAt,
      recovered: false,
    };
  }

  async previewEdit(request: EditCommandGroup): Promise<EditPreview> {
    const commands = request.commands.map(mapCommand);
    const output = await this.jdp<{
      previewHash: string;
      validation: { valid: boolean; findings: Array<{ code: string; summary: string }> };
    }>(await this.projectId(), "edit_command.preview", {
      editSessionId: request.editSessionId,
      baseDraftRevision: request.baseDraftRevision,
      commands,
      expectedPostconditions: ["graph.integrity"],
      previewHash: null,
    });
    return {
      previewId: output.previewHash,
      baseDraftRevision: request.baseDraftRevision,
      normalizedCommands: request.commands,
      ghostPrimitives: [],
      affectedObjectIds: [...new Set(request.commands.flatMap(commandEntityIds))],
      conflicts: output.validation.valid
        ? []
        : output.validation.findings.map((finding) => ({ code: finding.code, message: finding.summary, entityIds: [] })),
      findings: [],
      committable: output.validation.valid,
      latencyMs: 0,
    };
  }

  async commitEdit(request: EditCommandGroup): Promise<EditCommitResult> {
    if (!request.previewId) throw new ClientContractError("preview_required", "真实 draft commit 必须引用 daemon preview hash");
    const output = await this.jdp<{
      draftRevision: string;
      validation: { valid: boolean };
    }>(await this.projectId(), "edit_command.commit", {
      editSessionId: request.editSessionId,
      baseDraftRevision: request.baseDraftRevision,
      commands: request.commands.map(mapCommand),
      expectedPostconditions: ["graph.integrity"],
      previewHash: request.previewId,
    });
    return editResult(output.draftRevision, request.label, request.commands);
  }

  async undo(editSessionId: string): Promise<EditCommitResult> {
    const output = await this.jdp<{ draftRevision: string }>(await this.projectId(), "edit.undo", { editSessionId });
    return editResult(output.draftRevision, "已由 daemon 创建 inverse draft revision", []);
  }

  async redo(editSessionId: string): Promise<EditCommitResult> {
    const output = await this.jdp<{ draftRevision: string }>(await this.projectId(), "edit.redo", { editSessionId });
    return editResult(output.draftRevision, "已由 daemon 重放 draft command group", []);
  }

  async checkpoint(request: CheckpointRequest): Promise<CheckpointView> {
    const info = await this.jdp<{ draftHeadRevision: string }>(await this.projectId(), "edit.checkpoint", {
      editSessionId: request.editSessionId,
    });
    return { id: `checkpoint-${crypto.randomUUID()}`, name: request.name, revision: info.draftHeadRevision, createdAt: new Date().toISOString() };
  }

  async publishDraft(request: PublishDraftRequest): Promise<RevisionView> {
    const output = await this.jdp<{ revision: string; proposalId: string }>(await this.projectId(), "edit.publish", {
      editSessionId: request.editSessionId,
    });
    return {
      id: output.revision,
      parentId: request.draftRevision,
      label: `已创建 proposal ${output.proposalId}；project head 未改变`,
    };
  }

  async proposeAiPatch(request: AiPatchRequest): Promise<PatchProposalView> {
    throw new ClientContractError("capability_unavailable", `真实 AI patch/hunk adapter 尚未接入（${request.instruction.slice(0, 80)}）；不会回退到 fixture`);
  }

  async acceptPatchHunks(request: PatchDecision): Promise<RevisionView> {
    throw new ClientContractError("capability_unavailable", `真实 hunk decision adapter 尚未接入（proposal ${request.proposalId}）；不会伪造 revision`);
  }

  async *events(aggregateId: string, afterSequence = 0): AsyncIterable<DurableEvent> {
    const events = await this.jdp<DurableEvent[]>(aggregateId, "task.events", { afterSequence, limit: 1_000 });
    for (const event of events) yield event;
  }

  async capabilities(): Promise<DesignCapabilities> {
    const projectId = await this.projectId();
    const output = await this.jdp<{
      protocolVersions: string[];
      schemaVersions: string[];
      operations: Record<string, "available" | "capability-unavailable">;
      readWrite: boolean;
    }>(projectId, "workspace.capabilities", {});
    if (!output.protocolVersions.includes("0.1") || !output.schemaVersions.includes("0.2")) {
      return { contractVersion: "0.1", mode: "real", state: "version-mismatch", reason: "JDP/JIR version mismatch" };
    }
    const required = ["project.get", "design.query", "edit_session.open", "edit_command.preview", "edit_command.commit"];
    const missing = required.filter((operation) => output.operations[operation] !== "available");
    return {
      contractVersion: "0.1",
      mode: "real",
      state: missing.length === 0 && output.readWrite ? "connected" : "unavailable",
      reason: missing.length === 0 ? "真实 daemon JDP transport" : `缺少真实操作: ${missing.join(", ")}`,
    };
  }

  private async assertRevisionExists(projectId: string, revision: string): Promise<void> {
    await this.jdp(projectId, "design.query", { revision, kind: null, ids: [], limit: 1, cursor: null });
  }

  private async projectId(): Promise<string> {
    return (await getTransportConfiguration()).projectId;
  }

  private async jdp<T>(projectId: string, operation: string, input: unknown, baseRevision?: string): Promise<T> {
    const configuration = await getTransportConfiguration();
    const response = await invokeDesignRpc<JdpSuccess<T> | JdpFailure>({
      protocolVersion: configuration.protocolVersion,
      requestId: `request-ui-${crypto.randomUUID()}`,
      idempotencyKey: `idem-ui-${crypto.randomUUID()}`,
      workspaceId: configuration.workspaceId,
      projectId,
      operation,
      ...(baseRevision ? { baseRevision } : {}),
      deadlineMs: 30_000,
      dryRun: false,
      input,
    });
    if (!response.success) {
      throw new ClientContractError(
        response.error.code,
        response.error.summary,
        response.error.retryable,
        response.error.suggestedActions?.[0]?.description,
      );
    }
    return response.data;
  }
}

function mapCommand(command: SemanticEditCommand): { kind: string; payload: unknown } {
  if (command.kind === "schematic.set_property") {
    return { kind: "core.set_property", payload: { entityId: command.entityId, key: command.key, value: command.value } };
  }
  if (command.kind === "design.set_edit_control") {
    return { kind: "core.set_edit_policy", payload: { entityIds: command.entityIds, policy: command.mode } };
  }
  throw new ClientContractError(
    "capability_unavailable",
    `真实 daemon 尚未注册人工编辑命令 ${command.kind}；不会回退到 fixture`,
  );
}

function commandEntityIds(command: SemanticEditCommand): string[] {
  if ("entityId" in command) return [command.entityId];
  if ("entityIds" in command) return command.entityIds;
  if ("fromEntityId" in command) return [command.fromEntityId, command.toEntityId];
  return [];
}

function editResult(revision: string, summary: string, commands: SemanticEditCommand[]): EditCommitResult {
  return {
    newDraftRevision: revision,
    operationGroupId: `daemon-${crypto.randomUUID()}`,
    summary,
    changedEntityIds: [...new Set(commands.flatMap(commandEntityIds))],
    validationDelta: { added: [], resolvedIds: [] },
    undoAvailable: true,
    redoAvailable: false,
    currentProjectRevisionChanged: false,
  };
}

function stringProperties(value: unknown): Record<string, string> {
  if (!value || typeof value !== "object" || Array.isArray(value)) return { value: String(value) };
  return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, typeof item === "string" ? item : JSON.stringify(item)]));
}
