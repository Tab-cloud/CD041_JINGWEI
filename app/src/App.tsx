import { useState } from "react";
import { ApprovalDrawer } from "./components/ApprovalDrawer";
import { BottomPanel } from "./components/BottomPanel";
import { EditorArea } from "./components/EditorArea";
import { InspectorPanel } from "./components/InspectorPanel";
import { ProjectSidebar } from "./components/ProjectSidebar";
import { TopBar } from "./components/TopBar";
import { ModelSettingsDrawer } from "./components/ModelSettingsDrawer";
import { AlertCircleIcon, CheckCircleIcon, RefreshIcon, XIcon } from "./components/Icons";
import { useWorkbenchController } from "./hooks/useWorkbenchController";
import { useTheme } from "./theme/useTheme";

export default function App() {
  const { state, actions } = useWorkbenchController();
  const [modelSettingsOpen, setModelSettingsOpen] = useState(false);
  const { theme, toggleTheme } = useTheme();
  const sidecarsHealthy = state.sidecars.length > 0 && state.sidecars.every((sidecar) => sidecar.state === "healthy");
  const realData = state.runtimeCapabilities?.mode === "real" && state.designCapabilities?.mode === "real";
  const providerReady = (state.runtimeCapabilities?.providers.length ?? 0) > 0;

  if (state.loading) {
    return (
      <div className="boot-screen">
        <span className="boot-mark"><RefreshIcon size={24} /></span>
        <strong>正在建立工作台投影</strong>
        <p>检查 versioned clients、sidecar 状态与草稿恢复游标</p>
      </div>
    );
  }

  if (!state.project) {
    return (
      <div className="boot-screen" role="alert">
        <span className="boot-mark"><AlertCircleIcon size={24} /></span>
        <strong>真实工作台连接失败</strong>
        <p>{state.error ?? "未获得 project projection"}</p>
        <p>浏览器开发预览必须显式使用 <code>?fixture=1</code>；安装版不会自动回退 fixture。</p>
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
        onOpenModelSettings={() => setModelSettingsOpen(true)}
        onToggleTheme={toggleTheme}
        onRedo={() => void actions.redo()}
        onRun={() => void actions.submitAgentInput("检查当前 candidate 的阻塞项并生成局部修复计划", "queue")}
        onUndo={() => void actions.undo()}
        project={state.project}
        redoAvailable={state.redoAvailable}
        runActive={state.run?.status === "running"}
        sidecars={state.sidecars}
        theme={theme}
        undoAvailable={state.undoAvailable}
        realData={realData}
        providerReady={providerReady}
      />

      <div className="connection-ribbon">
        <span className="mock-ribbon-label">{realData ? "REAL LOCAL RPC · JIR 0.2" : "EXPLICIT FIXTURE MODE · NOT DAEMON EVIDENCE"}</span>
        <span>{state.runtimeCapabilities?.mode === "real" ? "Runtime RPC" : "Fixture Runtime"} {state.runtimeCapabilities?.state}</span>
        <span>{state.designCapabilities?.mode === "real" ? "Daemon JDP" : "Fixture JDP"} {state.designCapabilities?.state}</span>
        <span className={`replay-state state-${state.replayState}`}><span className="status-dot" /> replay {state.replayState} · seq {state.lastRuntimeSequence}</span>
        <span className="ribbon-gap">
          {sidecarsHealthy && realData ? "真实 Runtime/daemon 与 UI transport 已连接；未实现能力会明确拒绝" : "真实 sidecar、ERC/DRC、制造能力尚未接入"}
        </span>
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
          theme={theme}
          viewport={state.viewports[state.activeView]}
          realData={realData}
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
        providerReady={providerReady}
        run={state.run}
      />

      <ApprovalDrawer approvals={state.approvals} onReply={(id, decision) => void actions.replyApproval(id, decision)} />
      <ModelSettingsDrawer
        onClose={() => setModelSettingsOpen(false)}
        open={modelSettingsOpen}
        onProviderApplied={() => void actions.refreshRuntimeCapabilities()}
        runtimeAvailable={state.sidecars.some((sidecar) => sidecar.kind === "runtime" && sidecar.state === "healthy")}
      />

      {state.notice && (
        <div className="toast toast-success" role="status"><CheckCircleIcon size={16} /><span>{state.notice}</span><button onClick={actions.clearNotice} title="关闭"><XIcon size={14} /></button></div>
      )}
      {state.error && (
        <div className="toast toast-error" role="alert"><AlertCircleIcon size={16} /><span>{state.error}</span><button onClick={actions.clearError} title="关闭"><XIcon size={14} /></button></div>
      )}
    </div>
  );
}
