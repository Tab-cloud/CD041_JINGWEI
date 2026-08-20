/**
 * UI-facing facade shapes for mock contract version 0.1.
 * These are deliberately presentation projections, not a handwritten copy of
 * the normative JDP wire schema. The real adapters must map generated bindings
 * into these views during integration.
 */

export const MOCK_CONTRACT_VERSION = "0.1" as const;

export type ConnectionMode = "mock" | "real";
export type ConnectionState = "connected" | "reconnecting" | "unavailable" | "version-mismatch";
export type MaturityStage =
  | "draft"
  | "candidate"
  | "engineering-verified"
  | "manufacturing-candidate"
  | "prototype-verified"
  | "production-released";
export type EditPolicy = "ai-editable" | "ask-before-change" | "locked";
export type DesignViewKind = "schematic" | "pcb";
export type Severity = "error" | "warning" | "info";

export interface Point {
  x: number;
  y: number;
}

export interface Viewport {
  x: number;
  y: number;
  width: number;
  height: number;
  zoom: number;
}

export interface DurableEvent<T = unknown> {
  schemaVersion: typeof MOCK_CONTRACT_VERSION;
  aggregateId: string;
  sequence: number;
  timestamp: string;
  sessionId?: string;
  runId?: string;
  type: string;
  data: T;
}

export interface SessionView {
  id: string;
  title: string;
  projectId: string;
  status: "idle" | "running" | "waiting" | "failed";
  lastSequence: number;
}

export interface CreateSessionInput {
  projectId: string;
  title: string;
}

export interface AdmitInput {
  sessionId: string;
  text: string;
  attachmentRefs: string[];
  delivery: "steer" | "queue";
  clientRequestId: string;
}

export interface AdmittedInput {
  inputId: string;
  admittedSequence: number;
  delivery: "steer" | "queue";
}

export interface ToolTimelineItem {
  id: string;
  parentRunId?: string;
  kind: "run" | "model" | "tool" | "approval" | "message";
  title: string;
  detail: string;
  target?: string;
  effect?: string;
  status: "queued" | "running" | "waiting" | "succeeded" | "failed" | "denied" | "cancelled";
  startedAt: string;
  durationMs?: number;
  error?: {
    code: string;
    summary: string;
    suggestedAction: string;
  };
}

export interface RunView {
  id: string;
  sessionId: string;
  title: string;
  status: "running" | "waiting" | "completed" | "failed" | "cancelled";
  timeline: ToolTimelineItem[];
  output?: string;
}

export interface ApprovalView {
  id: string;
  sessionId: string;
  toolName: string;
  target: string;
  reason: string;
  effect: string;
  redactedInput: string;
  scope: string;
  expiresAt: string;
  status: "pending" | "approved" | "denied" | "expired";
}

export interface ApprovalReply {
  approvalId: string;
  decision: "approve-once" | "deny";
}

export interface ArtifactView {
  id: string;
  name: string;
  mediaType: string;
  revision: string;
  summary: string;
}

export interface RuntimeCapabilities {
  contractVersion: typeof MOCK_CONTRACT_VERSION;
  mode: ConnectionMode;
  state: ConnectionState;
  replay: boolean;
  approvals: boolean;
  providers: Array<{ provider: string; model: string }>;
  reason?: string;
}

export interface RuntimeClient {
  createSession(input: CreateSessionInput): Promise<SessionView>;
  admitInput(input: AdmitInput): Promise<AdmittedInput>;
  resumeRun(sessionId: string): Promise<RunView>;
  cancelRun(runId: string, reason?: string): Promise<void>;
  events(sessionId: string, afterSequence?: number): AsyncIterable<DurableEvent>;
  listApprovals(sessionId?: string): Promise<ApprovalView[]>;
  replyApproval(input: ApprovalReply): Promise<void>;
  getArtifact(artifactId: string): Promise<ArtifactView>;
  capabilities(): Promise<RuntimeCapabilities>;
}

export interface LayerView {
  id: string;
  name: string;
  color: string;
  visible: boolean;
  category: "copper" | "silkscreen" | "outline" | "logical" | "overlay";
}

export interface FindingView {
  id: string;
  code: string;
  title: string;
  detail: string;
  severity: Severity;
  view: DesignViewKind;
  entityIds: string[];
  location: Point;
  rule: string;
  firstSeenRevision: string;
  evidenceRefs: string[];
  status: "open" | "resolved" | "waived";
}

export interface ProjectView {
  id: string;
  name: string;
  currentRevision: string;
  candidateRevision: string;
  draftRevision?: string;
  maturity: MaturityStage;
  stages: Array<{ id: string; label: string; status: "complete" | "current" | "blocked" }>;
  releaseBlockers: string[];
  sheets: Array<{ id: string; name: string; childCount: number }>;
  layers: LayerView[];
  findings: FindingView[];
}

export type PrimitiveKind =
  | "symbol"
  | "pin"
  | "wire"
  | "label"
  | "junction"
  | "board-outline"
  | "footprint"
  | "pad"
  | "ratsnest"
  | "track"
  | "via"
  | "zone"
  | "text"
  | "finding";

export interface DisplayPrimitive {
  id: string;
  entityId: string;
  kind: PrimitiveKind;
  layerId: string;
  netId?: string;
  position?: Point;
  width?: number;
  height?: number;
  radius?: number;
  points?: Point[];
  label?: string;
  color?: string;
  rotation?: number;
  locked?: boolean;
  severity?: Severity;
  zIndex: number;
}

export interface ViewRequest {
  projectId: string;
  revision: string;
  view: DesignViewKind;
  viewport: Viewport;
  visibleLayerIds: string[];
  lod: number;
}

export interface DisplayList {
  revision: string;
  view: DesignViewKind;
  primitives: DisplayPrimitive[];
  totalPrimitives: number;
  incrementalToken: string;
}

export interface HitTestRequest {
  projectId: string;
  revision: string;
  view: DesignViewKind;
  point: Point;
  tolerance: number;
  visibleLayerIds: string[];
}

export interface HitTestResult {
  entityIds: string[];
  snap?: { point: Point; kind: "grid" | "pin" | "pad" | "track" };
  latencyMs: number;
}

export interface CrossProbeRequest {
  projectId: string;
  revision: string;
  sourceView: DesignViewKind | "bom" | "finding";
  entityIds: string[];
}

export interface CrossProbeResult {
  entityIds: string[];
  relatedEntityIds: string[];
  netIds: string[];
  focus: Partial<Record<DesignViewKind, Point>>;
}

export interface OpenEditSessionRequest {
  projectId: string;
  baseRevision: string;
  recoverSessionId?: string;
}

export interface EditSessionView {
  id: string;
  baseRevision: string;
  draftRevision: string;
  leaseExpiresAt: string;
  recovered: boolean;
}

export type SemanticEditCommand =
  | { kind: "schematic.move_symbol"; entityId: string; position: Point }
  | { kind: "schematic.connect"; fromEntityId: string; toEntityId: string }
  | { kind: "schematic.set_property"; entityId: string; key: string; value: string }
  | { kind: "pcb.move_footprint"; entityId: string; position: Point; rotation: number }
  | { kind: "pcb.route_segment"; netId: string; layerId: string; points: Point[] }
  | { kind: "pcb.add_via"; netId: string; position: Point; viaClassId: string }
  | { kind: "design.set_edit_control"; entityIds: string[]; mode: EditPolicy };

export interface EditCommandGroup {
  editSessionId: string;
  baseDraftRevision: string;
  label: string;
  commands: SemanticEditCommand[];
  expectedPostconditions: string[];
  previewId?: string;
}

export interface EditPreview {
  previewId: string;
  baseDraftRevision: string;
  normalizedCommands: SemanticEditCommand[];
  ghostPrimitives: DisplayPrimitive[];
  affectedObjectIds: string[];
  conflicts: Array<{ code: string; message: string; entityIds: string[] }>;
  findings: FindingView[];
  committable: boolean;
  latencyMs: number;
}

export interface EditCommitResult {
  newDraftRevision: string;
  operationGroupId: string;
  summary: string;
  changedEntityIds: string[];
  validationDelta: { added: FindingView[]; resolvedIds: string[] };
  undoAvailable: boolean;
  redoAvailable: boolean;
  currentProjectRevisionChanged: false;
}

export interface CheckpointRequest {
  editSessionId: string;
  draftRevision: string;
  name: string;
}

export interface CheckpointView {
  id: string;
  name: string;
  revision: string;
  createdAt: string;
}

export interface PublishDraftRequest {
  editSessionId: string;
  draftRevision: string;
}

export interface RevisionView {
  id: string;
  parentId: string;
  label: string;
}

export interface DesignQuery {
  projectId: string;
  revision: string;
  entityIds?: string[];
  query: "entity-properties" | "history" | "selection-context";
}

export interface AiPatchRequest {
  projectId: string;
  baseRevision: string;
  instruction: string;
  entityIds: string[];
}

export interface PatchHunk {
  id: string;
  label: string;
  scope: "object" | "net" | "module" | "region";
  entityIds: string[];
  summary: string;
  status: "pending" | "accepted" | "rejected";
  protectedImpact: boolean;
}

export interface PatchProposalView {
  id: string;
  baseRevision: string;
  status: "ready" | "stale" | "conflict";
  instruction: string;
  affectedNets: string[];
  affectedRules: string[];
  findings: FindingView[];
  hunks: PatchHunk[];
}

export interface PatchDecision {
  proposalId: string;
  baseRevision: string;
  acceptedHunkIds: string[];
  rejectedHunkIds: string[];
}

export interface DesignCapabilities {
  contractVersion: typeof MOCK_CONTRACT_VERSION;
  mode: ConnectionMode;
  state: ConnectionState;
  reason?: string;
}

export interface DesignClient {
  getProject(projectId: string): Promise<ProjectView>;
  query<T>(request: DesignQuery): Promise<T>;
  getView(request: ViewRequest): Promise<DisplayList>;
  hitTest(request: HitTestRequest): Promise<HitTestResult>;
  crossProbe(request: CrossProbeRequest): Promise<CrossProbeResult>;
  openEditSession(request: OpenEditSessionRequest): Promise<EditSessionView>;
  previewEdit(request: EditCommandGroup): Promise<EditPreview>;
  commitEdit(request: EditCommandGroup): Promise<EditCommitResult>;
  undo(editSessionId: string): Promise<EditCommitResult>;
  redo(editSessionId: string): Promise<EditCommitResult>;
  checkpoint(request: CheckpointRequest): Promise<CheckpointView>;
  publishDraft(request: PublishDraftRequest): Promise<RevisionView>;
  proposeAiPatch(request: AiPatchRequest): Promise<PatchProposalView>;
  acceptPatchHunks(request: PatchDecision): Promise<RevisionView>;
  events(aggregateId: string, afterSequence?: number): AsyncIterable<DurableEvent>;
  capabilities(): Promise<DesignCapabilities>;
}
