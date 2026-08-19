import { useEffect, useRef } from "react";
import type { DisplayList, Point, Viewport } from "../clients/contracts";
import type { ToolMode } from "../store/workbenchState";
import { canvasPointToWorld } from "../canvas/displayList";
import { renderCanvas } from "../canvas/renderCanvas";

export interface CanvasIntent {
  phase: "down" | "move" | "up";
  point: Point;
  shiftKey: boolean;
}

interface SemanticCanvasProps {
  displayList?: DisplayList;
  viewport: Viewport;
  toolMode: ToolMode;
  selectedIds: string[];
  relatedIds: string[];
  ghostPrimitives: DisplayList["primitives"];
  onViewportChange: (viewport: Viewport) => void;
  onIntent: (intent: CanvasIntent) => void;
  onCancel: () => void;
}

export function SemanticCanvas({
  displayList,
  viewport,
  toolMode,
  selectedIds,
  relatedIds,
  ghostPrimitives,
  onViewportChange,
  onIntent,
  onCancel,
}: SemanticCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const panRef = useRef<{ client: Point; viewport: Viewport } | undefined>(undefined);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const render = () => renderCanvas(canvas, displayList?.primitives ?? [], {
      viewport,
      selectedIds: new Set(selectedIds),
      relatedIds: new Set(relatedIds),
      ghostPrimitives,
      devicePixelRatio: window.devicePixelRatio,
    });
    render();
    const observer = new ResizeObserver(render);
    observer.observe(canvas);
    return () => observer.disconnect();
  }, [displayList, ghostPrimitives, relatedIds, selectedIds, viewport]);

  const worldPoint = (clientX: number, clientY: number): Point => {
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) return { x: 0, y: 0 };
    return canvasPointToWorld({ x: clientX, y: clientY }, rect, viewport);
  };

  return (
    <div className="semantic-canvas-shell">
      <canvas
        ref={canvasRef}
        aria-label={`${displayList?.view === "pcb" ? "PCB" : "原理图"}语义画布；当前工具 ${toolMode}`}
        className={`semantic-canvas tool-${toolMode}`}
        onKeyDown={(event) => {
          if (event.key === "Escape") onCancel();
          if (event.key === "+" || event.key === "=") onViewportChange({ ...viewport, zoom: Math.min(3, viewport.zoom * 1.15) });
          if (event.key === "-") onViewportChange({ ...viewport, zoom: Math.max(0.35, viewport.zoom / 1.15) });
          if (event.key === "Home") onViewportChange({ ...viewport, x: 70, y: 45, zoom: 1 });
        }}
        onPointerDown={(event) => {
          event.currentTarget.focus();
          event.currentTarget.setPointerCapture(event.pointerId);
          if (event.button === 1 || event.button === 2) {
            panRef.current = { client: { x: event.clientX, y: event.clientY }, viewport };
            return;
          }
          onIntent({ phase: "down", point: worldPoint(event.clientX, event.clientY), shiftKey: event.shiftKey });
        }}
        onPointerMove={(event) => {
          const pan = panRef.current;
          if (pan) {
            onViewportChange({
              ...pan.viewport,
              x: pan.viewport.x - (event.clientX - pan.client.x) / pan.viewport.zoom,
              y: pan.viewport.y - (event.clientY - pan.client.y) / pan.viewport.zoom,
            });
            return;
          }
          onIntent({ phase: "move", point: worldPoint(event.clientX, event.clientY), shiftKey: event.shiftKey });
        }}
        onPointerUp={(event) => {
          if (panRef.current) {
            panRef.current = undefined;
            return;
          }
          onIntent({ phase: "up", point: worldPoint(event.clientX, event.clientY), shiftKey: event.shiftKey });
        }}
        onWheel={(event) => {
          event.preventDefault();
          const point = worldPoint(event.clientX, event.clientY);
          const nextZoom = Math.min(3, Math.max(0.35, viewport.zoom * (event.deltaY < 0 ? 1.12 : 0.89)));
          const rect = event.currentTarget.getBoundingClientRect();
          onViewportChange({
            ...viewport,
            zoom: nextZoom,
            x: point.x - (event.clientX - rect.left) / nextZoom,
            y: point.y - (event.clientY - rect.top) / nextZoom,
          });
        }}
        onContextMenu={(event) => event.preventDefault()}
        tabIndex={0}
      />
      {!displayList && <div className="canvas-loading">正在请求增量 display list</div>}
      <div aria-live="polite" className="sr-only">
        已选择 {selectedIds.length} 个对象，cross-probe 关联 {relatedIds.length} 个对象
      </div>
    </div>
  );
}
