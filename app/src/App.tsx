import { ApprovalDrawer } from "./components/ApprovalDrawer";
import { BottomPanel } from "./components/BottomPanel";
import { EditorArea } from "./components/EditorArea";
import { InspectorPanel } from "./components/InspectorPanel";
import { ProjectSidebar } from "./components/ProjectSidebar";
import { TopBar } from "./components/TopBar";
import { AlertCircleIcon, CheckCircleIcon, RefreshIcon, XIcon } from "./components/Icons";
import { useWorkbenchController } from "./hooks/useWorkbenchController";

export default function App() {
  const { state, actions } = useWorkbenchController();

  if (state.loading) {
    return (
      <div className="boot-screen">
        <span className="boot-mark"><RefreshIcon size={24} /></span>
        <strong>正在建立工作台投影</strong>
        <p>检查 versioned clients、sidecar 状态与草稿恢复游标</p>
      </div>
    );
  }

  return (
    <div className="workbench-app">
      <TopBar
        onCancel={() => void actions.cancelRun()}
        onCheckpoint={() => void actions.checkpoint()}
        onCreateProject={() => void actions.createProject()}
        onOpenProject={() => void actions.openProject()}
        onRedo={() => void actions.redo()}
        onRun={() => void actions.submitAgentInput("检查当前 candidate 的阻塞项并生成局部修复计划", "queue")}
        onUndo={() => void actions.undo()}
        project={state.project}
        redoAvailable={state.redoAvailable}
        runActive={state.run?.status === "running"}
        sidecars={state.sidecars}
        undoAvailable={state.undoAvailable}
      />

      <div className="connection-ribbon">
        <span className="mock-ribbon-label">VERSIONED MOCK · NOT REAL DAEMON EVIDENCE</span>
        <span>Mock Runtime {state.runtimeCapabilities?.state}</span>
        <span>Mock JDP {state.designCapabilities?.state}</span>
        <span className={`replay-state state-${state.replayState}`}><span className="status-dot" /> replay {state.replayState} · seq {state.lastRuntimeSequence}</span>
        <span className="ribbon-gap">真实 sidecar、ERC/DRC、制造能力尚未接入</span>
        <button className="ribbon-refresh" onClick={() => void actions.refreshSidecars()} title="按受控 manifest 重新检查 sidecar">
          <RefreshIcon size={12} />重检
        </button>
      </div>

      <div className="primary-workspace">
        <ProjectSidebar
          activeView={state.activeView}
          onSelectEntity={(entityId, view) => void actions.selectEntity(entityId, view)}
          onToggleLayer={actions.toggleLayer}
          project={state.project}
          visibleLayerIds={state.visibleLayerIds}
        />
        <EditorArea
          activeView={state.activeView}
          displayList={state.displayLists[state.activeView]}
          ghostPrimitives={state.preview?.ghostPrimitives ?? []}
          onCancelInteraction={actions.cancelInteraction}
          onIntent={actions.handleCanvasIntent}
          onToolChange={actions.changeTool}
          onViewChange={actions.changeView}
          onViewportChange={(viewport) => actions.changeViewport(state.activeView, viewport)}
          previewLatencyMs={state.preview?.latencyMs}
          relatedIds={state.relatedIds}
          selectedIds={state.selectedIds}
          toolMode={state.toolMode}
          viewport={state.viewports[state.activeView]}
        />
        <InspectorPanel
          onFocusFinding={actions.focusFinding}
          onSetPolicy={(ids, mode) => actions.setPolicy(ids, mode)}
          onSetProperty={(entityId, key, value) => actions.setProperty(entityId, key, value)}
          project={state.project}
          properties={state.objectProperties}
          selectedIds={state.selectedIds}
        />
      </div>

      <BottomPanel
        activeTab={state.bottomTab}
        events={state.runtimeEvents}
        onApplyPatch={(accepted, rejected) => void actions.applyPatch(accepted, rejected)}
        onFocusFinding={actions.focusFinding}
        onGeneratePatch={(instruction) => void actions.generatePatch(instruction)}
        onSubmit={(text, delivery) => void actions.submitAgentInput(text, delivery)}
        onTabChange={actions.setBottomTab}
        project={state.project}
        proposal={state.patchProposal}
        run={state.run}
      />

      <ApprovalDrawer approvals={state.approvals} onReply={(id, decision) => void actions.replyApproval(id, decision)} />

      {state.notice && (
        <div className="toast toast-success" role="status"><CheckCircleIcon size={16} /><span>{state.notice}</span><button onClick={actions.clearNotice} title="关闭"><XIcon size={14} /></button></div>
      )}
      {state.error && (
        <div className="toast toast-error" role="alert"><AlertCircleIcon size={16} /><span>{state.error}</span><button onClick={actions.clearError} title="关闭"><XIcon size={14} /></button></div>
      )}
    </div>
  );
}
