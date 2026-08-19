import { useMemo, useState } from "react";
import type { DesignViewKind, ProjectView } from "../clients/contracts";
import { ChevronDownIcon, ChevronRightIcon, EyeIcon, EyeOffIcon, LayersIcon, MessageSquareIcon, SearchIcon } from "./Icons";

interface ProjectSidebarProps {
  project?: ProjectView;
  activeView: DesignViewKind;
  visibleLayerIds: string[];
  onToggleLayer: (layerId: string) => void;
  onSelectEntity: (entityId: string, view?: DesignViewKind) => void;
}

export function ProjectSidebar({ project, activeView, visibleLayerIds, onToggleLayer, onSelectEntity }: ProjectSidebarProps) {
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<Record<string, boolean>>({ sessions: true, sheets: true, layers: true, queues: true });
  const matchingSheets = useMemo(
    () => project?.sheets.filter((sheet) => sheet.name.toLowerCase().includes(search.toLowerCase())) ?? [],
    [project, search],
  );
  const toggleSection = (section: string) => setExpanded((current) => ({ ...current, [section]: !current[section] }));

  return (
    <aside className="project-sidebar" aria-label="项目导航">
      <label className="sidebar-search">
        <SearchIcon size={14} />
        <input aria-label="搜索对象、网络或 finding" onChange={(event) => setSearch(event.target.value)} placeholder="搜索对象、网络、finding" value={search} />
      </label>

      <SidebarSection expanded={Boolean(expanded.sessions)} icon={<MessageSquareIcon size={14} />} label="Agent 会话" onToggle={() => toggleSection("sessions")}>
        <button className="tree-row selected" onClick={() => onSelectEntity("session-main")}>
          <span className="tree-leading status-dot running" />
          <span>主设计会话</span>
          <small>等待审批</small>
        </button>
        <button className="tree-row" onClick={() => onSelectEntity("session-review")}>
          <span className="tree-leading status-dot" />
          <span>工程审查</span>
          <small>空闲</small>
        </button>
      </SidebarSection>

      <SidebarSection expanded={Boolean(expanded.sheets)} label="原理图层次" onToggle={() => toggleSection("sheets")}>
        {matchingSheets.map((sheet, index) => (
          <button className="tree-row" key={sheet.id} onClick={() => onSelectEntity(sheet.id, "schematic")} style={{ paddingLeft: index === 0 ? 10 : 25 }}>
            <ChevronRightIcon className={sheet.childCount ? "" : "muted-icon"} size={12} />
            <span>{sheet.name}</span>
            {sheet.childCount > 0 && <small>{sheet.childCount}</small>}
          </button>
        ))}
      </SidebarSection>

      <SidebarSection expanded={Boolean(expanded.layers)} icon={<LayersIcon size={14} />} label={activeView === "pcb" ? "PCB 图层" : "视图图层"} onToggle={() => toggleSection("layers")}>
        {project?.layers
          .filter((layer) => activeView === "pcb" ? layer.category !== "logical" : ["logical", "overlay"].includes(layer.category))
          .map((layer) => {
            const visible = visibleLayerIds.includes(layer.id);
            return (
              <button className="tree-row layer-row" key={layer.id} onClick={() => onToggleLayer(layer.id)}>
                {visible ? <EyeIcon size={13} /> : <EyeOffIcon size={13} />}
                <span className="layer-color" style={{ backgroundColor: layer.color }} />
                <span>{layer.name}</span>
              </button>
            );
          })}
      </SidebarSection>

      <SidebarSection expanded={Boolean(expanded.queues)} label="待处理队列" onToggle={() => toggleSection("queues")}>
        <button className="tree-row queue-row" onClick={() => onSelectEntity("finding-erc-001", "schematic")}>
          <span className="queue-count warning">1</span><span>ERC warnings</span>
        </button>
        <button className="tree-row queue-row" onClick={() => onSelectEntity("finding-drc-004", "pcb")}>
          <span className="queue-count danger">1</span><span>DRC errors</span>
        </button>
        <div className="tree-row queue-row static"><span className="queue-count">3</span><span>未布通网络</span></div>
      </SidebarSection>
    </aside>
  );
}

function SidebarSection({
  label,
  icon,
  expanded,
  onToggle,
  children,
}: {
  label: string;
  icon?: React.ReactNode;
  expanded: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  return (
    <section className="sidebar-section">
      <button aria-expanded={expanded} className="section-heading" onClick={onToggle}>
        {expanded ? <ChevronDownIcon size={13} /> : <ChevronRightIcon size={13} />}
        {icon}<span>{label}</span>
      </button>
      {expanded && <div className="section-tree">{children}</div>}
    </section>
  );
}
