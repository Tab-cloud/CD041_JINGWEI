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
  const ercFindings = project?.findings.filter((finding) => finding.status === "open" && finding.view === "schematic") ?? [];
  const drcFindings = project?.findings.filter((finding) => finding.status === "open" && finding.view === "pcb") ?? [];
  const toggleSection = (section: string) => setExpanded((current) => ({ ...current, [section]: !current[section] }));

  return (
    <aside className="project-sidebar" aria-label="项目导航">
      <label className="sidebar-search">
        <SearchIcon size={14} />
        <input aria-label="搜索对象、网络或 finding" onChange={(event) => setSearch(event.target.value)} placeholder="搜索对象、网络、finding" value={search} />
      </label>

      <SidebarSection expanded={Boolean(expanded.sessions)} icon={<MessageSquareIcon size={14} />} label="Agent 会话" onToggle={() => toggleSection("sessions")}>
        <div className="tree-row selected static">
          <span className="tree-leading status-dot running" />
          <span>当前设计会话</span>
          <small>本地 Runtime</small>
        </div>
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
        <button className="tree-row queue-row" disabled={!ercFindings[0]} onClick={() => ercFindings[0] && onSelectEntity(ercFindings[0].id, "schematic")}>
          <span className="queue-count warning">{ercFindings.length}</span><span>ERC findings</span>
        </button>
        <button className="tree-row queue-row" disabled={!drcFindings[0]} onClick={() => drcFindings[0] && onSelectEntity(drcFindings[0].id, "pcb")}>
          <span className="queue-count danger">{drcFindings.length}</span><span>DRC findings</span>
        </button>
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
