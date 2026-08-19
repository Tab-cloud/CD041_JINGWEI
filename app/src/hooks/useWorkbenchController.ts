import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";
import type {
  DesignClient,
  DesignViewKind,
  EditCommandGroup,
  EditPolicy,
  FindingView,
  Point,
  RuntimeClient,
  SemanticEditCommand,
  Viewport,
} from "../clients/contracts";
import { MockDesignClient, MockRuntimeClient } from "../mocks/v0.1";
import { loadRecovery, saveRecovery } from "../store/recovery";
import { initialWorkbenchState, type ToolMode, workbenchReducer } from "../store/workbenchState";
import { chooseProjectFolder, ensureSidecars, getSidecarStatuses } from "../utils/tauriBridge";
import type { CanvasIntent } from "../components/SemanticCanvas";

const PROJECT_ID = "project-golden-rs485";
const SESSION_ID = "session-main";

export function useWorkbenchController() {
  const runtime: RuntimeClient = useMemo(() => new MockRuntimeClient("normal"), []);
  const design: DesignClient = useMemo(() => new MockDesignClient("normal"), []);
  const [state, dispatch] = useReducer(workbenchReducer, initialWorkbenchState);
  const stateRef = useRef(state);
  const previewSequence = useRef(0);

  useEffect(() => {
    stateRef.current = state;
  }, [state]);

  const requestDisplay = useCallback(async (view: DesignViewKind, revision?: string) => {
    const snapshot = stateRef.current;
    const project = snapshot.project;
    if (!project) return;
    const displayList = await design.getView({
      projectId: project.id,
      revision: revision ?? project.draftRevision ?? project.candidateRevision,
      view,
      viewport: snapshot.viewports[view],
      visibleLayerIds: snapshot.visibleLayerIds,
      lod: snapshot.viewports[view].zoom < 0.7 ? 0 : 1,
    });
    dispatch({ type: "set-display", view, displayList });
  }, [design]);

  const replayRuntimeEvents = useCallback(async (afterSequence: number) => {
    let cursor = afterSequence;
    dispatch({ type: "replay-state", replayState: "reconnecting" });
    for (let attempt = 0; attempt < 2; attempt += 1) {
      try {
        for await (const event of runtime.events(SESSION_ID, cursor)) {
          cursor = event.sequence;
          dispatch({ type: "event", event });
        }
        dispatch({ type: "replay-state", replayState: "caught-up" });
        return;
      } catch (error) {
        if (attempt === 1) dispatch({ type: "error", error: errorMessage(error) });
      }
    }
  }, [runtime]);

  useEffect(() => {
    let cancelled = false;
    const initialize = async () => {
      try {
        const recovery = loadRecovery();
        const [project, runtimeCapabilities, designCapabilities, approvals, sidecars] = await Promise.all([
          design.getProject(PROJECT_ID),
          runtime.capabilities(),
          design.capabilities(),
          runtime.listApprovals(SESSION_ID),
          getSidecarStatuses(),
        ]);
        const editSession = await design.openEditSession({
          projectId: project.id,
          baseRevision: project.draftRevision ?? project.candidateRevision,
          recoverSessionId: recovery?.projectId === project.id ? recovery.editSessionId : undefined,
        });
        if (cancelled) return;
        dispatch({ type: "loaded", project, runtimeCapabilities, designCapabilities, approvals, editSession, sidecars });
        if (recovery) {
          dispatch({ type: "restore-cursor", lastRuntimeSequence: recovery.lastRuntimeSequence });
          dispatch({ type: "set-view", view: recovery.activeView });
          dispatch({ type: "set-viewport", view: "schematic", viewport: recovery.viewports.schematic });
          dispatch({ type: "set-viewport", view: "pcb", viewport: recovery.viewports.pcb });
          dispatch({ type: "notice", notice: editSession.recovered ? "已恢复 server-backed draft 与事件游标；本地 ghost 已丢弃。" : "恢复记录存在，但 server edit session 已失效。" });
        }
        void replayRuntimeEvents(recovery?.lastRuntimeSequence ?? 0);
      } catch (error) {
        if (!cancelled) dispatch({ type: "error", error: errorMessage(error) });
      }
    };
    void initialize();
    return () => { cancelled = true; };
  }, [design, replayRuntimeEvents, runtime]);

  useEffect(() => {
    if (!state.project) return;
    const timer = window.setTimeout(() => void requestDisplay(state.activeView), 35);
    return () => window.clearTimeout(timer);
  }, [requestDisplay, state.activeView, state.project, state.project?.draftRevision, state.viewports, state.visibleLayerIds]);

  useEffect(() => {
    if (!state.project || !state.editSession) return;
    saveRecovery({
      schemaVersion: "0.1",
      projectId: state.project.id,
      editSessionId: state.editSession.id,
      lastRuntimeSequence: state.lastRuntimeSequence,
      lastDesignSequence: 0,
      activeView: state.activeView,
      viewports: state.viewports,
      savedAt: new Date().toISOString(),
    });
  }, [state.activeView, state.editSession, state.lastRuntimeSequence, state.project, state.viewports]);

  const selectAt = useCallback(async (point: Point, additive = false): Promise<string[]> => {
    const snapshot = stateRef.current;
    const project = snapshot.project;
    if (!project) return [];
    const hit = await design.hitTest({
      projectId: project.id,
      revision: project.draftRevision ?? project.candidateRevision,
      view: snapshot.activeView,
      point,
      tolerance: 12 / snapshot.viewports[snapshot.activeView].zoom,
      visibleLayerIds: snapshot.visibleLayerIds,
    });
    const primary = hit.entityIds[0];
    const ids = primary ? (additive ? [...new Set([...snapshot.selectedIds, primary])] : [primary]) : [];
    if (ids.length === 0) {
      dispatch({ type: "select", ids: [] });
      dispatch({ type: "set-properties", properties: {} });
      return [];
    }
    const crossProbe = await design.crossProbe({ projectId: project.id, revision: project.draftRevision ?? project.candidateRevision, sourceView: snapshot.activeView, entityIds: ids });
    const properties = await design.query<Record<string, Record<string, string>>>({ projectId: project.id, revision: project.draftRevision ?? project.candidateRevision, query: "entity-properties", entityIds: ids });
    dispatch({ type: "select", ids, relatedIds: [...crossProbe.relatedEntityIds, ...crossProbe.netIds] });
    dispatch({ type: "set-properties", properties });
    return ids;
  }, [design]);

  const selectEntity = useCallback(async (entityId: string, view?: DesignViewKind) => {
    const snapshot = stateRef.current;
    const project = snapshot.project;
    if (!project) return;
    const targetView = view ?? snapshot.activeView;
    const crossProbe = await design.crossProbe({ projectId: project.id, revision: project.draftRevision ?? project.candidateRevision, sourceView: targetView, entityIds: [entityId] });
    const properties = await design.query<Record<string, Record<string, string>>>({ projectId: project.id, revision: project.draftRevision ?? project.candidateRevision, query: "entity-properties", entityIds: [entityId] });
    dispatch({ type: "set-view", view: targetView });
    dispatch({ type: "select", ids: [entityId], relatedIds: [...crossProbe.relatedEntityIds, ...crossProbe.netIds] });
    dispatch({ type: "set-properties", properties });
    const focus = crossProbe.focus[targetView];
    if (focus) {
      const viewport = snapshot.viewports[targetView];
      dispatch({ type: "set-viewport", view: targetView, viewport: { ...viewport, x: focus.x - viewport.width / viewport.zoom / 2, y: focus.y - viewport.height / viewport.zoom / 2 } });
    }
  }, [design]);

  const previewCommand = useCallback(async (command: SemanticEditCommand, label: string) => {
    const snapshot = stateRef.current;
    if (!snapshot.editSession) return undefined;
    const sequence = ++previewSequence.current;
    const preview = await design.previewEdit(commandGroup(snapshot.editSession.id, snapshot.editSession.draftRevision, command, label));
    if (sequence === previewSequence.current) dispatch({ type: "set-preview", preview });
    return preview;
  }, [design]);

  const commitCommand = useCallback(async (command: SemanticEditCommand, label: string) => {
    const snapshot = stateRef.current;
    if (!snapshot.editSession) return;
    try {
      const group = commandGroup(snapshot.editSession.id, snapshot.editSession.draftRevision, command, label);
      const preview = await design.previewEdit(group);
      dispatch({ type: "set-preview", preview });
      if (!preview.committable) {
        dispatch({ type: "error", error: preview.conflicts.map((conflict) => `${conflict.code}: ${conflict.message}`).join("；") || "preview 不可提交" });
        return;
      }
      const result = await design.commitEdit({ ...group, previewId: preview.previewId });
      dispatch({ type: "commit-result", draftRevision: result.newDraftRevision, undoAvailable: result.undoAvailable, redoAvailable: result.redoAvailable, notice: `${result.summary}；current revision 未改变。` });
      await requestDisplay(snapshot.activeView, result.newDraftRevision);
    } catch (error) {
      dispatch({ type: "error", error: errorMessage(error) });
    }
  }, [design, requestDisplay]);

  const handleCanvasIntent = useCallback((intent: CanvasIntent) => {
    const snapshot = stateRef.current;
    const mode = snapshot.toolMode;
    const view = snapshot.activeView;
    if (mode === "select" && intent.phase === "up") {
      void selectAt(intent.point, intent.shiftKey);
      return;
    }
    if (mode === "move") {
      if (intent.phase === "down") {
        void selectAt(intent.point).then((ids) => {
          const entityId = ids[0];
          const valid = entityId && (view === "schematic" ? entityId.startsWith("symbol-") : entityId.startsWith("footprint-"));
          if (valid) dispatch({ type: "interaction-start", interaction: { point: intent.point, entityId } });
          else dispatch({ type: "error", error: view === "schematic" ? "移动工具需要选择 symbol" : "移动工具需要选择 footprint" });
        });
        return;
      }
      if (!snapshot.interactionStart?.entityId) return;
      const command = view === "schematic"
        ? { kind: "schematic.move_symbol", entityId: snapshot.interactionStart.entityId, position: intent.point } satisfies SemanticEditCommand
        : { kind: "pcb.move_footprint", entityId: snapshot.interactionStart.entityId, position: intent.point, rotation: 0 } satisfies SemanticEditCommand;
      if (intent.phase === "move") void previewCommand(command, `移动 ${snapshot.interactionStart.entityId}`);
      if (intent.phase === "up") void commitCommand(command, `移动 ${snapshot.interactionStart.entityId}`);
      return;
    }
    if (mode === "connect" && view === "schematic") {
      if (intent.phase === "down" && !snapshot.interactionStart) {
        void selectAt(intent.point).then((ids) => {
          if (ids[0]) dispatch({ type: "interaction-start", interaction: { point: intent.point, entityId: ids[0] } });
        });
        return;
      }
      if (!snapshot.interactionStart?.entityId) return;
      if (intent.phase === "move") {
        const target = snapshot.relatedIds.find((id) => id.startsWith("pin-")) ?? "pin-u2-i2c";
        void previewCommand({ kind: "schematic.connect", fromEntityId: snapshot.interactionStart.entityId, toEntityId: target }, "连接 wire");
      }
      if (intent.phase === "down") {
        void selectAt(intent.point).then((ids) => {
          const target = ids[0];
          if (target) void commitCommand({ kind: "schematic.connect", fromEntityId: snapshot.interactionStart!.entityId!, toEntityId: target }, "连接 wire");
        });
      }
      return;
    }
    if (mode === "route" && view === "pcb") {
      if (intent.phase === "down" && !snapshot.interactionStart) {
        const netId = snapshot.relatedIds.find((id) => id.startsWith("net-")) ?? "net-rs485-a";
        dispatch({ type: "interaction-start", interaction: { point: intent.point, entityId: netId } });
        return;
      }
      if (!snapshot.interactionStart?.entityId) return;
      const command = { kind: "pcb.route_segment", netId: snapshot.interactionStart.entityId, layerId: "F.Cu", points: [snapshot.interactionStart.point, { x: intent.point.x, y: snapshot.interactionStart.point.y }, intent.point] } satisfies SemanticEditCommand;
      if (intent.phase === "move") void previewCommand(command, "绘制基础 track");
      if (intent.phase === "down") void commitCommand(command, "绘制基础 track");
      return;
    }
    if (mode === "via" && view === "pcb" && intent.phase === "down") {
      const netId = snapshot.relatedIds.find((id) => id.startsWith("net-")) ?? "net-rs485-a";
      void commitCommand({ kind: "pcb.add_via", netId, position: intent.point, viaClassId: "via-default" }, "放置基础 via");
    }
  }, [commitCommand, previewCommand, selectAt]);

  const cancelInteraction = useCallback(() => {
    previewSequence.current += 1;
    dispatch({ type: "interaction-start", interaction: undefined });
    dispatch({ type: "set-preview", preview: undefined });
    dispatch({ type: "notice", notice: "已丢弃本地 transient ghost；没有写入 draft。" });
  }, []);

  const changeView = useCallback((view: DesignViewKind) => dispatch({ type: "set-view", view }), []);
  const changeViewport = useCallback((view: DesignViewKind, viewport: Viewport) => dispatch({ type: "set-viewport", view, viewport }), []);
  const toggleLayer = useCallback((layerId: string) => dispatch({ type: "toggle-layer", layerId }), []);
  const changeTool = useCallback((toolMode: ToolMode) => dispatch({ type: "set-tool", toolMode }), []);

  const setProperty = useCallback((entityId: string, key: string, value: string) => {
    void commitCommand({ kind: "schematic.set_property", entityId, key, value }, `设置 ${entityId}.${key}`);
  }, [commitCommand]);

  const setPolicy = useCallback((entityIds: string[], mode: EditPolicy) => {
    void commitCommand({ kind: "design.set_edit_control", entityIds, mode }, `设置 edit control: ${mode}`);
  }, [commitCommand]);

  const undo = useCallback(async () => {
    const session = stateRef.current.editSession;
    if (!session) return;
    try {
      const result = await design.undo(session.id);
      dispatch({ type: "commit-result", draftRevision: result.newDraftRevision, undoAvailable: result.undoAvailable, redoAvailable: result.redoAvailable, notice: result.summary });
      await requestDisplay(stateRef.current.activeView, result.newDraftRevision);
    } catch (error) { dispatch({ type: "error", error: errorMessage(error) }); }
  }, [design, requestDisplay]);

  const redo = useCallback(async () => {
    const session = stateRef.current.editSession;
    if (!session) return;
    try {
      const result = await design.redo(session.id);
      dispatch({ type: "commit-result", draftRevision: result.newDraftRevision, undoAvailable: result.undoAvailable, redoAvailable: result.redoAvailable, notice: result.summary });
      await requestDisplay(stateRef.current.activeView, result.newDraftRevision);
    } catch (error) { dispatch({ type: "error", error: errorMessage(error) }); }
  }, [design, requestDisplay]);

  const checkpoint = useCallback(async () => {
    const session = stateRef.current.editSession;
    if (!session) return;
    try {
      const result = await design.checkpoint({ editSessionId: session.id, draftRevision: session.draftRevision, name: `人工检查点 ${new Date().toLocaleTimeString("zh-CN", { hour12: false })}` });
      dispatch({ type: "notice", notice: `已创建 ${result.name}（${result.revision}）；current revision 未改变。` });
    } catch (error) { dispatch({ type: "error", error: errorMessage(error) }); }
  }, [design]);

  const submitAgentInput = useCallback(async (text: string, delivery: "steer" | "queue") => {
    try {
      await runtime.admitInput({ sessionId: SESSION_ID, text, attachmentRefs: [], delivery, clientRequestId: crypto.randomUUID() });
      const run = await runtime.resumeRun(SESSION_ID);
      dispatch({ type: "set-run", run });
      void replayRuntimeEvents(stateRef.current.lastRuntimeSequence);
    } catch (error) { dispatch({ type: "error", error: errorMessage(error) }); }
  }, [replayRuntimeEvents, runtime]);

  const cancelRun = useCallback(async () => {
    const run = stateRef.current.run;
    if (!run) return;
    try {
      await runtime.cancelRun(run.id, "用户从工作台取消");
      dispatch({ type: "set-run", run: { ...run, status: "cancelled" } });
    } catch (error) { dispatch({ type: "error", error: errorMessage(error) }); }
  }, [runtime]);

  const replyApproval = useCallback(async (approvalId: string, decision: "approve-once" | "deny") => {
    try {
      await runtime.replyApproval({ approvalId, decision });
      dispatch({ type: "set-approvals", approvals: await runtime.listApprovals(SESSION_ID) });
      dispatch({ type: "notice", notice: decision === "approve-once" ? "审批已持久化；仍需 daemon 二次授权。" : "审批已拒绝；run 不会无限等待。" });
      void replayRuntimeEvents(stateRef.current.lastRuntimeSequence);
    } catch (error) { dispatch({ type: "error", error: errorMessage(error) }); }
  }, [replayRuntimeEvents, runtime]);

  const focusFinding = useCallback((finding: FindingView) => void selectEntity(finding.id, finding.view), [selectEntity]);

  const generatePatch = useCallback(async (instruction: string) => {
    const snapshot = stateRef.current;
    if (!snapshot.project) return;
    try {
      const proposal = await design.proposeAiPatch({ projectId: snapshot.project.id, baseRevision: snapshot.project.draftRevision ?? snapshot.project.candidateRevision, instruction, entityIds: snapshot.selectedIds });
      dispatch({ type: "set-patch", patchProposal: proposal });
    } catch (error) { dispatch({ type: "error", error: errorMessage(error) }); }
  }, [design]);

  const applyPatch = useCallback(async (accepted: string[], rejected: string[]) => {
    const proposal = stateRef.current.patchProposal;
    if (!proposal) return;
    try {
      const revision = await design.acceptPatchHunks({ proposalId: proposal.id, baseRevision: proposal.baseRevision, acceptedHunkIds: accepted, rejectedHunkIds: rejected });
      dispatch({ type: "commit-result", draftRevision: revision.id, undoAvailable: true, redoAvailable: false, notice: revision.label });
      await requestDisplay(stateRef.current.activeView, revision.id);
    } catch (error) { dispatch({ type: "error", error: errorMessage(error) }); }
  }, [design, requestDisplay]);

  const createProject = useCallback(async () => {
    try {
      const session = await runtime.createSession({ projectId: PROJECT_ID, title: "新建 mock 设计会话" });
      dispatch({ type: "notice", notice: `已创建 ${session.title}；mock client 仍指向黄金板 fixture。` });
    } catch (error) { dispatch({ type: "error", error: errorMessage(error) }); }
  }, [runtime]);

  const openProject = useCallback(async () => {
    const selected = await chooseProjectFolder();
    dispatch({ type: "notice", notice: selected ? "已完成受控目录选择；mock transport 不读取该目录，继续显示黄金板 fixture。" : "已取消打开项目。" });
  }, []);

  const refreshSidecars = useCallback(async () => {
    try {
      dispatch({ type: "set-sidecars", sidecars: await ensureSidecars() });
      dispatch({ type: "notice", notice: "已按受控 manifest 重新检查 sidecar；本开发包不会伪装真实进程。" });
    } catch (error) {
      dispatch({ type: "error", error: errorMessage(error) });
    }
  }, []);

  return {
    state,
    actions: {
      changeView,
      changeViewport,
      toggleLayer,
      changeTool,
      handleCanvasIntent,
      cancelInteraction,
      selectEntity,
      setProperty,
      setPolicy,
      undo,
      redo,
      checkpoint,
      submitAgentInput,
      cancelRun,
      replyApproval,
      focusFinding,
      generatePatch,
      applyPatch,
      createProject,
      openProject,
      refreshSidecars,
      setBottomTab: (bottomTab: typeof state.bottomTab) => dispatch({ type: "set-bottom-tab", bottomTab }),
      clearNotice: () => dispatch({ type: "notice", notice: undefined }),
      clearError: () => dispatch({ type: "error", error: undefined }),
    },
  };
}

function commandGroup(editSessionId: string, baseDraftRevision: string, command: SemanticEditCommand, label: string): EditCommandGroup {
  return { editSessionId, baseDraftRevision, label, commands: [command], expectedPostconditions: ["currentProjectRevisionChanged=false"] };
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  return "发生未知错误";
}
