import type { DesignViewKind } from "../clients/contracts";
import type { ToolMode } from "../store/workbenchState";
import { CrosshairIcon, LinkIcon, MousePointerIcon, MoveIcon, RouteIcon, ViaIcon, ZoomInIcon, ZoomOutIcon } from "./Icons";

interface CanvasToolbarProps {
  view: DesignViewKind;
  toolMode: ToolMode;
  zoom: number;
  onToolChange: (tool: ToolMode) => void;
  onZoom: (factor: number) => void;
  onFit: () => void;
}

export function CanvasToolbar({ view, toolMode, zoom, onToolChange, onZoom, onFit }: CanvasToolbarProps) {
  const tools: Array<{ id: ToolMode; label: string; icon: React.ReactNode; views: DesignViewKind[] }> = [
    { id: "select", label: "选择", icon: <MousePointerIcon />, views: ["schematic", "pcb"] },
    { id: "move", label: "移动", icon: <MoveIcon />, views: ["schematic", "pcb"] },
    { id: "connect", label: "连接 wire", icon: <LinkIcon />, views: ["schematic"] },
    { id: "route", label: "绘制 track", icon: <RouteIcon />, views: ["pcb"] },
    { id: "via", label: "放置 via", icon: <ViaIcon />, views: ["pcb"] },
  ];
  return (
    <div className="canvas-toolbar" role="toolbar" aria-label="画布工具">
      <div className="tool-group">
        {tools.filter((tool) => tool.views.includes(view)).map((tool) => (
          <button aria-pressed={toolMode === tool.id} className={`tool-button ${toolMode === tool.id ? "active" : ""}`} key={tool.id} onClick={() => onToolChange(tool.id)} title={tool.label}>
            {tool.icon}<span>{tool.label}</span>
          </button>
        ))}
      </div>
      <div className="tool-group zoom-tools">
        <button className="icon-button" onClick={() => onZoom(1 / 1.15)} title="缩小"><ZoomOutIcon size={15} /></button>
        <span>{Math.round(zoom * 100)}%</span>
        <button className="icon-button" onClick={() => onZoom(1.15)} title="放大"><ZoomInIcon size={15} /></button>
        <button className="icon-button" onClick={onFit} title="适合窗口"><CrosshairIcon size={15} /></button>
      </div>
    </div>
  );
}
