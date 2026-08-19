# Jingwei-Agent 系统架构

> 版本：0.6
> 日期：2026-08-19  
> 状态：Accepted；状态所有权与生命周期以 `13-authoritative-boundaries-and-lifecycle.md` 为准

## 1. 架构原则

1. LLM 负责理解、解释、方案生成和在合法工具之间决策；Jingwei 原生内核负责设计事务、约束、验证和投影。
2. 复用 Eval 的通用 Agent 运行时，不复制会话、队列、权限、持久化与失败恢复。
3. JIR revision 是唯一设计语义真相；JDL、UI、KiCad 和制造文件都是受控输入或 projection。
4. 普通对话完成仍遵循轻量工具循环；ERC/DRC/DFM/签核是电子设计领域的发布条件，不是旧式模型评分门。
5. 所有外部库、数据手册、API 结果和自动布线结果都视为需要验证的候选输入。
6. 每个有副作用的工具都必须有工作区范围、事务、取消、回滚和审计语义。
7. Agent、GUI、CLI 和插件使用同一 JDP query/patch 接口，不形成隐藏的第二套编辑逻辑。
8. 原生验证器成熟前采用异构交叉验证，不用“自研”降低生产门槛。
9. JDL/JIR/JDP、领域规则、事务、adapter 和制造工具使用 Rust 作为规范实现语言。
10. `jingwei-daemon` 是工作区唯一状态所有者；Tauri/React 只持有视图状态并通过认证 JDP 调用能力。
11. Codex App Server/SDK 与 OpenCode V2 只提供协议和可靠性参考；Jingwei 不整体 fork 框架，也不运行第二套
    session/tool owner。
12. 固定工程阶段使用确定性 workflow，Main Agent 只拥有开放推理和候选生成；模型不能跳过发布门禁。

## 2. 总体分层

```text
┌───────────────────────────────────────────────────────────────┐
│ JDP Clients                                                   │
│ Tauri Workbench │ Jingwei Agent Runtime (Eval-derived)       │
│ CLI │ CI │ Plugins                                            │
│ views/interaction  owner/tool loop      headless automation   │
└──────────────────────────────┬────────────────────────────────┘
                               │ authenticated local JDP
                               │ query / patch / task / event
┌──────────────────────────────▼────────────────────────────────┐
│ jingwei-daemon (Rust; one writer per workspace)               │
│ auth / lock / transaction / revision / task / cancel/recovery │
├───────────────────────────────────────────────────────────────┤
│ Jingwei Design Kernel                                         │
│ JDL / JIR / constraints / ERC/DRC/DFM / dependency graph     │
├──────────────────────────────┬────────────────────────────────┤
│ Native Solvers & Renderers   │ Reference/External Adapters    │
│ library / placement / route  │ KiCad / supplier / router/DFM │
│ geometry / display / export  │ manufacturer                   │
└──────────────────────────────┴───────────────┬────────────────┘
                                               │ atomic commit
┌──────────────────────────────────────────────▼────────────────┐
│ JIR Revision Store + Project Workspace + Artifact Store       │
│ JDL / JIR / operations / evidence / projections / releases    │
└───────────────────────────────────────────────────────────────┘
```

### 2.1 进程职责

- `jingwei-daemon` 持有 workspace lock、current revision、operation journal 和长任务状态，是唯一写入者；
- Tauri 2 启动或附着 daemon sidecar，负责窗口、安装/更新、系统集成与最小权限桥接；
- React/TypeScript 只保存面板、选择、viewport 等视图状态，以 daemon 返回的 revision 为最终事实；
- Jingwei Agent Runtime 从 Eval runtime 抽取，保持 TypeScript/Bun 控制面；它直接作为 JDP client 调用领域
  工具，不通过前端或截图间接操作设计；
- CLI/CI 使用同一 client 和协议；不存在 daemon 时可启动受同一 workspace lock 保护的 headless 实例；
- Windows 使用命名管道，macOS/Linux 使用 Unix domain socket；默认不开放 localhost TCP。

## 3. 运行时复用边界

### 3.1 直接复用

- 单一稳定 `main-agent` 会话 owner；
- 原生工具调用循环与结构化工具结果；
- 用户命令 durable FIFO、多会话隔离、取消和恢复；
- 用户授权、工作区路径、命令和网络权限；
- SQLite 会话/任务/工具执行审计；
- revisioned artifact 与前端投影；
- 结构化失败证据、失败指纹去重和合法替代路径；
- 只有根主 Agent 可创建子任务，子 Agent 结果返回主 Agent。
- provider adapter、模型尝试/fault-domain fallback、MCP host 和敏感信息脱敏。

### 3.2 可复用但需要升级

- `schematic_read`：保留为读取 PDF/图片和审查参考资料的工具，不能承担 ECAD 源文件生成；
- `datasheet_read`：升级为页码/表格/封装图可引用的证据提取器；
- `pin_extract`：从生成固件头文件升级为可输出标准化 pin model，但不得替代库引脚验证器；
- 文档附件：增加数据手册版本、来源 URL、内容哈希和页级证据；
- artifact：增加设计对象稳定 ID、源 revision、生成器和依赖图。
- session input：增加 durable admission、client request ID、明确 `steer`/`queue` 和同 session join/coalesced wake；
- event：增加聚合内 sequence、read-after + live replay，UI 不再依赖进程内事件恢复状态；
- tool registry：内置/JDP/MCP/plugin 归一为单一 descriptor 和 settlement pipeline，集中做 schema、权限、
  output bounding、artifact 化与持久终态；
- approval：保持“请求先持久化再展示/等待、决定先持久化再恢复”，并限制 `allow-all` 的 session/资源范围；
- observability：使用 OpenTelemetry 贯通 runtime → JDP → daemon，同时保持审计数据库独立。

### 3.3 Jingwei 新增

- 需求合同与工程风险分类；
- Rust JDL AST/compiler/formatter、JIR schema/store、JDP server 与生成的 client bindings；
- 原生 query、事务 patch、branch/diff/merge、影响分析和增量编译；
- 器件搜索、选型、替代料和供应链快照；
- 符号、封装、3D 模型解析/生成/验证；
- 原生 logical/physical design graph、renderer 和可视化编辑；
- KiCad 等 reference backend 的结构化导入/导出；
- 约束、布局、布线、铜皮和丝印处理；
- ERC、DRC、DFM、网表/制造数据一致性验证；
- 制造商 profile 和制造包发布；
- 工程审查、样板验证与 Production Release 签核。

### 3.4 参考但不直接依赖

- Codex App Server/SDK：参考 thread、resume、approval、event stream 和产品内嵌边界；
- OpenCode V2：参考 durable input、steer/queue、per-session run coordinator、durable event 和单一 tool
  settlement；其 V2 中明确未完成的 continuation/maintenance 能力不得当作现成功能；
- OpenAI Agents SDK：参考 Main Agent 把专业 Agent 当受限工具调用的 manager pattern；
- LangGraph/Microsoft Agent Framework/Temporal：仅作为 Phase 3 后云端或企业 workflow adapter 候选；
- MCP：外部集成标准，不取代内部 JDP；OpenTelemetry：观测标准，不拥有业务终态。

完整选型、不变量、源码证据和 Phase 0 门禁见 `12-agent-runtime-reference-architecture.md`。

## 4. 控制面与设计面

### 4.1 控制面

Runtime 控制面保存 session、input、run、model attempt、ToolExecution、Runtime approval 和 durable event。
它可以缓存带 daemon epoch/sequence 的 `observedRevision`、finding、gate 和 release projection，但不拥有项目
project head、draft head、change proposal、对象锁、waiver validity、阶段或签核终态。后者由 daemon/release domain 根据持久
设计事实计算。JDP transaction 与 ToolExecution 关联，但 EDA 对象和设计指针只由 JIR store 管理。

关键对象：

- Runtime owner：`Session`、`Input`、`Run`、`ModelAttempt`、`ToolExecution`、`ApprovalRequest`；
- daemon owner：`Project`、`DesignRevision`、`DraftSession`、`ChangeSet`、`DesignIntent`、`Issue`、`Waiver`、
  `ValidationRun`、`MaturityAssessment` 和 `ArtifactManifest`；
- organization release boundary：`ReleaseRecord`、角色 attestation、签名与 revoke record。

完整矩阵见 `13-authoritative-boundaries-and-lifecycle.md`。任何 Runtime/UI 中的项目阶段都只是可重建投影。

### 4.2 设计面

设计面以 JIR revision 与 operation journal 为唯一真相，保存 JDL、项目私有库、规则、制造 profile、报告和
导出 projection。它必须能脱离 Jingwei UI 通过 headless kernel 查询、修改、验证和导出。

禁止把整个 JIR、`.kicad_sch` 或 `.kicad_pcb` 作为不可查询的大字符串塞进会话消息。模型只通过 JDP 接收
与当前任务有关的局部 graph、语义 diff、渲染图和证据链接。

### 4.3 原生工作台编辑面

原理图和 PCB 不是只读 projection。AI 生成候选 revision 后，Tauri 工作台通过 JDP 打开交互式语义视图，
用户可查看、选择和人工编辑对象。图片、PDF、SVG、Gerber 和 3D 截图只用于导出/审查，不能替代编辑面。

```text
JIR revision
  → Rust viewport/display-list query
  → Tauri schematic/PCB canvas
  → transient drag/draw preview
  → semantic edit command
  → daemon precondition + geometry/electrical validation
  → draft revision + incremental ERC/DRC
  → checkpoint/current candidate
```

边界规则：

- pointer move/drag frame 只形成 transient preview，完成一次逻辑动作后才提交语义 operation；
- hit-test、snap、connectivity、规则和最终坐标由 Rust 内核判定，前端不成为几何/电气真相；
- 人工编辑与 AI patch 使用相同 base revision、precondition、transaction、semantic diff 和 validation delta；
- draft session 与 project current revision 分离，异常退出后可恢复草稿但不能污染 current pointer；
- undo/redo 使用可逆 operation；对已提交历史生成 inverse/reapply revision，不改写旧 revision；
- 人工对象可设为 `ai-editable`、`ask-before-change` 或 `locked`，AI/solver 默认保留后两类保护；
- 原理图、PCB、3D、BOM、finding 与 AI 引用通过稳定对象 ID cross-probe；
- stale base 或并发编辑产生语义 rebase/typed conflict，禁止 last-write-wins。

详细交互范围和验收见 `11-native-workbench-preview-and-editing.md`。

## 5. 领域阶段

建议使用以下阶段状态，而不是让模型自由跳过工程步骤：

```text
intake
  → intent-baselined
  → semantic-compiled
  → architecture-selected
  → components-resolved
  → libraries-verified
  → schematic-candidate
  → schematic-verified
  → pcb-constrained
  → placement-candidate
  → routing-candidate
  → pcb-verified
  → manufacturing-candidate
  → engineering-verified
  → prototype-verified
  → production-released
```

阶段状态是对持久事实的投影：例如 `schematic-verified` 必须引用具体 revision 的 ERC/自定义检查报告。
它不会强迫普通 LLM 继续生成，也不会把正常对话判失败；它只控制高风险工具（例如制造发布）能否执行。

### 5.1 阶段回退

- 需求硬约束变化：至少回到 `intent-baselined` 后重新做影响分析；
- 关键 IC/封装变化：使库、原理图、PCB 和发布验证失效；
- 仅丝印文字变化：可以只重跑 PCB/制造输出相关检查；
- 制造商 profile 变化：至少使 PCB DRC/DFM 和制造发布失效；
- 自动布线器或 KiCad 版本变化：生成新 revision 并执行黄金回归。

## 6. 主 Agent 与专业任务

Main Agent 保持用户会话、需求和最终交付所有权。可按需调用以下专业任务，但它们都返回结构化候选结果，
不能自行发布：

- `requirements-engineer`：编译需求、识别未知与冲突；
- `component-engineer`：候选器件、供应风险和替代关系；
- `library-engineer`：符号/封装/3D 解析、生成和验证；
- `schematic-engineer`：模块划分、原理图候选和电气问题；
- `pcb-engineer`：约束、布局、布线候选；
- `manufacturing-engineer`：DFM、BOM/CPL、制造包；
- `verification-engineer`：独立执行确定性检查并形成 issue ledger。

只有 Main Agent 可以发起这些任务。专业任务不得再创建同级/下级 Agent，也不得直接改写 Production Release。

专业任务采用 agents-as-tools：输入、输出、预算、可见工具和资源 claim 均显式声明。需求基线、库批准、ERC/
DRC/DFM、制造一致性、样板与发布阶段使用确定性 workflow；Agent 可以生成候选和解释缺口，但不能决定跳过
哪个强制节点。

### 6.1 Agent runtime 与 JDP 调用链

```text
durable user input
  → one provider turn
  → canonical tool call
  → runtime policy / durable approval
  → durable ToolExecution
  → generated JDP client
  → daemon authorization / transaction / evidence
  → bounded structured tool result
  → next provider turn or normal completion
```

同一 session 的 provider continuation 串行，不同 session 可并行。JDP 设计写入还需要按 board/page/library/
release 等资源 claim 串行化；工具可被模型看到不代表其已获执行权限。

## 7. 事务与并发

### 7.1 修改事务

每次设计修改遵循：

1. 基于明确的 base revision 创建隔离工作副本；
2. 解析并验证输入；
3. 应用变更并保存源文件；
4. 用锁定 EDA 工具 open/save 或 parse/serialize round-trip；
5. 运行与影响范围匹配的最小检查；
6. 成功后原子提交新 revision 和 artifact manifest；
7. 失败则保留诊断，不移动项目 current pointer。

### 7.2 写冲突

- 原理图页、PCB、库条目、规则文件和发布目录都属于显式资源；
- 同一资源只允许一个写事务；
- 独立读取和外部搜索可并行；
- 无法确认外部进程停止或工作区回滚时进入 `safety-quarantine`；
- 用户手工在 KiCad 中编辑时建立外部编辑锁；完成后导入为新 JIR revision 并执行语义 diff。

## 8. AI 原生 EDA 读写策略

### 8.1 原生设计事务

- JDL、GUI、Agent 和 importer 全部编译为 JDP operations；
- JDP patch 必须携带 base revision、precondition 和 postcondition；
- JIR entity 使用稳定 UUID 和有类型 relationship；
- transaction 成功后原子创建新 revision，失败不移动 current pointer；
- 使用依赖图只失效和重跑受影响的编译/验证阶段；
- JDL formatter 生成确定性、可审查的文本 projection；
- 任意 TypeScript/Python 扩展只在沙箱运行，并只能提交验证后的 JDP/JDL。

### 8.2 Reference backend

KiCad S-expression AST、CLI 和 IPC 都放在 anti-corruption adapter 后面：

- `JIR → KiCad` 是确定性 projection，并记录 object mapping；
- `KiCad → JIR` 是受控 import，不允许直接更新已发布 revision；
- 生成器 ID 使用 `jingwei-agent`，不冒充 `pcbnew`；
- 对未知字段采用隔离/保留策略；无法无损导入时拒绝覆盖；
- 保存后由锁定 KiCad 版本打开/保存并重新解析；
- projection connectivity、placement、geometry、BOM 与 JIR 做等价性检查。

### 8.3 KiCad GUI IPC

KiCad IPC API 只用于兼容工作流中的选择、定位和外部人工编辑同步。官方文档说明 KiCad 9/10 的 IPC API
需要连接运行中的 GUI且能力仍在扩展，因此它不是 Jingwei 原生内核或 CI 的必要条件。

### 8.4 外部 CLI 与交叉验证

首期 `kicad-cli` 作为 ERC、DRC、BOM、Gerber、钻孔、IPC-2581、ODB++ 的 reference backend。每次调用保存
命令、版本、退出码、输入/输出哈希，并与 native result 比较；任何差异形成结构化 finding。

## 9. 建议代码结构

```text
jingwei-agent/
  Cargo.toml                   # 单一 Rust workspace
  rust-toolchain.toml
  crates/
    jingwei-jdl/               # parser、AST、type checker、formatter
    jingwei-jir/               # schema、graph、index、revision、migration
    jingwei-jdp/               # query、patch、transaction、diff、events
    jingwei-kernel/            # domain、dependency graph、release policy
    jingwei-store/             # journal、CAS、workspace lock
    jingwei-geometry/          # shape、spatial index、boolean operations
    jingwei-library/           # symbol/footprint/3D 解析生成验证
    jingwei-components/        # MPN、候选评分、供应商适配
    jingwei-erc/               # 原理图与领域电气检查
    jingwei-drc/               # PCB、连通性和制造规则
    jingwei-placement/         # 布局约束与候选生成
    jingwei-routing/           # 路由器适配与候选评分
    jingwei-kicad/             # reference backend：AST、CLI、IPC
    jingwei-manufacturing/     # profile、导出、包验证
    jingwei-agent-tools/       # Eval 工具合同映射
    jingwei-daemon/            # 唯一状态 owner 与 JDP server
    jingwei-cli/               # headless/CI client
  app/
    src/                       # React/TypeScript presentation only
    src-tauri/                 # Tauri 2 shell、sidecar、capabilities、updater
  bindings/
    typescript/                # 从 Rust schema 自动生成
  sdk/                         # 受控 TypeScript/Python 高级 SDK
  profiles/
    manufacturers/            # 经版本控制的制造能力/输出映射
    risk/                      # 产品风险分类与所需签核
  testdata/
    golden/                    # 可公开、可复现的黄金板
    invalid/                   # 已知错误库/设计/制造包
  docs/
```

核心 crate 不依赖 Tauri、React 或 Eval。最终目录可根据 Eval 复用方式调整为 monorepo 成员，但禁止把设计
语义拆成前端和 daemon 两套实现。更完整边界见 `10-rust-tauri-platform-architecture.md`。

## 10. 服务接口

### 10.1 Domain Service

```rust
trait DesignService {
    async fn compile_intent(&self, input: RequirementInput) -> Result<DesignIntentResult>;
    async fn compile_jdl(&self, input: JdlSource) -> Result<CompileResult>;
    async fn query(&self, request: JdpQuery) -> Result<JdpQueryResult>;
    async fn explain(&self, request: JdpExplain) -> Result<JdpExplanation>;
    async fn propose_patch(&self, request: JdpPatch) -> Result<ChangeProposal>;
    async fn apply_patch(&self, proposal_id: ProposalId) -> Result<DesignRevisionResult>;
    async fn validate(&self, revision: RevisionId, profile: ValidationProfile) -> Result<ValidationRun>;
    async fn solve(&self, revision: RevisionId, request: SolveRequest) -> Result<Vec<DesignCandidate>>;
    async fn project(&self, revision: RevisionId, backend: ReferenceBackend) -> Result<ProjectionResult>;
    async fn export_manufacturing(
        &self,
        revision: RevisionId,
        profile: ManufacturerProfile,
    ) -> Result<ReleaseCandidate>;
}
```

该 trait 是 daemon 内部规范边界；JDP schema 和 TypeScript client 由 Rust 类型生成。GUI 不直接链接 store，
即使某个 client 与 daemon 位于同一安装包，也必须保留相同授权、事务和审计语义。

### 10.2 Adapter 原则

- 所有适配器返回规范化数据和原始响应摘要；
- 适配器不拥有跨源器件合并逻辑；
- 网络失败、额度、凭据缺失和数据过期要显式区分；
- 适配器版本属于 artifact manifest；
- 云端适配器执行前进行数据外发授权检查。
- 外部 adapter 类型不得泄漏为 JIR 核心类型；所有映射都有等价性和 coverage 报告。

## 11. 失败恢复

工具失败统一返回：

```ts
interface DomainToolFailure {
  code: string
  stage: string
  summary: string
  objectRefs: string[]
  evidenceRefs: string[]
  retryable: boolean
  fingerprint: string
  suggestedActions: Array<{
    kind: "change-input" | "use-alternative" | "request-user" | "repair" | "abort-release"
    description: string
  }>
}
```

运行时禁止原样重复相同工具和输入。Main Agent 可修改参数、改选数据源、缩小设计范围、请求用户决策或
修复上游对象。只有真实外部条件、用户等待/取消或安全隔离才形成阻塞。

恢复时遵守：输入先持久化再唤醒；ToolExecution 先持久化再产生副作用；审批请求/决定先持久化再等待/唤醒；
daemon 已提交但 runtime 未收到响应时用 execution/idempotency key 对账。无法证明外部副作用终态时禁止盲重试，
进入 `needs-reconciliation` 或 `safety-quarantine`。UI 使用 durable event sequence 回放，不用内存动画推断状态。

## 12. 可观测性

每个设计 run 使用 OpenTelemetry trace context 串联 UI、Agent runtime、JDP、daemon、adapter 和 provider，至少记录：

- sessionId、projectId、runId、requestId；
- base/new revision、工具与模型版本；
- 输入/输出 artifact 哈希；
- EDA 命令、退出码、耗时和资源使用；
- 新增/解决/豁免的问题；
- 外部 API 来源、时间戳与响应 freshness；
- 用户审批、工程签核和发布状态变化。

日志/trace 不记录 API 密钥、私有库完整内容、完整模型上下文或未经授权的原始数据手册正文。OpenTelemetry
用于诊断和性能分析；不可变审计、ToolExecution、JIR revision 和 release record 仍是业务真相。
