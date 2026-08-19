import type { RunView, ToolTimelineItem } from "../../clients/contracts";
import { AlertCircleIcon, BotIcon, CheckCircleIcon, ClockIcon } from "../Icons";

export function TimelinePanel({ run }: { run?: RunView }) {
  if (!run) return <div className="bottom-empty">提交 Agent 输入或点击运行后，这里显示持久 run / model attempt / tool / approval 终态。</div>;
  return (
    <div className="timeline-layout">
      <div className="run-summary-card">
        <span className={`run-state state-${run.status}`}>{run.status}</span>
        <strong>{run.title}</strong>
        <code>{run.id}</code>
        <p>只显示可审计运行投影，不暴露 private child reasoning。</p>
      </div>
      <ol className="timeline-list">
        {run.timeline.map((item) => <TimelineItem item={item} key={item.id} />)}
      </ol>
    </div>
  );
}

function TimelineItem({ item }: { item: ToolTimelineItem }) {
  const StatusIcon = item.status === "succeeded" ? CheckCircleIcon : item.status === "failed" || item.status === "denied" ? AlertCircleIcon : item.status === "waiting" ? ClockIcon : BotIcon;
  return (
    <li className={`timeline-item status-${item.status}`}>
      <span className="timeline-marker"><StatusIcon size={14} /></span>
      <div className="timeline-main">
        <div><strong>{item.title}</strong><span>{item.status}</span>{item.durationMs !== undefined && <small>{item.durationMs} ms</small>}</div>
        <p>{item.detail}</p>
        {(item.target || item.effect) && <div className="timeline-meta">{item.target && <span>target: {item.target}</span>}{item.effect && <span>effect: {item.effect}</span>}</div>}
        {item.error && <div className="structured-error"><code>{item.error.code}</code><span>{item.error.summary}</span><small>{item.error.suggestedAction}</small></div>}
      </div>
    </li>
  );
}
