import { useState } from "react";
import type { PatchProposalView } from "../../clients/contracts";
import { AlertCircleIcon, CheckCircleIcon, GitCompareIcon, LockIcon, XIcon } from "../Icons";

interface PatchReviewPanelProps {
  proposal?: PatchProposalView;
  onGenerate: (instruction: string) => void;
  onApply: (accepted: string[], rejected: string[]) => void;
}

export function PatchReviewPanel({ proposal, onGenerate, onApply }: PatchReviewPanelProps) {
  if (!proposal) {
    return (
      <div className="patch-empty">
        <span><GitCompareIcon size={24} /></span>
        <div><strong>审查 AI semantic patch</strong><p>proposal 会列出 base revision、对象、网络、规则、finding 与受保护内容。</p></div>
        <button className="primary-button" onClick={() => onGenerate("保持 J1 锁定，只修复 RS-485 走线间距")}>生成 versioned mock proposal</button>
      </div>
    );
  }

  return <PatchDecisionContent key={proposal.id} onApply={onApply} proposal={proposal} />;
}

function PatchDecisionContent({ proposal, onApply }: { proposal: PatchProposalView; onApply: (accepted: string[], rejected: string[]) => void }) {
  const [decisions, setDecisions] = useState<Record<string, "accepted" | "rejected">>({});

  const accepted = Object.entries(decisions).filter(([, decision]) => decision === "accepted").map(([id]) => id);
  const rejected = Object.entries(decisions).filter(([, decision]) => decision === "rejected").map(([id]) => id);
  const pending = proposal.hunks.filter((hunk) => !decisions[hunk.id]).length;
  return (
    <div className="patch-review">
      <div className="patch-summary">
        <div><span className="eyebrow">AI PROPOSAL</span><strong>{proposal.instruction}</strong><code>base {proposal.baseRevision}</code></div>
        <span className={`proposal-state state-${proposal.status}`}>{proposal.status}</span>
        <dl><div><dt>Networks</dt><dd>{proposal.affectedNets.join(", ")}</dd></div><div><dt>Rules</dt><dd>{proposal.affectedRules.join(", ")}</dd></div></dl>
      </div>
      {proposal.status === "stale" && <div className="stale-banner"><AlertCircleIcon size={15} />proposal base 已过期，禁止直接应用；请 rebase 或重新生成。</div>}
      <div className="hunk-list">
        {proposal.hunks.map((hunk) => (
          <article className={`hunk-card decision-${decisions[hunk.id] ?? "pending"}`} key={hunk.id}>
            <div className="hunk-icon">{hunk.protectedImpact ? <LockIcon size={15} /> : <GitCompareIcon size={15} />}</div>
            <div><strong>{hunk.label}</strong><p>{hunk.summary}</p><small>{hunk.scope} · {hunk.entityIds.join(", ")}</small></div>
            <div className="hunk-actions">
              <button aria-label={`接受 ${hunk.label}`} className={decisions[hunk.id] === "accepted" ? "active accept" : ""} disabled={hunk.protectedImpact} onClick={() => setDecisions((current) => ({ ...current, [hunk.id]: "accepted" }))}><CheckCircleIcon size={14} /></button>
              <button aria-label={`拒绝 ${hunk.label}`} className={decisions[hunk.id] === "rejected" ? "active reject" : ""} onClick={() => setDecisions((current) => ({ ...current, [hunk.id]: "rejected" }))}><XIcon size={14} /></button>
            </div>
          </article>
        ))}
      </div>
      <div className="patch-footer">
        <span>{accepted.length} 接受 · {rejected.length} 拒绝 · {pending} 待决定</span>
        <button className="primary-button" disabled={proposal.status !== "ready" || pending > 0 || accepted.length === 0} onClick={() => onApply(accepted, rejected)}>应用已接受 hunks</button>
      </div>
    </div>
  );
}
