import type { ProjectView } from "../clients/contracts";
import type { SidecarStatus } from "../store/workbenchState";
import {
  CircuitBoardIcon,
  FolderOpenIcon,
  PlayIcon,
  PlusIcon,
  RedoIcon,
  SaveIcon,
  StopIcon,
  UndoIcon,
} from "./Icons";
import { StatusBadge } from "./StatusBadge";

interface TopBarProps {
  project?: ProjectView;
  sidecars: SidecarStatus[];
  runActive: boolean;
  undoAvailable: boolean;
  redoAvailable: boolean;
  onCreateProject: () => void;
  onOpenProject: () => void;
  onCheckpoint: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onRun: () => void;
  onCancel: () => void;
}

const maturityLabels: Record<ProjectView["maturity"], string> = {
  draft: "Draft",
  candidate: "Candidate",
  "engineering-verified": "Engineering Verified",
  "manufacturing-candidate": "Manufacturing Candidate",
  "prototype-verified": "Prototype Verified",
  "production-released": "Production Released",
};

export function TopBar({
  project,
  sidecars,
  runActive,
  undoAvailable,
  redoAvailable,
  onCreateProject,
  onOpenProject,
  onCheckpoint,
  onUndo,
  onRedo,
  onRun,
  onCancel,
}: TopBarProps) {
  return (
    <header className="top-bar">
      <div className="brand-block">
        <span className="brand-mark"><CircuitBoardIcon size={20} /></span>
        <div>
          <strong>经纬 Agent</strong>
          <span>原生设计工作台</span>
        </div>
      </div>

      <div className="project-identity">
        <div className="project-title-row">
          <strong>{project?.name ?? "尚未打开项目"}</strong>
          <StatusBadge tone="mock">MOCK 0.1 · DEV</StatusBadge>
          {project && <StatusBadge tone={project.maturity === "production-released" ? "success" : "warning"}>{maturityLabels[project.maturity]}</StatusBadge>}
        </div>
        {project && (
          <div className="revision-row" aria-label="项目 revision">
            <span>current <code>{project.currentRevision}</code></span>
            <span>candidate <code>{project.candidateRevision}</code></span>
            <span>draft <code>{project.draftRevision ?? "—"}</code></span>
          </div>
        )}
      </div>

      <div className="sidecar-summary" aria-label="sidecar 状态">
        {sidecars.map((sidecar) => (
          <span className={`sidecar-pill state-${sidecar.state}`} key={sidecar.kind} title={sidecar.reason}>
            <span className="status-dot" /> {sidecar.kind}
          </span>
        ))}
      </div>

      <nav aria-label="项目与编辑操作" className="top-actions">
        <button className="icon-button" onClick={onCreateProject} title="创建 mock 项目"><PlusIcon /></button>
        <button className="icon-button" onClick={onOpenProject} title="选择项目目录"><FolderOpenIcon /></button>
        <span className="action-divider" />
        <button className="icon-button" disabled={!undoAvailable} onClick={onUndo} title="撤销语义命令"><UndoIcon /></button>
        <button className="icon-button" disabled={!redoAvailable} onClick={onRedo} title="重做语义命令"><RedoIcon /></button>
        <button className="icon-button" disabled={!project} onClick={onCheckpoint} title="创建命名检查点"><SaveIcon /></button>
        <span className="action-divider" />
        {runActive ? (
          <button className="primary-button danger" onClick={onCancel}><StopIcon size={15} />取消</button>
        ) : (
          <button className="primary-button" onClick={onRun}><PlayIcon size={15} />运行</button>
        )}
      </nav>
    </header>
  );
}
