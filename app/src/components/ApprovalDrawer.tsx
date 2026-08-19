import { useMemo, useState } from "react";
import type { ApprovalView } from "../clients/contracts";
import { AlertCircleIcon, CheckCircleIcon, ChevronRightIcon, ClockIcon, XIcon } from "./Icons";

interface ApprovalDrawerProps {
  approvals: ApprovalView[];
  onReply: (approvalId: string, decision: "approve-once" | "deny") => void;
}

export function ApprovalDrawer({ approvals, onReply }: ApprovalDrawerProps) {
  const pending = useMemo(() => approvals.filter((approval) => approval.status === "pending"), [approvals]);
  const [open, setOpen] = useState(true);
  if (pending.length === 0) return null;
  if (!open) {
    return <button className="approval-rail" onClick={() => setOpen(true)}><AlertCircleIcon size={16} /><span>{pending.length} 个审批待处理</span><ChevronRightIcon size={14} /></button>;
  }
  return (
    <aside className="approval-drawer" aria-label="待审批操作">
      <div className="approval-header">
        <div><span className="eyebrow">DURABLE APPROVAL</span><h2>操作审批</h2></div>
        <button className="icon-button" onClick={() => setOpen(false)} title="收起审批"><XIcon size={15} /></button>
      </div>
      {pending.map((approval) => (
        <article className="approval-card" key={approval.id}>
          <div className="approval-tool"><span className="tool-glyph"><ClockIcon size={15} /></span><div><strong>{approval.toolName}</strong><code>{approval.id}</code></div></div>
          <dl className="approval-details">
            <div><dt>目标</dt><dd>{approval.target}</dd></div>
            <div><dt>Effect</dt><dd><span className="effect-pill">{approval.effect}</span></dd></div>
            <div><dt>原因</dt><dd>{approval.reason}</dd></div>
            <div><dt>脱敏输入</dt><dd><code>{approval.redactedInput}</code></dd></div>
            <div><dt>有效范围</dt><dd>{approval.scope}</dd></div>
            <div><dt>过期</dt><dd>{new Date(approval.expiresAt).toLocaleString("zh-CN", { hour12: false })}</dd></div>
          </dl>
          <div className="approval-warning"><AlertCircleIcon size={14} />批准只对本次调用有效，不能覆盖 daemon deny、锁定对象或发布门禁。</div>
          <div className="approval-actions">
            <button className="secondary-button danger" onClick={() => onReply(approval.id, "deny")}><XIcon size={14} />拒绝</button>
            <button className="primary-button" onClick={() => onReply(approval.id, "approve-once")}><CheckCircleIcon size={14} />仅批准一次</button>
          </div>
        </article>
      ))}
    </aside>
  );
}
