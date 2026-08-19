import type { FindingView } from "../../clients/contracts";
import { AlertCircleIcon, CheckCircleIcon } from "../Icons";

export function FindingsPanel({ findings, onFocus }: { findings: FindingView[]; onFocus: (finding: FindingView) => void }) {
  return (
    <div className="findings-table" role="table" aria-label="ERC 与 DRC findings">
      <div className="findings-head" role="row"><span>状态</span><span>规则 / Finding</span><span>对象</span><span>首次出现</span><span>证据</span></div>
      {findings.map((finding) => (
        <button className={`findings-row severity-${finding.severity}`} key={finding.id} onClick={() => onFocus(finding)} role="row">
          <span>{finding.status === "resolved" ? <CheckCircleIcon size={15} /> : <AlertCircleIcon size={15} />}{finding.severity}</span>
          <span><strong>{finding.code}</strong><small>{finding.title}</small></span>
          <span>{finding.entityIds.join(", ")}</span>
          <span><code>{finding.firstSeenRevision}</code></span>
          <span>{finding.evidenceRefs.length} refs</span>
        </button>
      ))}
      {findings.length === 0 && <div className="bottom-empty">当前 projection 没有 finding。此状态不代表全量验证已经运行。</div>}
    </div>
  );
}
