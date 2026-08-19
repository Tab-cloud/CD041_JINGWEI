import type {
  ApprovalView,
  DesignCapabilities,
  DesignViewKind,
  DisplayList,
  DurableEvent,
  EditPreview,
  EditSessionView,
  PatchProposalView,
  Point,
  ProjectView,
  RunView,
  RuntimeCapabilities,
  Viewport,
} from "../clients/contracts";

export type ToolMode = "select" | "move" | "connect" | "route" | "via";
export type BottomTab = "agent" | "timeline" | "findings" | "diff" | "history";

export interface SidecarStatus {
  kind: "runtime" | "daemon";
  state: "healthy" | "starting" | "unavailable" | "version-mismatch" | "crashed";
  version?: string;
  reason?: string;
}

export interface WorkbenchState {
  loading: boolean;
  project?: ProjectView;
  activeView: DesignViewKind;
  viewports: Record<DesignViewKind, Viewport>;
  displayLists: Partial<Record<DesignViewKind, DisplayList>>;
  visibleLayerIds: string[];
  selectedIds: string[];
  relatedIds: string[];
  objectProperties: Record<string, Record<string, string>>;
  toolMode: ToolMode;
  interactionStart?: { point: Point; entityId?: string };
  editSession?: EditSessionView;
  preview?: EditPreview;
  run?: RunView;
  approvals: ApprovalView[];
  runtimeCapabilities?: RuntimeCapabilities;
  designCapabilities?: DesignCapabilities;
  runtimeEvents: DurableEvent[];
  lastRuntimeSequence: number;
  replayState: "idle" | "reconnecting" | "caught-up";
  bottomTab: BottomTab;
  patchProposal?: PatchProposalView;
  sidecars: SidecarStatus[];
  undoAvailable: boolean;
  redoAvailable: boolean;
  notice?: string;
  error?: string;
}

export type WorkbenchAction =
  | { type: "loaded"; project: ProjectView; runtimeCapabilities: RuntimeCapabilities; designCapabilities: DesignCapabilities; approvals: ApprovalView[]; editSession: EditSessionView; sidecars: SidecarStatus[] }
  | { type: "set-view"; view: DesignViewKind }
  | { type: "set-display"; view: DesignViewKind; displayList: DisplayList }
  | { type: "set-viewport"; view: DesignViewKind; viewport: Viewport }
  | { type: "toggle-layer"; layerId: string }
  | { type: "select"; ids: string[]; relatedIds?: string[] }
  | { type: "set-properties"; properties: Record<string, Record<string, string>> }
  | { type: "set-tool"; toolMode: ToolMode }
  | { type: "interaction-start"; interaction?: WorkbenchState["interactionStart"] }
  | { type: "set-preview"; preview?: EditPreview }
  | { type: "commit-result"; draftRevision: string; undoAvailable: boolean; redoAvailable: boolean; notice: string }
  | { type: "set-run"; run: RunView }
  | { type: "set-approvals"; approvals: ApprovalView[] }
  | { type: "event"; event: DurableEvent }
  | { type: "restore-cursor"; lastRuntimeSequence: number }
  | { type: "replay-state"; replayState: WorkbenchState["replayState"] }
  | { type: "set-bottom-tab"; bottomTab: BottomTab }
  | { type: "set-patch"; patchProposal?: PatchProposalView }
  | { type: "set-sidecars"; sidecars: SidecarStatus[] }
  | { type: "notice"; notice?: string }
  | { type: "error"; error?: string };

export const initialWorkbenchState: WorkbenchState = {
  loading: true,
  activeView: "schematic",
  viewports: {
    schematic: { x: 80, y: 70, width: 1100, height: 650, zoom: 1 },
    pcb: { x: 80, y: 55, width: 1100, height: 650, zoom: 1 },
  },
  displayLists: {},
  visibleLayerIds: ["logical", "F.Cu", "B.Cu", "F.Silkscreen", "Edge.Cuts", "findings"],
  selectedIds: [],
  relatedIds: [],
  objectProperties: {},
  toolMode: "select",
  approvals: [],
  runtimeEvents: [],
  lastRuntimeSequence: 0,
  replayState: "idle",
  bottomTab: "agent",
  sidecars: [],
  undoAvailable: false,
  redoAvailable: false,
};

export function workbenchReducer(state: WorkbenchState, action: WorkbenchAction): WorkbenchState {
  switch (action.type) {
    case "loaded":
      return {
        ...state,
        loading: false,
        project: action.project,
        runtimeCapabilities: action.runtimeCapabilities,
        designCapabilities: action.designCapabilities,
        approvals: action.approvals,
        editSession: action.editSession,
        sidecars: action.sidecars,
        visibleLayerIds: action.project.layers.filter((layer) => layer.visible).map((layer) => layer.id),
      };
    case "set-view":
      return { ...state, activeView: action.view, selectedIds: [], relatedIds: [], preview: undefined, interactionStart: undefined };
    case "set-display":
      return { ...state, displayLists: { ...state.displayLists, [action.view]: action.displayList } };
    case "set-viewport":
      return { ...state, viewports: { ...state.viewports, [action.view]: action.viewport } };
    case "toggle-layer":
      return {
        ...state,
        visibleLayerIds: state.visibleLayerIds.includes(action.layerId)
          ? state.visibleLayerIds.filter((id) => id !== action.layerId)
          : [...state.visibleLayerIds, action.layerId],
      };
    case "select":
      return { ...state, selectedIds: action.ids, relatedIds: action.relatedIds ?? [] };
    case "set-properties":
      return { ...state, objectProperties: action.properties };
    case "set-tool":
      return { ...state, toolMode: action.toolMode, interactionStart: undefined, preview: undefined };
    case "interaction-start":
      return { ...state, interactionStart: action.interaction };
    case "set-preview":
      return { ...state, preview: action.preview };
    case "commit-result":
      return {
        ...state,
        project: state.project ? { ...state.project, draftRevision: action.draftRevision } : undefined,
        editSession: state.editSession ? { ...state.editSession, draftRevision: action.draftRevision } : undefined,
        preview: undefined,
        interactionStart: undefined,
        undoAvailable: action.undoAvailable,
        redoAvailable: action.redoAvailable,
        notice: action.notice,
        error: undefined,
      };
    case "set-run":
      return { ...state, run: action.run, bottomTab: "timeline" };
    case "set-approvals":
      return { ...state, approvals: action.approvals };
    case "event":
      if (action.event.sequence <= state.lastRuntimeSequence) return state;
      return { ...state, runtimeEvents: [...state.runtimeEvents, action.event], lastRuntimeSequence: action.event.sequence };
    case "restore-cursor":
      return { ...state, lastRuntimeSequence: Math.max(state.lastRuntimeSequence, action.lastRuntimeSequence) };
    case "replay-state":
      return { ...state, replayState: action.replayState };
    case "set-bottom-tab":
      return { ...state, bottomTab: action.bottomTab };
    case "set-patch":
      return { ...state, patchProposal: action.patchProposal, bottomTab: action.patchProposal ? "diff" : state.bottomTab };
    case "set-sidecars":
      return { ...state, sidecars: action.sidecars };
    case "notice":
      return { ...state, notice: action.notice, error: undefined };
    case "error":
      return { ...state, error: action.error, notice: undefined };
  }
}
