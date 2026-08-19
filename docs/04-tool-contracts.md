# Jingwei-Agent 工具合同

> 版本：0.5  
> 日期：2026-08-19  
> 状态：接口草案

## 1. 合同原则

- 工具名表达领域动作，不暴露 GUI 菜单操作；
- 输入引用 project/revision/object ID，避免把整份 ECAD 文件塞进参数；
- 所有写工具要求 `baseRevision`，成功后返回新 revision；
- 所有设计读取和写入都通过 JDP；禁止工具直接改 current JIR snapshot 或外部 EDA projection；
- 所有外部调用返回来源、时间、版本、freshness 和证据引用；
- 所有验证器返回机器可读 findings，不只返回“通过/失败”；
- 工具失败返回结构化错误和恢复建议；
- 高风险阶段工具必须检查前置条件和审批，不能由模型文本绕过；
- dry-run、取消、幂等键和超时是长任务的标准字段。
- 内置、JDP、MCP 和插件工具只有一个规范注册与结算入口；模型可见性不等于执行授权；
- 第三方工具的 description/annotations/effect 声明是提示而非权限事实，runtime 和 daemon 必须独立授权；
- 工具的规范 handler 和协议类型位于 Rust `jingwei-agent-tools`/`jingwei-jdp` crate；
- Tauri、Eval Agent、CLI 和插件只是 client，不能在 client 内复制规则或绕过 `jingwei-daemon` 写设计；
- Rust 类型生成 JSON Schema 和 TypeScript binding，以下 TypeScript 片段仅展示生成后的 client 形态。

## 2. 通用信封

wire schema 由 Rust 类型生成并版本化；前端与 Jingwei Agent Runtime 消费生成 binding，不手工维护同名 interface。

```ts
interface ToolRequest<T> {
  projectId: string
  baseRevision?: string
  idempotencyKey: string
  dryRun?: boolean
  input: T
}

interface ToolSuccess<T> {
  success: true
  executionId: string
  baseRevision?: string
  newRevision?: string
  data: T
  artifactRefs: string[]
  evidenceRefs: string[]
  findings: FindingSummary
  outcome: {
    status: "succeeded" | "succeeded-with-warnings"
    summary: string
  }
}

interface ToolFailure {
  success: false
  executionId: string
  error: {
    code: string
    stage: string
    summary: string
    fingerprint: string
    retryable: boolean
    objectRefs: string[]
    evidenceRefs: string[]
    suggestedActions: SuggestedAction[]
  }
}
```

### 2.1 执行边界

1. client 经本地 JDP 提交 `ToolRequest`，daemon 完成身份、workspace、能力和 base revision 校验；
2. `jingwei-agent-tools` 将请求映射到 Rust domain service，不直接调用 UI 或拼接外部 EDA 文件；
3. 长任务返回 task ID，并通过事件流报告阶段、进度、finding 和可取消状态；
4. 只有 kernel transaction service 可以提交新 revision；adapter、solver 和外部进程只能返回候选结果；
5. client 断线不等于取消，daemon 根据租约、显式 cancel 和发布安全策略决定任务终态；
6. 响应必须携带 daemon build、JDP/schema version 和执行证据，便于复现与审计。

### 2.2 Runtime 工具描述符

JDP 工具的领域 schema 从 Rust 生成；runtime 只补充控制面元数据。所有来源归一为：

```ts
interface RuntimeToolDescriptor {
  name: string
  description: string
  inputSchema: JsonSchema
  outputSchema?: JsonSchema
  source: "builtin" | "jdp" | "mcp" | "plugin"
  permission: string
  effects: Array<"read" | "design-write" | "workspace-write" | "process" |
    "network" | "hardware" | "release">
  resourceClaims: string[]
  idempotency: "required" | "supported" | "unknown"
  cancellation: "supported" | "cooperative" | "unsupported"
  timeoutMs: number
  outputBudget: { inlineBytes: number; overflow: "artifact" | "reject" }
}
```

统一结算顺序为：规范化和 schema 校验 → stale identity 检查 → runtime policy → 可选持久审批 →
持久 `ToolExecution(running)` → handler/JDP/MCP → 输出校验/脱敏/限长 → 持久终态 → durable event。
任何插件不得建立第二个 executor；任何 MCP annotation 不得缩小真实 effect 或扩大权限。

## 3. 工具目录

### 3.1 AI 原生设计内核

| 工具 | 副作用 | 核心输出 | 权限 |
| --- | --- | --- | --- |
| `design_compile_jdl` | 候选 revision | JDL diagnostics、JIR graph、语义 diff | write-design-candidate |
| `design_query` | 无 | 分页的 JIR entities/edges/aggregates | read-design |
| `design_explain` | 无 | requirement/constraint/evidence/solver 决策链 | read-design |
| `design_view_get` | 无 | viewport/layer/LOD 增量 display list | read-design |
| `design_hit_test` | 无 | 坐标对应的稳定对象、候选选择与 snap | read-design |
| `design_cross_probe` | 无 | schematic/PCB/3D/BOM/finding 对象映射 | read-design |
| `design_patch_propose` | 无/候选 | precondition、影响分析、postcondition、patch preview | read-design |
| `design_patch_apply` | 新 revision | 原子 JDP transaction 与 validation delta | write-design |
| `edit_session_open` | draft session | 基于 revision 创建/恢复人工编辑草稿 | write-design-candidate |
| `edit_command_preview` | transient | ghost geometry、snap、影响、局部 finding | read-design |
| `edit_command_commit` | draft revision | 原子语义编辑、diff、validation delta | write-design |
| `edit_undo` / `edit_redo` | draft revision | inverse/reapply operation 与新 revision | write-design |
| `edit_checkpoint` | checkpoint revision | 命名检查点和指定增量验证 | write-design |
| `edit_session_publish` | 候选 revision | 将批准 draft 提升为项目候选 | write-design |
| `design_lock_set` | 新 revision | 对象/网络/区域的 AI 修改策略 | write-design |
| `design_validate_native` | validation run | 原生 ERC/DRC/DFM findings 与 coverage | execute-native-validator |
| `design_solve` | 候选 revisions | 选型/pin/placement/routing 候选及指标 | execute-solver |
| `design_branch` | 新 branch | 独立候选分支 | write-design |
| `design_merge` | 新 revision/冲突 | 三方语义合并或 typed conflicts | write-design |
| `design_project` | 新 projection | KiCad/制造/solver 格式和 object map | execute-adapter |
| `design_import` | 候选 revision | 外部 EDA 映射、coverage、未知特性 | read-external/write-design |
| `design_equivalence_check` | validation run | JIR 与 projection 的连接/几何/BOM 等价性 | execute-validator |

### 3.2 需求与决策

| 工具 | 副作用 | 核心输出 | 权限 |
| --- | --- | --- | --- |
| `requirement_compile` | 创建候选 intent | 约束、假设、未知、冲突、问题 | read/write-project |
| `requirement_baseline` | 创建 intent revision | 已确认需求基线 | approval + write-project |
| `requirement_change_analyze` | 无/报告 | 受影响对象、失效检查和阶段回退 | read-project |
| `decision_record` | 写入决策 | 选择、备选、原因、审批 | write-project |
| `risk_classify` | 无/报告 | 风险等级、禁止项、所需签核 | read-project |

### 3.3 器件与证据

| 工具 | 副作用 | 核心输出 | 权限 |
| --- | --- | --- | --- |
| `part_search` | 外部只读 | 规范化候选与源快照 | network: supplier |
| `part_compare` | 无 | 硬约束矩阵和评分 | read-project |
| `part_select` | 新 revision | 首选/备选器件决定 | write-project + optional approval |
| `part_substitute_validate` | 无/报告 | pin/footprint/electrical/firmware 等价性 | read-project |
| `datasheet_acquire` | 下载受控证据 | 本地 artifact、hash、版本和许可 | network: manufacturer |
| `datasheet_extract` | 创建投影 | pin table、limits、package、证据定位 | read/write-artifact |

### 3.4 库

| 工具 | 副作用 | 核心输出 | 权限 |
| --- | --- | --- | --- |
| `library_resolve` | 可缓存 | 匹配候选、provenance、适配结果 | read-project/network-optional |
| `symbol_generate` | 项目私有库 revision | symbol、pin map、渲染 | write-library |
| `footprint_generate` | 项目私有库 revision | footprint、尺寸表、渲染 | write-library |
| `model3d_resolve` | 下载/生成候选 | STEP、许可、坐标/高度检查 | network/write-library |
| `library_verify` | 报告 | pin-pad、几何、KLC、自定义规则 findings | read-library |
| `library_approve` | 改审批状态 | project/organization approval | named approval |
| `library_diff` | 无 | 上下游版本语义差异和影响 | read-library |

### 3.5 原理图

| 工具 | 副作用 | 核心输出 | 权限 |
| --- | --- | --- | --- |
| `schematic_create` | 新 revision | JIR logical graph、JDL projection、KiCad compatibility projection | write-design |
| `schematic_apply_change` | 新 revision | 增量修改和语义 diff | write-design |
| `schematic_connect` | 新 revision | 明确的 pin/net 连接变更 | write-design |
| `schematic_render` | 新 artifact | PDF/SVG/PNG 审查视图 | execute-kicad |
| `schematic_erc` | 新 validation run | native ERC、KiCad reference ERC 和差异 findings | execute-validator |
| `schematic_verify_custom` | 新 validation run | 电源、接口、封装、地址等领域检查 | read-design |
| `schematic_issue_waive` | 写 waiver | 具名、有范围的豁免 | named approval |

### 3.6 PCB

| 工具 | 副作用 | 核心输出 | 权限 |
| --- | --- | --- | --- |
| `pcb_create_from_schematic` | 新 revision | PCB、footprint、net、未放置队列 | write-design |
| `pcb_profile_apply` | 新 revision | 叠层、网类、规则和工厂 lock | write-design |
| `pcb_mechanical_import` | 新 revision | 板框、孔、禁布/高度区域 | read-external/write-design |
| `pcb_place` | 候选 revisions | 多个布局、指标、seed | execute-optimizer/write-design |
| `pcb_route_critical` | 候选 revision | 关键网络布线和理由 | write-design |
| `pcb_autoroute` | 候选 revisions | 路由结果、未布通网络、指标 | execute-router/write-design |
| `pcb_zone_fill` | 新 revision | 规范化铜皮结果 | execute-kicad/write-design |
| `pcb_drc` | validation run | native DRC、KiCad reference DRC 和差异 findings | execute-validator |
| `pcb_verify_custom` | validation run | 回流、热、装配、测试和几何检查 | read-design |
| `pcb_render` | artifacts | 层图、3D、局部问题截图 | execute-kicad |

### 3.7 制造与发布

| 工具 | 副作用 | 核心输出 | 权限 |
| --- | --- | --- | --- |
| `manufacturing_profile_validate` | 报告 | profile 完整性、新鲜度、适用范围 | read-project |
| `manufacturing_export` | release candidate | 源、Gerber、drill、BOM、CPL 等 | execute-kicad/write-release |
| `manufacturing_package_verify` | validation run | 层集、解析、映射、哈希、一致性 | read-release |
| `dfm_report_import` | 新 artifact/issues | 工厂问题与对象映射 | read-external/write-project |
| `prototype_test_record` | 新 test record | 计划、样本、数据、结论和附件 | named engineer |
| `release_readiness` | 无/报告 | 缺失门禁与有效签核 | read-project |
| `production_release_request` | release request | 固定 candidate/assessment/manifest hash 与缺失 attestation | release-manager request |
| `release_record_verify` | 只读验证/导入引用 | 组织签名 ReleaseRecord 的验证结果 | daemon + signer trust |
| `release_revoke_request` | revoke request | 撤销理由、release/批次影响范围 | release-manager request |

### 3.8 兼容与通用设计操作

| 工具 | 副作用 | 核心输出 | 权限 |
| --- | --- | --- | --- |
| `design_parse` | 缓存投影 | 外部 ECAD AST、JIR 映射、兼容性报告 | read-design |
| `design_diff` | 无 | 基于 JIR entity/edge 的需求/器件/网络/PCB/制造语义 diff | read-design |
| `design_revision_checkout` | 改当前工作副本 | 指定 revision | write-workspace |
| `design_revision_merge` | 新 revision | 合并结果或冲突集合 | write-design |
| `design_open_in_kicad` | 启动/连接 GUI | 实例 ID、文件锁和 IPC session | launch-app |
| `design_locate_object` | GUI 交互 | 选中/高亮对象 | control-kicad |

## 4. 关键工具合同

### 4.1 `design_patch_apply`

```ts
interface JdpPatchInput {
  baseRevision: string
  operations: TypedDesignOperation[]
  preconditions: DesignAssertion[]
  postconditions: DesignAssertion[]
  validationProfile: string
  preserveLockedObjects: boolean
}

interface JdpPatchOutput {
  newRevision: string
  appliedOperationIds: string[]
  semanticDiff: DesignSemanticDiff
  invalidatedRuns: string[]
  validationDelta: FindingDelta
  jdlProjectionRef: string
}
```

任何 precondition 或 hard postcondition 失败都使整个 transaction 回滚。Agent、GUI 和 CLI 必须得到同一结果。

### 4.2 `edit_command_preview` / `edit_command_commit`

```ts
interface EditCommandPreviewInput {
  sessionId: string
  baseDraftRevision: string
  commands: SemanticEditCommand[]
  viewport?: ViewportContext
  preserveLockedObjects: true
}

interface EditCommandPreviewOutput {
  previewId: string
  normalizedCommands: SemanticEditCommand[]
  ghostGeometryRef?: string
  snapResult?: SnapResult
  affectedObjectIds: string[]
  invalidatedRuns: string[]
  findings: FindingDelta
  conflicts: TypedConflict[]
  committable: boolean
}

interface EditCommandCommitInput {
  sessionId: string
  previewId: string
  baseDraftRevision: string
  commandGroupLabel: string
  idempotencyKey: string
}

interface EditCommandCommitOutput {
  newDraftRevision: string
  operationGroupId: string
  semanticDiff: DesignSemanticDiff
  validationDelta: FindingDelta
  undoAvailable: boolean
  currentProjectRevisionChanged: false
}
```

约束：

- drag/draw 中间帧不调用 commit；一次放置、移动、连接、走线或属性确认形成一个 command group；
- preview 只产生临时显示和诊断，不写 JIR；commit 必须重新检查 base revision、锁和所有 hard precondition；
- schematic 命令使用 pin/net/module 等语义对象，PCB 命令使用 footprint/pad/net/track/via/zone/region 等对象；
- 前端不得提交任意几何 blob 作为设计真相；daemon 规范化坐标、snap、连接和规则结果；
- undo/redo 对已提交 command group 创建 inverse/reapply revision，不删除 operation journal；
- `edit_session_publish` 前必须显示 draft 与 project current 的语义 diff，并解决 typed conflict；
- 用户锁定或 `ask-before-change` 对象被命令影响时，preview 必须阻止或要求明确授权。

### 4.3 `requirement_compile`

```ts
interface RequirementCompileInput {
  text: string
  attachmentRefs: string[]
  existingIntentRevision?: string
  defaultProfile?: string
}

interface RequirementCompileOutput {
  candidateIntent: DesignIntent
  extractedRequirements: Requirement[]
  assumptions: Assumption[]
  unknowns: Unknown[]
  conflicts: Conflict[]
  questions: Array<{
    id: string
    question: string
    whyItMatters: string
    blockingBeforeStage: string
    suggestedDefault?: unknown
  }>
}
```

安全、供电、机械、关键接口和制造未知项不得被埋在长段文本里。

### 4.4 `part_search`

```ts
interface PartSearchInput {
  requirementId: string
  category: string
  hardFilters: ParameterConstraint[]
  preferences: WeightedPreference[]
  distributors: string[]
  region: string
  currency: string
  quantityBreaks: number[]
  maxDataAgeHours: number
}
```

结果必须保留每个字段的 source。某个 API 未返回 lifecycle 时使用 `unknown`，不能推断为 active。

### 4.5 `library_resolve`

```ts
interface LibraryResolveInput {
  manufacturer: string
  mpn: string
  packageName: string
  sources: Array<"project" | "kicad-official" | "approved-third-party" | "manufacturer">
  require3dModel: boolean
}

interface LibraryResolveOutput {
  status: "exact" | "compatible-needs-review" | "not-found" | "conflict"
  candidates: LibraryMatch[]
  pinMapCheck: CheckResult[]
  footprintCheck: CheckResult[]
  missingEvidence: string[]
}
```

MPN 或 package 模糊匹配不能返回 `exact`。

### 4.6 `schematic_apply_change`

```ts
interface SchematicChangeInput {
  operations: Array<
    | { kind: "add-symbol"; partDecisionId: string; sheetId: string }
    | { kind: "remove-symbol"; symbolId: string }
    | { kind: "connect"; pinRefs: string[]; netId?: string; netName?: string }
    | { kind: "disconnect"; pinRefs: string[] }
    | { kind: "set-field"; symbolId: string; field: string; value: string }
    | { kind: "add-test-point"; netId: string; requirementRef: string }
  >
  preserveUserLockedObjects: true
}
```

该工具编译为 JDP operations，检查对象身份与前置条件后修改 JIR；不能直接搜索/替换 JDL 或 KiCad 文件。

### 4.7 `pcb_place`

```ts
interface PCBPlaceInput {
  boardRevision: string
  fixedObjectIds: string[]
  regionConstraints: string[]
  candidateCount: number
  timeBudgetSeconds: number
  seeds: number[]
  objectiveWeights: {
    ratsnest: number
    congestion: number
    loopArea: number
    thermal: number
    assembly: number
  }
}
```

输出每个候选的硬约束结果和评分明细。硬约束失败的候选不能因综合分高而被推荐。

### 4.8 `pcb_autoroute`

```ts
interface PCBAutorouteInput {
  boardRevision: string
  routerProfileId: string
  includedNetClasses: string[]
  excludedNetIds: string[]
  timeBudgetSeconds: number
  seeds: number[]
  candidateCount: number
}
```

成功条件包括：router 完成、结果可映射为 JIR、未连接计数符合要求、native DRC 通过、KiCad projection 可保存且
reference DRC 通过、两套结果差异已处置。

### 4.9 `manufacturing_export`

```ts
interface ManufacturingExportInput {
  revision: string
  manufacturerProfileId: string
  variant: string
  outputs: Array<"source" | "gerber" | "drill" | "gerber-job" | "ipc356" |
                 "bom" | "cpl" | "assembly" | "step" | "ipc2581" | "odb">
  candidateLabel: string
}
```

前置检查：revision 不可脏、profile 有效、ERC/DRC 与自定义验证属于该 revision、关键 issue 关闭、库全部批准。
输出目录只能新建，不能覆盖已有候选。

### 4.10 `production_release_request` / `release_record_verify`

```ts
interface ProductionReleaseRequestInput {
  releaseCandidateId: string
  maturityAssessmentId: string
  expectedArtifactManifestHash: string
  releaseNotes: string
}
```

daemon 只创建不可变 release request，并从权威 ledger 查询 attestation/DFM/test record；客户端提交的 actor、role
或 approval ID 不能直接成为发布事实。独立组织 signer 重新验证 request、manifest、assessment、角色和分离职责，
签名后返回 `ReleaseRecord`；daemon 只验证签名和保存引用。缺少 signer 时该能力必须报告 unavailable。

通用 Agent、普通 Runtime approval、shell、文件复制或直接数据库写入都不能创建 Production Release。撤销同样
由组织 release boundary 追加签名 revoke record，不删除旧发布。

## 5. 权限级别

| 级别 | 示例 | 默认策略 |
| --- | --- | --- |
| Read | 解析、diff、查看报告 | 项目内允许 |
| Network read | 供应商/API/数据手册 | 按域名和凭据授权 |
| Candidate write | 需求、库、原理图、PCB 新 revision | 自动或一次项目授权 |
| External process | KiCad reference backend/router/外部渲染 | 固定二进制与参数策略 |
| Waiver | 豁免 ERC/DRC/DFM | 具名工程师确认 |
| Release candidate | 创建制造候选包 | 工程权限 + 前置门禁 |
| Production release | 生产发布 | 多角色签核，不允许模型代签 |
| Revoke | 撤销发布 | 授权负责人，强审计 |

## 6. 幂等、取消与恢复

- runtime 必须在工具副作用前保存 ToolCall/ToolExecution；审批请求和决定也必须分别在展示/唤醒前持久化；
- 同一 `idempotencyKey + baseRevision + normalized input` 只提交一次；
- 长任务定期发布进度，但进度事件不拥有最终状态；
- 取消后等待真实子进程退出并验证临时目录；
- 无法确认退出/回滚时隔离相关 board/library/release 资源；
- router/API 临时失败可在有界策略内重试，确定性 ERC/DRC 违规不可通过重试消失；
- daemon 已提交但 runtime 未收到结果时，必须用 JDP execution/idempotency key 查询，不得重复创建 revision；
- 未知外部副作用先对账，无法证明终态时进入 `needs-reconciliation`/`safety-quarantine`；
- 重启后从持久化 execution、approval、durable event 和 artifact 恢复，不根据 UI 动画推断任务状态。

## 7. 测试要求

每个工具至少有：

- schema 正反例；
- 已被模型发现但缺少 runtime/daemon 授权时仍拒绝执行；
- MCP 恶意 description/annotations/instructions 不扩大权限；
- 路径与权限边界；
- 幂等重复调用；
- 取消与进程退出；
- base revision 冲突；
- 结构化失败与敏感字段脱敏；
- 真实 JDL/JIR/JDP、KiCad reference backend 与适配器契约测试；
- 黄金制品快照或语义回归；
- 故意损坏输入的负向测试。
