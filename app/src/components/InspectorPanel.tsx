import { useState } from "react";
import type { EditPolicy, FindingView, ProjectView } from "../clients/contracts";
import { AlertCircleIcon, CheckCircleIcon, LockIcon, UnlockIcon } from "./Icons";

interface InspectorPanelProps {
  project?: ProjectView;
  selectedIds: string[];
  properties: Record<string, Record<string, string>>;
  onSetProperty: (entityId: string, key: string, value: string) => void;
  onSetPolicy: (entityIds: string[], mode: EditPolicy) => void;
  onFocusFinding: (finding: FindingView) => void;
}

export function InspectorPanel({ project, selectedIds, properties, onSetProperty, onSetPolicy, onFocusFinding }: InspectorPanelProps) {
  const primary = selectedIds[0];
  const selectedProperties = primary ? properties[primary] : undefined;
  const relatedFindings = project?.findings.filter((finding) => finding.entityIds.some((id) => selectedIds.includes(id))) ?? [];

  return (
    <aside className="inspector-panel" aria-label="属性与证据">
      <div className="panel-heading-row">
        <div><span className="eyebrow">SELECTION</span><h2>属性与约束</h2></div>
        <span className="selection-count">{selectedIds.length}</span>
      </div>

      {!primary && (
        <div className="empty-panel-state">
          <span className="empty-icon"><CircuitPlaceholder /></span>
          <strong>选择设计对象</strong>
          <p>选择 symbol、footprint、net 或 finding 后，可查看稳定 ID、证据、provenance 和编辑策略。</p>
        </div>
      )}

      {primary && (
        <>
          <section className="inspector-section">
            <h3>对象身份</h3>
            <dl className="property-list">
              <div><dt>Entity ID</dt><dd><code>{primary}</code></dd></div>
              <div><dt>Revision</dt><dd><code>{project?.draftRevision}</code></dd></div>
              {Object.entries(selectedProperties ?? {}).filter(([key]) => !["value", "editPolicy"].includes(key)).map(([key, value]) => (
                <div key={key}><dt>{key}</dt><dd>{value}</dd></div>
              ))}
            </dl>
          </section>

          <section className="inspector-section">
            <h3>人工语义编辑</h3>
            <EditablePropertyField initialValue={selectedProperties?.value ?? ""} key={`${primary}:${selectedProperties?.value ?? ""}`} onCommit={(value) => onSetProperty(primary, "value", value)} />
            <p className="field-hint">Enter 提交为语义 command；不会直接写入 React store。</p>
          </section>

          <section className="inspector-section">
            <h3>AI edit control</h3>
            <div className="segmented-control" role="group" aria-label="AI 编辑策略">
              <button onClick={() => onSetPolicy(selectedIds, "ai-editable")}><UnlockIcon size={14} />AI 可编辑</button>
              <button onClick={() => onSetPolicy(selectedIds, "ask-before-change")}>变更前询问</button>
              <button onClick={() => onSetPolicy(selectedIds, "locked")}><LockIcon size={14} />锁定</button>
            </div>
          </section>

          <section className="inspector-section">
            <h3>关联 findings</h3>
            {relatedFindings.length === 0 && <div className="compact-success"><CheckCircleIcon size={15} />当前选择无直接 finding</div>}
            {relatedFindings.map((finding) => (
              <button className={`finding-card severity-${finding.severity}`} key={finding.id} onClick={() => onFocusFinding(finding)}>
                <AlertCircleIcon size={15} />
                <span><strong>{finding.code}</strong><small>{finding.title}</small></span>
              </button>
            ))}
          </section>

          <section className="inspector-section">
            <h3>证据与 provenance</h3>
            <div className="evidence-card">
              <span>来源</span><strong>{selectedProperties?.provenance ?? "mock display projection"}</strong>
              <small>真实集成后由 JDP explain/query 返回 artifact/evidence refs。</small>
            </div>
          </section>
        </>
      )}

      <section className="release-gap-card">
        <span className="eyebrow">RELEASE GAPS</span>
        <strong>Production Release 被阻止</strong>
        <ul>{project?.releaseBlockers.map((blocker) => <li key={blocker}>{blocker}</li>)}</ul>
      </section>
    </aside>
  );
}

function EditablePropertyField({ initialValue, onCommit }: { initialValue: string; onCommit: (value: string) => void }) {
  const [value, setValue] = useState(initialValue);
  return (
    <label className="field-label">
      value / 属性
      <input
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && value) onCommit(value);
        }}
        placeholder="选择可编辑字段"
        value={value}
      />
    </label>
  );
}

function CircuitPlaceholder() {
  return (
    <svg aria-hidden="true" fill="none" height="34" stroke="currentColor" strokeWidth="1.5" viewBox="0 0 48 48" width="34">
      <rect height="28" rx="4" width="28" x="10" y="10" />
      <path d="M18 10V4M30 10V4M18 44v-6M30 44v-6M10 18H4M10 30H4M44 18h-6M44 30h-6" />
      <circle cx="24" cy="24" r="5" />
    </svg>
  );
}
