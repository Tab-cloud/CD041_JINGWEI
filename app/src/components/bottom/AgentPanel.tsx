import { useState } from "react";
import type { RunView } from "../../clients/contracts";
import { BotIcon, SendIcon, UserIcon } from "../Icons";

interface AgentPanelProps {
  run?: RunView;
  onSubmit: (text: string, delivery: "steer" | "queue") => void;
}

export function AgentPanel({ run, onSubmit }: AgentPanelProps) {
  const [text, setText] = useState("解释 RS-485 DRC，并给出不移动锁定 J1 的局部修复候选");
  const [delivery, setDelivery] = useState<"steer" | "queue">("queue");
  const submit = () => {
    const trimmed = text.trim();
    if (!trimmed) return;
    onSubmit(trimmed, delivery);
    setText("");
  };
  return (
    <div className="agent-panel-grid">
      <div className="conversation-view">
        <div className="message-row user-message"><span className="avatar"><UserIcon size={15} /></span><p>保持连接器和安装孔不动，只修复 RS-485 局部 DRC。</p></div>
        <div className="message-row agent-message">
          <span className="avatar"><BotIcon size={15} /></span>
          <div>
            <p>{run?.output ?? "当前为 versioned mock。提交输入后，将展示 durable run/tool/approval 投影。"}</p>
            <div className="message-refs"><button>track-rs485-a</button><button>finding-drc-004</button><button>rev-candidate-007</button></div>
          </div>
        </div>
      </div>
      <div className="composer-area">
        <div className="quick-prompts">
          <button onClick={() => setText("解释当前选中 finding，并给出两个候选")}>解释 finding</button>
          <button onClick={() => setText("只重新布线选中网络，不修改锁定对象")}>局部重布线</button>
          <button onClick={() => setText("检查当前 draft 的验证失效项")}>检查 draft</button>
        </div>
        <div className="composer-box">
          <textarea aria-label="给 Agent 的指令" onChange={(event) => setText(event.target.value)} onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); submit(); }
          }} placeholder="描述设计意图；稳定对象 ID 和 selection 会作为语义上下文" value={text} />
          <div className="composer-actions">
            <label>投递方式<select onChange={(event) => setDelivery(event.target.value as "steer" | "queue")} value={delivery}><option value="queue">Queue</option><option value="steer">Steer</option></select></label>
            <span>附件仅保存 artifact reference</span>
            <button className="send-button" disabled={!text.trim()} onClick={submit}><SendIcon size={14} />提交</button>
          </div>
        </div>
      </div>
    </div>
  );
}
