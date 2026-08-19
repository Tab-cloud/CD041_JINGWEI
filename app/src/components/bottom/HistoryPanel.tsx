import type { DurableEvent, ProjectView } from "../../clients/contracts";
import { CheckCircleIcon, ClockIcon, RefreshIcon } from "../Icons";

export function HistoryPanel({ project, events }: { project?: ProjectView; events: DurableEvent[] }) {
  return (
    <div className="history-layout">
      <div className="revision-rail">
        <RevisionNode label="Project current" revision={project?.currentRevision} status="stable" />
        <RevisionNode label="Candidate" revision={project?.candidateRevision} status="candidate" />
        <RevisionNode label="Server-backed draft" revision={project?.draftRevision} status="draft" />
      </div>
      <div className="replay-card">
        <div className="panel-heading-row"><div><span className="eyebrow">DURABLE REPLAY</span><h3>事件恢复游标</h3></div><RefreshIcon size={17} /></div>
        <p>重连先 read-after，再接 live stream。本地只保存 sequence、edit session ID 与 viewport；不保存 JIR。</p>
        <div className="event-list">
          {events.slice(-6).map((event) => (
            <div key={`${event.aggregateId}-${event.sequence}`}><code>#{event.sequence}</code><span>{event.type}</span><time>{new Date(event.timestamp).toLocaleTimeString("zh-CN", { hour12: false })}</time></div>
          ))}
          {events.length === 0 && <span className="muted-copy">尚无回放事件</span>}
        </div>
      </div>
    </div>
  );
}

function RevisionNode({ label, revision, status }: { label: string; revision?: string; status: "stable" | "candidate" | "draft" }) {
  return (
    <div className={`revision-node status-${status}`}>
      <span>{status === "draft" ? <ClockIcon size={14} /> : <CheckCircleIcon size={14} />}</span>
      <div><small>{label}</small><code>{revision ?? "—"}</code></div>
    </div>
  );
}
