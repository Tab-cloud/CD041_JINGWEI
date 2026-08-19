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
  DisplayPrimitive,
  DurableEvent,
  EditCommandGroup,
  EditCommitResult,
  EditPolicy,
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
} from "../../clients/contracts";
import { ClientContractError } from "../../clients/errors";
import { cullDisplayList, hitDistance } from "../../canvas/displayList";
import {
  goldenFindings,
  objectProperties,
  pcbPrimitives,
  projectFixture,
  schematicPrimitives,
} from "./fixtures";
import { loadMockServerState, saveMockServerState } from "./mockServerPersistence";

type DesignMockScenario = "normal" | "unavailable" | "version-mismatch";
type DesignSnapshot = Record<"schematic" | "pcb", DisplayPrimitive[]>;

interface PersistedDesignServer {
  schemaVersion: "0.1";
  project: ProjectView;
  views: DesignSnapshot;
  policies: Array<[string, EditPolicy]>;
  undoStack: DesignSnapshot[];
  redoStack: DesignSnapshot[];
  eventLog: DurableEvent[];
  editSession?: EditSessionView;
  revisionNumber: number;
  properties: typeof objectProperties;
}

export class MockDesignClient implements DesignClient {
  readonly contractVersion = "0.1" as const;
  private readonly scenario: DesignMockScenario;
  private project = structuredClone(projectFixture);
  private views: DesignSnapshot = {
    schematic: structuredClone(schematicPrimitives),
    pcb: structuredClone(pcbPrimitives),
  };
  private readonly policies = new Map<string, EditPolicy>([["footprint-j1", "locked"], ["symbol-u2", "ask-before-change"]]);
  private readonly previews = new Map<string, EditPreview>();
  private readonly undoStack: DesignSnapshot[] = [];
  private readonly redoStack: DesignSnapshot[] = [];
  private readonly eventLog: DurableEvent[] = [];
  private editSession?: EditSessionView;
  private revisionNumber = 11;

  constructor(scenario: DesignMockScenario = "normal") {
    this.scenario = scenario;
    const persisted = scenario === "normal" ? loadMockServerState<PersistedDesignServer>("design") : undefined;
    if (persisted?.schemaVersion === "0.1") this.restoreServerState(persisted);
  }

  async getProject(projectId: string): Promise<ProjectView> {
    this.assertAvailable();
    if (projectId !== this.project.id) throw new ClientContractError("PROJECT_NOT_FOUND", "项目不存在");
    return structuredClone(this.project);
  }

  async query<T>(request: DesignQuery): Promise<T> {
    this.assertAvailable();
    if (request.query === "entity-properties") {
      const result = Object.fromEntries((request.entityIds ?? []).map((id) => [id, objectProperties[id] ?? { identity: id, provenance: "mock fixture" }]));
      return structuredClone(result) as T;
    }
    if (request.query === "history") return structuredClone({ revision: this.draftRevision, operations: this.undoStack.length }) as T;
    return structuredClone({ entityIds: request.entityIds ?? [], revision: request.revision, minimal: true }) as T;
  }

  async getView(request: ViewRequest): Promise<DisplayList> {
    this.assertAvailable();
    const source = this.views[request.view].filter((primitive) => request.visibleLayerIds.includes(primitive.layerId));
    const primitives = cullDisplayList(source, request.viewport);
    return {
      revision: this.draftRevision,
      view: request.view,
      primitives: structuredClone(primitives),
      totalPrimitives: source.length,
      incrementalToken: `${request.view}:${this.draftRevision}:${request.lod}`,
    };
  }

  async hitTest(request: HitTestRequest): Promise<HitTestResult> {
    this.assertAvailable();
    const started = performance.now();
    const candidates = this.views[request.view]
      .filter((primitive) => request.visibleLayerIds.includes(primitive.layerId))
      .map((primitive) => ({ primitive, distance: hitDistance(primitive, request.point) }))
      .filter((item) => item.distance <= request.tolerance)
      .sort((left, right) => left.distance - right.distance || right.primitive.zIndex - left.primitive.zIndex)
      .map((item) => item.primitive.entityId);
    const grid = 10;
    return {
      entityIds: [...new Set(candidates)],
      snap: { point: { x: Math.round(request.point.x / grid) * grid, y: Math.round(request.point.y / grid) * grid }, kind: "grid" },
      latencyMs: performance.now() - started,
    };
  }

  async crossProbe(request: CrossProbeRequest): Promise<CrossProbeResult> {
    const expanded = request.entityIds.flatMap((id) => this.relatedIds(id));
    const relatedEntityIds = [...new Set(expanded.filter((id) => !request.entityIds.includes(id)))];
    const netIds = [...new Set([...request.entityIds, ...relatedEntityIds].filter((id) => id.startsWith("net-")))];
    return {
      entityIds: request.entityIds,
      relatedEntityIds,
      netIds,
      focus: {
        schematic: this.focusFor("schematic", [...request.entityIds, ...relatedEntityIds]),
        pcb: this.focusFor("pcb", [...request.entityIds, ...relatedEntityIds]),
      },
    };
  }

  async openEditSession(request: OpenEditSessionRequest): Promise<EditSessionView> {
    this.assertAvailable();
    const recovered = Boolean(request.recoverSessionId && this.editSession?.id === request.recoverSessionId);
    this.editSession = recovered
      ? { ...this.editSession!, recovered: true, leaseExpiresAt: leaseTime() }
      : { id: "edit-session-golden", baseRevision: request.baseRevision, draftRevision: this.draftRevision, leaseExpiresAt: leaseTime(), recovered: false };
    this.persistServerState();
    return structuredClone(this.editSession);
  }

  async previewEdit(request: EditCommandGroup): Promise<EditPreview> {
    this.assertSession(request.editSessionId);
    const started = performance.now();
    const conflicts = request.commands.flatMap((command) => this.conflictsFor(command));
    const previewId = `preview-${this.previews.size + 1}`;
    const preview: EditPreview = {
      previewId,
      baseDraftRevision: request.baseDraftRevision,
      normalizedCommands: structuredClone(request.commands),
      ghostPrimitives: request.commands.flatMap((command) => this.ghostFor(command)),
      affectedObjectIds: [...new Set(request.commands.flatMap(affectedIds))],
      conflicts,
      findings: conflicts.length > 0 ? [] : structuredClone(goldenFindings.filter((finding) => finding.status === "open")),
      committable: conflicts.length === 0 && request.baseDraftRevision === this.draftRevision,
      latencyMs: performance.now() - started,
    };
    if (request.baseDraftRevision !== this.draftRevision) {
      preview.conflicts.push({ code: "STALE_BASE_REVISION", message: "draft 已更新，请重新生成 preview", entityIds: [] });
    }
    this.previews.set(previewId, structuredClone(preview));
    return preview;
  }

  async commitEdit(request: EditCommandGroup): Promise<EditCommitResult> {
    this.assertSession(request.editSessionId);
    const preview = request.previewId ? this.previews.get(request.previewId) : undefined;
    if (!preview) throw new ClientContractError("PREVIEW_REQUIRED", "commit 必须引用有效 preview");
    if (!preview.committable || preview.conflicts.length > 0) throw new ClientContractError("PREVIEW_NOT_COMMITTABLE", "preview 含冲突，未写入 draft");
    if (request.baseDraftRevision !== this.draftRevision) throw new ClientContractError("STALE_BASE_REVISION", "draft revision 已变化", false, "重新加载并预览");

    this.undoStack.push(structuredClone(this.views));
    this.redoStack.length = 0;
    for (const command of preview.normalizedCommands) this.applyCommand(command);
    const newDraftRevision = this.nextDraftRevision();
    this.editSession = { ...this.editSession!, draftRevision: newDraftRevision, leaseExpiresAt: leaseTime() };
    this.project.draftRevision = newDraftRevision;
    const operationGroupId = `operation-${this.revisionNumber}`;
    this.appendEvent("jdp.revision.committed", { newDraftRevision, operationGroupId });
    this.persistServerState();
    return {
      newDraftRevision,
      operationGroupId,
      summary: request.label,
      changedEntityIds: preview.affectedObjectIds,
      validationDelta: { added: [], resolvedIds: [] },
      undoAvailable: true,
      redoAvailable: false,
      currentProjectRevisionChanged: false,
    };
  }

  async undo(editSessionId: string): Promise<EditCommitResult> {
    this.assertSession(editSessionId);
    const previous = this.undoStack.pop();
    if (!previous) throw new ClientContractError("UNDO_EMPTY", "没有可撤销的语义命令");
    this.redoStack.push(structuredClone(this.views));
    this.views = previous;
    return this.historyResult("撤销上一组语义命令", this.redoStack.length > 0);
  }

  async redo(editSessionId: string): Promise<EditCommitResult> {
    this.assertSession(editSessionId);
    const next = this.redoStack.pop();
    if (!next) throw new ClientContractError("REDO_EMPTY", "没有可重做的语义命令");
    this.undoStack.push(structuredClone(this.views));
    this.views = next;
    return this.historyResult("重做上一组语义命令", this.redoStack.length > 0);
  }

  async checkpoint(request: CheckpointRequest): Promise<CheckpointView> {
    this.assertSession(request.editSessionId);
    if (request.draftRevision !== this.draftRevision) throw new ClientContractError("STALE_BASE_REVISION", "检查点 revision 已过期");
    return { id: `checkpoint-${this.revisionNumber}`, name: request.name, revision: request.draftRevision, createdAt: new Date().toISOString() };
  }

  async publishDraft(request: PublishDraftRequest): Promise<RevisionView> {
    this.assertSession(request.editSessionId);
    if (request.draftRevision !== this.draftRevision) throw new ClientContractError("STALE_BASE_REVISION", "无法发布过期 draft");
    const parentId = this.project.candidateRevision;
    this.project.candidateRevision = request.draftRevision;
    this.project.maturity = "candidate";
    this.persistServerState();
    return { id: request.draftRevision, parentId, label: "Candidate（仍受工程、DFM、样板门禁阻止）" };
  }

  async proposeAiPatch(request: AiPatchRequest): Promise<PatchProposalView> {
    const stale = request.baseRevision !== this.draftRevision;
    return {
      id: "proposal-ai-003",
      baseRevision: request.baseRevision,
      status: stale ? "stale" : "ready",
      instruction: request.instruction,
      affectedNets: ["net-rs485-a", "net-rs485-b"],
      affectedRules: ["jlcpcb.2layer.clearance.v0"],
      findings: structuredClone(goldenFindings.filter((finding) => finding.view === "pcb")),
      hunks: [
        { id: "hunk-track", label: "NET_A 局部绕行", scope: "net", entityIds: ["track-rs485-a"], summary: "增加 0.25 mm 间距并保留拓扑", status: "pending", protectedImpact: false },
        { id: "hunk-connector", label: "保持 J1 锁定", scope: "object", entityIds: ["footprint-j1"], summary: "不移动人工锁定连接器", status: "pending", protectedImpact: true },
      ],
    };
  }

  async acceptPatchHunks(request: PatchDecision): Promise<RevisionView> {
    if (request.baseRevision !== this.draftRevision) throw new ClientContractError("STALE_PROPOSAL", "AI proposal 已过期", false, "rebase 或重新生成 proposal");
    if (request.acceptedHunkIds.includes("hunk-connector")) throw new ClientContractError("LOCKED_OBJECT_CONFLICT", "不能应用影响锁定 J1 的 hunk");
    const parentId = this.draftRevision;
    const id = this.nextDraftRevision();
    this.project.draftRevision = id;
    this.editSession = this.editSession ? { ...this.editSession, draftRevision: id, leaseExpiresAt: leaseTime() } : undefined;
    this.persistServerState();
    return { id, parentId, label: `接受 ${request.acceptedHunkIds.length} 个 hunk，拒绝 ${request.rejectedHunkIds.length} 个` };
  }

  async *events(aggregateId: string, afterSequence = 0): AsyncIterable<DurableEvent> {
    for (const event of this.eventLog) {
      if (event.aggregateId === aggregateId && event.sequence > afterSequence) yield structuredClone(event);
    }
  }

  async capabilities(): Promise<DesignCapabilities> {
    if (this.scenario === "version-mismatch") return { contractVersion: "0.1", mode: "mock", state: "version-mismatch", reason: "daemon requires JDP 0.2" };
    if (this.scenario === "unavailable") return { contractVersion: "0.1", mode: "mock", state: "unavailable", reason: "mock daemon unavailable scenario" };
    return { contractVersion: "0.1", mode: "mock", state: "connected", reason: "versioned development fixture" };
  }

  private get draftRevision(): string {
    return this.project.draftRevision ?? this.project.candidateRevision;
  }

  private nextDraftRevision(): string {
    this.revisionNumber += 1;
    return `rev-draft-${String(this.revisionNumber).padStart(3, "0")}`;
  }

  private assertAvailable(): void {
    if (this.scenario !== "normal") throw new ClientContractError("DESIGN_CLIENT_UNAVAILABLE", "Design client 当前不可用", true, "检查 daemon sidecar 与协议版本");
  }

  private assertSession(sessionId: string): void {
    this.assertAvailable();
    if (!this.editSession || this.editSession.id !== sessionId) throw new ClientContractError("EDIT_SESSION_REQUIRED", "需要先打开 server-backed edit session");
  }

  private conflictsFor(command: SemanticEditCommand): EditPreview["conflicts"] {
    if (command.kind === "design.set_edit_control") return [];
    const ids = affectedIds(command);
    return ids.filter((id) => this.policies.get(id) === "locked").map((id) => ({ code: "LOCKED_OBJECT_CONFLICT", message: `${id} 由人工锁定`, entityIds: [id] }));
  }

  private ghostFor(command: SemanticEditCommand): DisplayPrimitive[] {
    if (command.kind === "schematic.move_symbol" || command.kind === "pcb.move_footprint") {
      const view = command.kind.startsWith("schematic") ? "schematic" : "pcb";
      const original = this.views[view].find((item) => item.entityId === command.entityId);
      return original ? [{ ...structuredClone(original), id: `ghost-${original.id}`, position: command.position, color: "#69d2c1", zIndex: 100 }] : [];
    }
    if (command.kind === "pcb.route_segment") return [{ id: "ghost-track", entityId: "ghost-track", kind: "track", layerId: command.layerId, netId: command.netId, points: command.points, width: 7, color: "#69d2c1", zIndex: 100 }];
    if (command.kind === "pcb.add_via") return [{ id: "ghost-via", entityId: "ghost-via", kind: "via", layerId: "F.Cu", netId: command.netId, position: command.position, radius: 9, color: "#69d2c1", zIndex: 100 }];
    if (command.kind === "schematic.connect") {
      const from = this.focusFor("schematic", [command.fromEntityId]);
      const to = this.focusFor("schematic", [command.toEntityId]);
      return from && to ? [{ id: "ghost-wire", entityId: "ghost-wire", kind: "wire", layerId: "logical", points: [from, { x: to.x, y: from.y }, to], color: "#69d2c1", zIndex: 100 }] : [];
    }
    return [];
  }

  private applyCommand(command: SemanticEditCommand): void {
    if (command.kind === "schematic.move_symbol" || command.kind === "pcb.move_footprint") {
      const view = command.kind.startsWith("schematic") ? "schematic" : "pcb";
      const primitive = this.views[view].find((item) => item.entityId === command.entityId);
      if (primitive) primitive.position = structuredClone(command.position);
    } else if (command.kind === "pcb.route_segment") {
      this.views.pcb.push({ id: `track-${this.revisionNumber}`, entityId: `track-${this.revisionNumber}`, kind: "track", layerId: command.layerId, netId: command.netId, points: structuredClone(command.points), width: 7, color: command.layerId === "B.Cu" ? "#4b8fd6" : "#de6377", zIndex: 15 });
    } else if (command.kind === "pcb.add_via") {
      this.views.pcb.push({ id: `via-${this.revisionNumber}`, entityId: `via-${this.revisionNumber}`, kind: "via", layerId: "F.Cu", netId: command.netId, position: structuredClone(command.position), radius: 8, color: "#d8ba6d", zIndex: 35 });
    } else if (command.kind === "schematic.connect") {
      const points = this.ghostFor(command)[0]?.points;
      if (points) this.views.schematic.push({ id: `wire-${this.revisionNumber}`, entityId: `net-manual-${this.revisionNumber}`, kind: "wire", layerId: "logical", netId: `net-manual-${this.revisionNumber}`, points, color: "#58b6b2", zIndex: 10 });
    } else if (command.kind === "design.set_edit_control") {
      for (const entityId of command.entityIds) this.policies.set(entityId, command.mode);
    } else if (command.kind === "schematic.set_property") {
      objectProperties[command.entityId] = { ...objectProperties[command.entityId], [command.key]: command.value, provenance: "human" };
    }
  }

  private historyResult(summary: string, redoAvailable: boolean): EditCommitResult {
    const newDraftRevision = this.nextDraftRevision();
    this.project.draftRevision = newDraftRevision;
    this.editSession = { ...this.editSession!, draftRevision: newDraftRevision, leaseExpiresAt: leaseTime() };
    this.persistServerState();
    return { newDraftRevision, operationGroupId: `operation-${this.revisionNumber}`, summary, changedEntityIds: [], validationDelta: { added: [], resolvedIds: [] }, undoAvailable: this.undoStack.length > 0, redoAvailable, currentProjectRevisionChanged: false };
  }

  private restoreServerState(state: PersistedDesignServer): void {
    this.project = structuredClone(state.project);
    this.views = structuredClone(state.views);
    this.policies.clear();
    for (const [entityId, policy] of state.policies) this.policies.set(entityId, policy);
    this.undoStack.push(...structuredClone(state.undoStack));
    this.redoStack.push(...structuredClone(state.redoStack));
    this.eventLog.push(...structuredClone(state.eventLog));
    this.editSession = state.editSession ? structuredClone(state.editSession) : undefined;
    this.revisionNumber = state.revisionNumber;
    for (const key of Object.keys(objectProperties)) delete objectProperties[key];
    Object.assign(objectProperties, structuredClone(state.properties));
  }

  private persistServerState(): void {
    saveMockServerState("design", {
      schemaVersion: "0.1",
      project: this.project,
      views: this.views,
      policies: [...this.policies.entries()],
      undoStack: this.undoStack,
      redoStack: this.redoStack,
      eventLog: this.eventLog,
      editSession: this.editSession,
      revisionNumber: this.revisionNumber,
      properties: objectProperties,
    } satisfies PersistedDesignServer);
  }

  private relatedIds(id: string): string[] {
    if (id.startsWith("symbol-")) return [id, id.replace("symbol-", "footprint-")];
    if (id.startsWith("footprint-")) return [id, id.replace("footprint-", "symbol-")];
    const finding = goldenFindings.find((item) => item.id === id);
    if (finding) return [id, ...finding.entityIds];
    const related = [...this.views.schematic, ...this.views.pcb].filter((item) => item.netId === id).map((item) => item.entityId);
    return [id, ...related];
  }

  private focusFor(view: "schematic" | "pcb", ids: string[]): { x: number; y: number } | undefined {
    const primitive = this.views[view].find((item) => ids.includes(item.entityId));
    return primitive?.position ?? primitive?.points?.[0];
  }

  private appendEvent(type: string, data: unknown): void {
    this.eventLog.push({ schemaVersion: "0.1", aggregateId: this.project.id, sequence: this.eventLog.length + 1, timestamp: new Date().toISOString(), type, data });
  }
}

function affectedIds(command: SemanticEditCommand): string[] {
  switch (command.kind) {
    case "schematic.move_symbol":
    case "schematic.set_property":
    case "pcb.move_footprint":
      return [command.entityId];
    case "schematic.connect":
      return [command.fromEntityId, command.toEntityId];
    case "pcb.route_segment":
    case "pcb.add_via":
      return [command.netId];
    case "design.set_edit_control":
      return command.entityIds;
  }
}

function leaseTime(): string {
  return new Date(Date.now() + 30 * 60 * 1000).toISOString();
}
