import type { DurableEvent, FindingView, PatchProposalView, ProjectView, RunView } from "../clients/contracts";
import type { BottomTab } from "../store/workbenchState";
import { GitCompareIcon, MessageSquareIcon, PanelBottomIcon } from "./Icons";
import { AgentPanel } from "./bottom/AgentPanel";
import { FindingsPanel } from "./bottom/FindingsPanel";
import { HistoryPanel } from "./bottom/HistoryPanel";
import { PatchReviewPanel } from "./bottom/PatchReviewPanel";
import { TimelinePanel } from "./bottom/TimelinePanel";

interface BottomPanelProps {
  activeTab: BottomTab;
  run?: RunView;
  project?: ProjectView;
  events: DurableEvent[];
  proposal?: PatchProposalView;
  onTabChange: (tab: BottomTab) => void;
  onSubmit: (text: string, delivery: "steer" | "queue") => void;
  onFocusFinding: (finding: FindingView) => void;
  onGeneratePatch: (instruction: string) => void;
  onApplyPatch: (accepted: string[], rejected: string[]) => void;
  providerReady: boolean;
}

const tabs: Array<{ id: BottomTab; label: string }> = [
  { id: "agent", label: "Agent 对话" },
  { id: "timeline", label: "Run / Tool Timeline" },
  { id: "findings", label: "ERC / DRC" },
  { id: "diff", label: "Semantic Diff" },
  { id: "history", label: "历史 / 恢复" },
];

export function BottomPanel({ activeTab, run, project, events, proposal, onTabChange, onSubmit, onFocusFinding, onGeneratePatch, onApplyPatch, providerReady }: BottomPanelProps) {
  return (
    <section className="bottom-panel" aria-label="Agent、验证和历史">
      <div className="bottom-tabs" role="tablist">
        <PanelBottomIcon size={15} />
        {tabs.map((tab) => (
          <button aria-selected={activeTab === tab.id} className={activeTab === tab.id ? "active" : ""} key={tab.id} onClick={() => onTabChange(tab.id)} role="tab">
            {tab.id === "agent" && <MessageSquareIcon size={13} />}
            {tab.id === "diff" && <GitCompareIcon size={13} />}
            {tab.label}
            {tab.id === "findings" && project && <span className="tab-count">{project.findings.filter((finding) => finding.status === "open").length}</span>}
          </button>
        ))}
      </div>
      <div className="bottom-content" role="tabpanel">
        {activeTab === "agent" && <AgentPanel onSubmit={onSubmit} providerReady={providerReady} run={run} />}
        {activeTab === "timeline" && <TimelinePanel run={run} />}
        {activeTab === "findings" && <FindingsPanel findings={project?.findings ?? []} onFocus={onFocusFinding} />}
        {activeTab === "diff" && <PatchReviewPanel onApply={onApplyPatch} onGenerate={onGeneratePatch} proposal={proposal} />}
        {activeTab === "history" && <HistoryPanel events={events} project={project} />}
      </div>
    </section>
  );
}
