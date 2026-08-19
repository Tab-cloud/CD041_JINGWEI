import type { DesignViewKind, DisplayList, Viewport } from "../clients/contracts";
import type { ToolMode } from "../store/workbenchState";
import type { Theme } from "../theme/theme";
import { CanvasToolbar } from "./CanvasToolbar";
import { SemanticCanvas, type CanvasIntent } from "./SemanticCanvas";

interface EditorAreaProps {
  activeView: DesignViewKind;
  displayList?: DisplayList;
  viewport: Viewport;
  toolMode: ToolMode;
  selectedIds: string[];
  relatedIds: string[];
  ghostPrimitives: DisplayList["primitives"];
  previewLatencyMs?: number;
  theme: Theme;
  onViewChange: (view: DesignViewKind) => void;
  onViewportChange: (viewport: Viewport) => void;
  onToolChange: (tool: ToolMode) => void;
  onIntent: (intent: CanvasIntent) => void;
  onCancelInteraction: () => void;
}

export function EditorArea({
  activeView,
  displayList,
  viewport,
  toolMode,
  selectedIds,
  relatedIds,
  ghostPrimitives,
  previewLatencyMs,
  theme,
  onViewChange,
  onViewportChange,
  onToolChange,
  onIntent,
  onCancelInteraction,
}: EditorAreaProps) {
  return (
    <main className="editor-area">
      <div className="editor-tabs" role="tablist" aria-label="设计视图">
        <button aria-selected={activeView === "schematic"} className={activeView === "schematic" ? "active" : ""} onClick={() => onViewChange("schematic")} role="tab">Schematic</button>
        <button aria-selected={activeView === "pcb"} className={activeView === "pcb" ? "active" : ""} onClick={() => onViewChange("pcb")} role="tab">PCB 2D</button>
        <button aria-disabled="true" className="placeholder-tab" role="tab" title="3D adapter 尚未接入">3D <small>cross-probe</small></button>
        <button aria-disabled="true" className="placeholder-tab" role="tab" title="BOM projection 尚未接入">BOM <small>projection</small></button>
        <button aria-disabled="true" className="placeholder-tab" role="tab" title="制造输出尚未接入">Manufacturing <small>blocked</small></button>
        <div className="editor-tab-spacer" />
        <span className="primitive-summary">
          {displayList ? `${displayList.primitives.length} / ${displayList.totalPrimitives} primitives` : "等待 display list"}
        </span>
      </div>
      <CanvasToolbar
        onFit={() => onViewportChange({ ...viewport, x: 70, y: 45, zoom: 1 })}
        onToolChange={onToolChange}
        onZoom={(factor) => onViewportChange({ ...viewport, zoom: Math.min(3, Math.max(0.35, viewport.zoom * factor)) })}
        toolMode={toolMode}
        view={activeView}
        zoom={viewport.zoom}
      />
      <SemanticCanvas
        displayList={displayList}
        ghostPrimitives={ghostPrimitives}
        onCancel={onCancelInteraction}
        onIntent={onIntent}
        onViewportChange={onViewportChange}
        relatedIds={relatedIds}
        selectedIds={selectedIds}
        theme={theme}
        toolMode={toolMode}
        viewport={viewport}
      />
      <div className="canvas-statusbar">
        <span>JDP display-list projection</span>
        <span>hit-test: daemon facade</span>
        {previewLatencyMs !== undefined && <span>preview {previewLatencyMs.toFixed(1)} ms</span>}
        <span className="mock-warning">几何与规则结果来自 mock fixture</span>
      </div>
    </main>
  );
}
