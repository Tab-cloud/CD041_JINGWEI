import type {
  ApprovalView,
  DisplayPrimitive,
  DurableEvent,
  FindingView,
  ProjectView,
  ToolTimelineItem,
} from "../../clients/contracts";

const now = "2026-08-19T12:00:00.000Z";

export const goldenFindings: FindingView[] = [
  {
    id: "finding-erc-001",
    code: "ERC.POWER_INPUT_UNDRIVEN",
    title: "3V3 电源输入缺少驱动证据",
    detail: "U2 的 3V3 引脚尚未关联稳压器输出证据。",
    severity: "warning",
    view: "schematic",
    entityIds: ["symbol-u2", "net-3v3"],
    location: { x: 620, y: 252 },
    rule: "erc.power-input-driver.v1",
    firstSeenRevision: "rev-candidate-007",
    evidenceRefs: ["evidence-datasheet-u2-p14"],
    status: "open",
  },
  {
    id: "finding-drc-004",
    code: "DRC.CLEARANCE",
    title: "RS-485 走线间距不足",
    detail: "NET_A 与安装孔禁布区之间的局部间距为 0.16 mm，规则要求 0.20 mm。",
    severity: "error",
    view: "pcb",
    entityIds: ["track-rs485-a", "mount-hole-2"],
    location: { x: 714, y: 368 },
    rule: "jlcpcb.2layer.clearance.v0",
    firstSeenRevision: "rev-candidate-007",
    evidenceRefs: ["evidence-drc-run-18"],
    status: "open",
  },
];

export const projectFixture: ProjectView = {
  id: "project-golden-rs485",
  name: "温湿度 RS-485 采集板",
  currentRevision: "rev-current-004",
  candidateRevision: "rev-candidate-007",
  draftRevision: "rev-draft-011",
  maturity: "candidate",
  stages: [
    { id: "intent", label: "需求已基线", status: "complete" },
    { id: "schematic", label: "原理图候选", status: "complete" },
    { id: "pcb", label: "PCB 候选", status: "current" },
    { id: "engineering", label: "工程验证", status: "blocked" },
    { id: "prototype", label: "样板验证", status: "blocked" },
    { id: "release", label: "生产发布", status: "blocked" },
  ],
  releaseBlockers: ["1 个 DRC error 未解决", "缺少制造商 DFM", "缺少样板与具名签核"],
  sheets: [
    { id: "sheet-root", name: "采集板总览", childCount: 3 },
    { id: "sheet-power", name: "电源与保护", childCount: 0 },
    { id: "sheet-mcu", name: "MCU 与传感器", childCount: 0 },
    { id: "sheet-io", name: "RS-485 接口", childCount: 0 },
  ],
  layers: [
    { id: "logical", name: "原理图对象", color: "#8ca3c7", visible: true, category: "logical" },
    { id: "F.Cu", name: "F.Cu", color: "#db5b70", visible: true, category: "copper" },
    { id: "B.Cu", name: "B.Cu", color: "#417dc1", visible: true, category: "copper" },
    { id: "F.Silkscreen", name: "F.Silkscreen", color: "#d9d9d9", visible: true, category: "silkscreen" },
    { id: "Edge.Cuts", name: "Edge.Cuts", color: "#ceb655", visible: true, category: "outline" },
    { id: "findings", name: "Findings", color: "#ef815f", visible: true, category: "overlay" },
  ],
  findings: goldenFindings,
};

export const schematicPrimitives: DisplayPrimitive[] = [
  { id: "sch-u1", entityId: "symbol-u1", kind: "symbol", layerId: "logical", position: { x: 290, y: 260 }, width: 128, height: 92, label: "U1\nSTM32G0", color: "#80a7d9", zIndex: 20 },
  { id: "sch-u2", entityId: "symbol-u2", kind: "symbol", layerId: "logical", position: { x: 610, y: 260 }, width: 122, height: 86, label: "U2\nSHT40", color: "#80a7d9", zIndex: 20 },
  { id: "sch-u3", entityId: "symbol-u3", kind: "symbol", layerId: "logical", position: { x: 600, y: 450 }, width: 132, height: 96, label: "U3\nMAX3485", color: "#80a7d9", zIndex: 20 },
  { id: "sch-j1", entityId: "symbol-j1", kind: "symbol", layerId: "logical", position: { x: 855, y: 448 }, width: 108, height: 116, label: "J1\nRS-485", color: "#7bbba4", zIndex: 20 },
  { id: "sch-r1", entityId: "symbol-r1", kind: "symbol", layerId: "logical", position: { x: 430, y: 445 }, width: 78, height: 34, label: "R1 120R", color: "#c29c68", zIndex: 20 },
  { id: "sch-c1", entityId: "symbol-c1", kind: "symbol", layerId: "logical", position: { x: 470, y: 185 }, width: 54, height: 38, label: "C1 100n", color: "#c29c68", zIndex: 20 },
  { id: "pin-u1-i2c", entityId: "pin-u1-i2c", kind: "pin", layerId: "logical", netId: "net-i2c", position: { x: 354, y: 242 }, radius: 4, zIndex: 30 },
  { id: "pin-u2-i2c", entityId: "pin-u2-i2c", kind: "pin", layerId: "logical", netId: "net-i2c", position: { x: 549, y: 242 }, radius: 4, zIndex: 30 },
  { id: "wire-i2c", entityId: "net-i2c", kind: "wire", layerId: "logical", netId: "net-i2c", points: [{ x: 354, y: 242 }, { x: 450, y: 242 }, { x: 450, y: 242 }, { x: 549, y: 242 }], color: "#58b6b2", zIndex: 10 },
  { id: "label-i2c", entityId: "net-i2c", kind: "label", layerId: "logical", netId: "net-i2c", position: { x: 438, y: 225 }, label: "I2C_SDA", color: "#58b6b2", zIndex: 25 },
  { id: "wire-uart", entityId: "net-uart", kind: "wire", layerId: "logical", netId: "net-uart", points: [{ x: 354, y: 284 }, { x: 452, y: 284 }, { x: 452, y: 450 }, { x: 534, y: 450 }], color: "#8db5d2", zIndex: 10 },
  { id: "wire-rs-a", entityId: "net-rs485-a", kind: "wire", layerId: "logical", netId: "net-rs485-a", points: [{ x: 666, y: 430 }, { x: 760, y: 430 }, { x: 760, y: 418 }, { x: 801, y: 418 }], color: "#e59a65", zIndex: 10 },
  { id: "wire-rs-b", entityId: "net-rs485-b", kind: "wire", layerId: "logical", netId: "net-rs485-b", points: [{ x: 666, y: 468 }, { x: 760, y: 468 }, { x: 760, y: 478 }, { x: 801, y: 478 }], color: "#e59a65", zIndex: 10 },
  { id: "finding-erc-mark", entityId: "finding-erc-001", kind: "finding", layerId: "findings", position: { x: 620, y: 252 }, radius: 18, severity: "warning", zIndex: 50 },
];

export const pcbPrimitives: DisplayPrimitive[] = [
  { id: "board", entityId: "board-main", kind: "board-outline", layerId: "Edge.Cuts", points: [{ x: 155, y: 105 }, { x: 950, y: 105 }, { x: 950, y: 585 }, { x: 155, y: 585 }, { x: 155, y: 105 }], color: "#ceb655", zIndex: 1 },
  { id: "zone-gnd", entityId: "zone-gnd-1", kind: "zone", layerId: "B.Cu", netId: "net-gnd", points: [{ x: 180, y: 130 }, { x: 925, y: 130 }, { x: 925, y: 560 }, { x: 180, y: 560 }], color: "#315f93", zIndex: 2 },
  { id: "fp-u1", entityId: "footprint-u1", kind: "footprint", layerId: "F.Cu", position: { x: 470, y: 310 }, width: 140, height: 120, label: "U1", color: "#db5b70", zIndex: 20 },
  { id: "fp-u2", entityId: "footprint-u2", kind: "footprint", layerId: "F.Cu", position: { x: 690, y: 230 }, width: 94, height: 82, label: "U2", color: "#db5b70", zIndex: 20 },
  { id: "fp-u3", entityId: "footprint-u3", kind: "footprint", layerId: "F.Cu", position: { x: 700, y: 405 }, width: 126, height: 88, label: "U3", color: "#db5b70", zIndex: 20 },
  { id: "fp-j1", entityId: "footprint-j1", kind: "footprint", layerId: "F.Cu", position: { x: 860, y: 365 }, width: 100, height: 170, label: "J1", color: "#d6776b", locked: true, zIndex: 20 },
  { id: "pad-u1-1", entityId: "pad-u1-1", kind: "pad", layerId: "F.Cu", netId: "net-3v3", position: { x: 425, y: 270 }, width: 18, height: 10, color: "#e9c06f", zIndex: 30 },
  { id: "pad-u2-1", entityId: "pad-u2-1", kind: "pad", layerId: "F.Cu", netId: "net-3v3", position: { x: 652, y: 215 }, width: 18, height: 12, color: "#e9c06f", zIndex: 30 },
  { id: "track-3v3", entityId: "track-3v3", kind: "track", layerId: "F.Cu", netId: "net-3v3", points: [{ x: 425, y: 270 }, { x: 560, y: 270 }, { x: 560, y: 215 }, { x: 652, y: 215 }], width: 7, color: "#de6377", zIndex: 15 },
  { id: "track-rs-a", entityId: "track-rs485-a", kind: "track", layerId: "F.Cu", netId: "net-rs485-a", points: [{ x: 750, y: 385 }, { x: 790, y: 385 }, { x: 790, y: 335 }, { x: 825, y: 335 }], width: 6, color: "#ef815f", zIndex: 15 },
  { id: "track-rs-b", entityId: "track-rs485-b", kind: "track", layerId: "B.Cu", netId: "net-rs485-b", points: [{ x: 750, y: 425 }, { x: 784, y: 425 }, { x: 784, y: 395 }, { x: 825, y: 395 }], width: 6, color: "#4b8fd6", zIndex: 15 },
  { id: "via-rs", entityId: "via-rs485-1", kind: "via", layerId: "F.Cu", netId: "net-rs485-b", position: { x: 784, y: 425 }, radius: 8, color: "#d8ba6d", zIndex: 35 },
  { id: "rat-u1-u3", entityId: "net-uart", kind: "ratsnest", layerId: "F.Cu", netId: "net-uart", points: [{ x: 530, y: 345 }, { x: 640, y: 405 }], color: "#70849f", zIndex: 8 },
  { id: "pcb-title", entityId: "text-board-title", kind: "text", layerId: "F.Silkscreen", position: { x: 250, y: 520 }, label: "JINGWEI GOLDEN BOARD", color: "#d9d9d9", zIndex: 25 },
  { id: "finding-drc-mark", entityId: "finding-drc-004", kind: "finding", layerId: "findings", position: { x: 714, y: 368 }, radius: 20, severity: "error", zIndex: 50 },
];

export const objectProperties: Record<string, Record<string, string>> = {
  "symbol-u1": { reference: "U1", value: "STM32G0", footprint: "LQFP-48", provenance: "AI candidate", editPolicy: "ai-editable" },
  "symbol-u2": { reference: "U2", value: "SHT40", footprint: "DFN-4", provenance: "library", editPolicy: "ask-before-change" },
  "footprint-u1": { reference: "U1", layer: "F.Cu", rotation: "0 deg", provenance: "solver", editPolicy: "ai-editable" },
  "footprint-j1": { reference: "J1", layer: "F.Cu", rotation: "90 deg", provenance: "human", editPolicy: "locked" },
  "net-rs485-a": { name: "RS485_A", netClass: "RS485", clearance: "0.20 mm", editPolicy: "ask-before-change" },
};

export const approvalFixtures: ApprovalView[] = [
  {
    id: "approval-002",
    sessionId: "session-main",
    toolName: "pcb.route_critical",
    target: "project-golden-rs485 / NET_A, NET_B",
    reason: "需要生成局部候选走线；不会移动锁定的 J1。",
    effect: "design-write candidate",
    redactedInput: "base=rev-candidate-007, nets=[NET_A, NET_B], credentials=[redacted]",
    scope: "本次调用、当前项目、选中网络",
    expiresAt: "2026-08-19T13:00:00.000Z",
    status: "pending",
  },
];

export const initialTimeline: ToolTimelineItem[] = [
  { id: "run-1", kind: "run", title: "Main Agent", detail: "评估 RS-485 DRC 并生成局部修复候选", status: "running", startedAt: now },
  { id: "model-1", parentRunId: "run-1", kind: "model", title: "模型尝试 1", detail: "已形成受限修复计划", status: "succeeded", startedAt: now, durationMs: 842 },
  { id: "tool-1", parentRunId: "run-1", kind: "tool", title: "design.query", detail: "读取 NET_A/NET_B 与禁布区", target: "rev-candidate-007", effect: "read", status: "succeeded", startedAt: now, durationMs: 31 },
  { id: "approval-1", parentRunId: "run-1", kind: "approval", title: "等待 pcb.route_critical 审批", detail: "需要候选设计写入权限", target: "NET_A, NET_B", effect: "design-write", status: "waiting", startedAt: now },
];

export const runtimeEventFixtures: DurableEvent[] = [
  { schemaVersion: "0.1", aggregateId: "session-main", sequence: 1, timestamp: now, sessionId: "session-main", type: "input.admitted", data: { delivery: "queue" } },
  { schemaVersion: "0.1", aggregateId: "session-main", sequence: 2, timestamp: now, sessionId: "session-main", runId: "run-1", type: "run.started", data: { title: "RS-485 局部修复" } },
  { schemaVersion: "0.1", aggregateId: "session-main", sequence: 3, timestamp: now, sessionId: "session-main", runId: "run-1", type: "tool.execution.completed", data: { toolName: "design.query", status: "succeeded" } },
  { schemaVersion: "0.1", aggregateId: "session-main", sequence: 4, timestamp: now, sessionId: "session-main", runId: "run-1", type: "approval.requested", data: { approvalId: "approval-002" } },
];
