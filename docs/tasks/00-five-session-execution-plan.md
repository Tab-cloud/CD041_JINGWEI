# Jingwei-Agent 五会话并行开发总任务书

> 版本：1.0  
> 日期：2026-08-19  
> 用途：把当前产品/架构需求拆成五个可分别委派、可独立开发、最终统一验收合并的功能任务。  
> 当前波次：Phase 0 技术基线 + Phase 1 最小纵向切片，不代表完整量产能力已经完成。

## 1. 当前设计需求总览

### 1.1 产品目标

Jingwei-Agent 接收自然语言需求、参考资料和结构化约束，自动完成需求基线、器件选择、库解析/生成、原理图、
PCB 约束、布局布线、验证和制造候选包。AI 生成的原理图与 PCB 必须能在原生工作台预览、人工编辑、撤销、
比较，并继续交给 AI 局部修改。

首期只承诺低压、非安全关键、常规 2/4 层 FR-4 板。Gerber 已生成不等于“可量产”；Production Release
仍需要确定性 ERC/DRC/DFM、制造商反馈、真实样板测试和具名多角色签核。

### 1.2 不可破坏的架构约束

1. `JIR` 是唯一规范化设计真相；JDL、KiCad、Gerber、BOM 和 UI 都是输入或 projection；
2. `jingwei-daemon` 是每个工作区唯一 JIR/JDP 写入者；
3. Agent、GUI、CLI、importer 和插件只能通过 JDP transaction 修改设计；
4. Rust 是 JDL/JIR/JDP、设计事务、规则、adapter 和制造工具的规范实现语言；
5. Agent 控制面复用 Eval 的 TypeScript/Bun runtime，不从零重写，不形成第二个 JIR owner；
6. Main Agent 是会话和最终答复 owner，专业 Agent 作为有预算/schema 的工具；
7. 固定工程阶段、验证和发布使用确定性 workflow，不由模型自行跳过；
8. Tauri/React 只拥有视图和临时交互状态，不拥有设计语义、最终坐标或发布权限；
9. MCP 只做受控外部集成；第三方 instructions/description/annotations/output 都是不可信输入；
10. 未知副作用、无法确认的外部进程或回滚状态进入 `needs-reconciliation`/`safety-quarantine`，禁止盲重试。

### 1.3 端到端主链路

```text
自然语言/附件
  → durable Agent session
  → DesignIntent 与定向追问
  → 精确 MPN 选择与证据
  → symbol/footprint/pin-pad 验证
  → JIR schematic + native ERC
  → PCB constraints + placement/routing candidates
  → native DRC + KiCad reference checks
  → Gerber/drill/BOM/CPL/manifest
  → 工程师在原生工作台审查/编辑
  → manufacturing candidate
  → 工厂 DFM + 样板 + 具名签核
  → production release
```

本轮五会话的联合目标止于 **可审查的 Manufacturing Candidate 原型**。任何会话都不得宣称已达到真实量产发布。

## 2. 五个功能点

| 会话 | 功能点 | 主要所有权 | 第一波必交结果 |
| --- | --- | --- | --- |
| 01 | Agent Runtime 控制面 | session/run/provider/tool/policy/approval/event/MCP/OTel | 可恢复的 Agent loop，通过生成/模拟 JDP client 调工具 |
| 02 | Rust 语义内核与 daemon | JDL/JIR/JDP/revision/store/transaction/IPC/CLI/bindings | 一个可编译、可查询、可原子 patch、可恢复的最小设计工程 |
| 03 | 器件、库与原理图 | DesignIntent/Part/Library/Schematic/ERC/schematic view | 黄金板逻辑设计、库验证和 native ERC 最小闭环 |
| 04 | PCB、验证与制造 | Board/constraints/placement/routing/DRC/KiCad/manufacturing | 黄金板 PCB 候选、DRC 和可解析制造候选包 |
| 05 | Tauri 原生工作台 | Runtime UI、schematic/PCB canvas、人工编辑、diff/approval | 通过 mock/真实 client 展示并编辑黄金板候选 |

### 2.1 需求覆盖矩阵

| 需求组 | 主责会话 | 协作/验收 | 说明 |
| --- | --- | --- | --- |
| REQ 需求与项目 | 03 | 01/02/05 | 03 定义 DesignIntent；01 编排追问；02 保存 revision；05 展示确认 |
| CMP 器件选型 | 03 | 01/05 | 精确 MPN、硬过滤、软评分、供应快照、替代料 |
| NAT AI 原生内核 | 02 | 01/03/04/05 | 02 拥有 JDL/JIR/JDP；其他会话只扩展/消费 |
| LIB 元器件库 | 03 | 04/05 | 03 生成/验证；04 消费 footprint；05 审查/定位 |
| SCH 原理图 | 03 | 02/05 | 03 逻辑模型/ERC/view/edit provider；02 事务；05 交互 |
| PCB 物理设计 | 04 | 02/03/05 | 04 几何/约束/布局布线/DRC；03 提供 netlist；05 交互 |
| VAL 验证 | 03/04 | 02/05/统一验收 | 03 ERC，04 DRC/制造；02 记录 run；05 展示；统一验收做端到端 |
| MFG 制造候选 | 04 | 02/05/统一验收 | 04 导出验证，02 revision/manifest 引用，05 展示；真实 DFM/样板不在本轮 |
| UX 用户体验 | 05 | 01/02/03/04 | 05 负责呈现与交互，事实来自四个后端域 |
| AGT Agent runtime | 01 | 02/05 | 01 控制面，02 JDP，05 Runtime UI |
| 安全/权限/审计 | 01/02 | 03/04/05/统一验收 | runtime 与 daemon 双层授权；各领域处理不可信输入；统一负向验收 |
| 发布门禁 | 04 提供 readiness | 01/02/05/统一验收 | 本轮只证明缺口并阻止发布；具名签核、DFM、样板后续实现 |
| 跨平台/打包 | 05 | 01/02/统一验收 | Windows 首发；真实 runtime/daemon sidecar 合入后重新打包验收 |
| 许可证/第三方 | 03/04 | 01/02/05/统一验收 | 每个依赖 owner 提供 license/NOTICE；统一验收审计安装包 |

没有任何会话单独拥有“产品已完成”判断。跨会话的验证、真实 UI、安装包和制造候选必须在统一验收分支重新执行。

对应任务文档：

- [01-agent-runtime-control-plane.md](01-agent-runtime-control-plane.md)
- [02-rust-jir-jdp-kernel-daemon.md](02-rust-jir-jdp-kernel-daemon.md)
- [03-component-library-schematic.md](03-component-library-schematic.md)
- [04-pcb-validation-manufacturing.md](04-pcb-validation-manufacturing.md)
- [05-tauri-native-workbench.md](05-tauri-native-workbench.md)

## 3. 并行开发前置条件

### 3.1 Git/工作区

当前 `D:\Projects\CD041_LUBAN` 不是 Git worktree，`jingwei-agent` 目前主要是文档目录。五个会话开始写代码前，
集成负责人必须先：

1. 把 `jingwei-agent` 建成独立 Git 仓库，提交当前文档基线；
2. 建立五个独立 branch/worktree，禁止五个会话同时写同一物理目录；
3. 推荐分支：
   - `feat/jw-01-agent-runtime`
   - `feat/jw-02-kernel-daemon`
   - `feat/jw-03-logical-design`
   - `feat/jw-04-physical-manufacturing`
   - `feat/jw-05-workbench`
4. 每个会话只在自己的 worktree 工作，不提交其他会话的产物；
5. 不在会话内自行合并其他分支；统一验收会话按本文第 10 节顺序合并。

如果暂时不能建立 worktree，只允许先做接口/测试/代码草案，不允许五个会话在同一目录并发编辑。

### 3.2 基线文档

所有会话开始前必须阅读：

- `README.md`
- `docs/00-product-requirements.md`
- `docs/01-system-architecture.md`
- `docs/04-tool-contracts.md`
- `docs/06-security-compliance-and-audit.md`
- `docs/07-roadmap-and-acceptance.md`
- `docs/08-open-questions-and-decisions.md`
- 本总任务书和本会话专属任务书。

进入 `embedded-agent` 读取/修改参考代码的会话还必须遵守其 `AGENTS.md`。OpenCode 只作参考，修改其源码不是
本轮任务。

### 3.3 启动节奏与接口脚手架

五个会话可以同时开始阅读、测试设计、fixture 和 mock 工作，但 Rust 领域代码需要一个可编译的 core 基线。
推荐采用以下启动节奏：

1. 会话 02 首先提交一个小型 `contract-pack-v0` 脚手架：Cargo workspace、JIR/JDP 基础 ID/信封、extension
   traits、schema/codegen 命令和空 registry；该提交不等待完整 daemon；
2. 集成负责人把该提交作为五个 worktree 的共同基线，或让会话 01/03/04/05 rebase/cherry-pick 后再进入正式
   接口实现；
3. 在脚手架到达前，会话 01/05 可开发 versioned mock，03/04 可准备独立 fixture、算法和 contract test，
   但不得各自创建第二套正式 `RevisionId`、JDP 信封或 extension trait；
4. 如果 `contract-pack-v0` 与本文协议不一致，会话 02 必须先提交 interface change proposal，由集成负责人裁决，
   不能让五个分支各自漂移。

这不是把五个会话改成串行：只有共享类型脚手架是短前置，其余实现仍然并行。

## 4. 目录和文件所有权

建议目标目录：

```text
jingwei-agent/
  Cargo.toml                         # 会话 02
  Cargo.lock                         # 集成生成；见 4.1
  rust-toolchain.toml                # 会话 02
  package.json                       # 会话 01
  pnpm-lock.yaml                     # 集成生成；见 4.1
  pnpm-workspace.yaml                # 会话 01
  crates/
    jingwei-jdl/                     # 会话 02
    jingwei-jir/                     # 会话 02
    jingwei-jdp/                     # 会话 02
    jingwei-kernel/                  # 会话 02
    jingwei-store/                   # 会话 02
    jingwei-daemon/                  # 会话 02
    jingwei-cli/                     # 会话 02
    jingwei-components/              # 会话 03
    jingwei-library/                 # 会话 03
    jingwei-schematic/               # 会话 03
    jingwei-erc/                     # 会话 03
    jingwei-kicad-schematic/         # 会话 03
    jingwei-geometry/                # 会话 04
    jingwei-pcb/                     # 会话 04
    jingwei-placement/               # 会话 04
    jingwei-routing/                 # 会话 04
    jingwei-drc/                     # 会话 04
    jingwei-kicad-pcb/               # 会话 04
    jingwei-manufacturing/           # 会话 04
  packages/
    agent-runtime/                   # 会话 01
    agent-tools/                     # 会话 01
    agent-policy/                    # 会话 01
    agent-events/                    # 会话 01
    agent-mcp/                       # 会话 01
    agent-observability/             # 会话 01
    agent-protocol/                  # 会话 01
  app/                               # 会话 05
  bindings/typescript/               # 会话 02 生成；其他会话只消费
  profiles/manufacturers/            # 会话 04
  testdata/kernel/                   # 会话 02
  testdata/logical/                  # 会话 03
  testdata/physical/                 # 会话 04
  docs/tasks/                        # 统一任务书；各会话不得改写
  docs/handoffs/task-0N/             # 各会话只写自己的交接证据
  docs/interface-change-proposals/   # 各会话提交跨域接口提案
```

所有 Cargo workspace member 使用 `crates/*`，所有 pnpm member 使用 `packages/*` 与 `app`，避免各会话为了注册
自己的模块同时修改根 manifest。

### 4.1 共享生成文件例外

`Cargo.lock` 和 `pnpm-lock.yaml` 是唯一允许在多个任务分支出现差异的共享生成文件：

- 各会话可以为本分支可复现测试生成并提交与自身 manifest 对应的 lockfile 变化；
- 会话不得手工编辑 lockfile，也不得用 lockfile 夹带其他会话依赖；
- 统一合并时不逐行手工拼接冲突：先合并所有 `Cargo.toml`/`package.json`，再由集成负责人用固定工具链重新生成；
- 重新生成后必须执行 Rust locked build/test，以及前端 frozen-lockfile install/build；
- 最终 lockfile、工具链版本和依赖许可证清单属于统一验收证据。

除此之外，任何共享文件冲突仍按目录所有权处理，不以“生成文件”为由扩大修改范围。

## 5. 共享协议基线 v0

本节是五个会话共同遵守的线协议语义。会话 02 拥有 Rust 规范类型和 codegen；会话 01/05 消费生成 binding。
在生成 binding 尚未合并前，可以使用同 shape 的测试 fixture，但不得另立正式类型。

### 5.1 基础标识

所有 ID 均为不透明 UTF-8 字符串，wire 上不得依赖 UUID 的具体版本：

```text
WorkspaceId  ProjectId  RevisionId  EntityId  EdgeId
RequestId    IdempotencyKey  TaskId  ExecutionId
SessionId    RunId  ToolCallId  ApprovalId  ArtifactId  EvidenceId
```

物理量在线协议中使用 `{ value, unit }`，禁止裸浮点数表达长度、电压、电流、温度或角度。

### 5.2 JDP 请求/响应信封

```ts
interface JdpRequest<T> {
  protocolVersion: "0.1"
  requestId: string
  idempotencyKey: string
  workspaceId: string
  projectId: string
  operation: string
  baseRevision?: string
  deadlineMs?: number
  dryRun?: boolean
  input: T
}

interface JdpSuccess<T> {
  success: true
  requestId: string
  executionId: string
  baseRevision?: string
  newRevision?: string
  data: T
  artifactRefs: string[]
  evidenceRefs: string[]
  findings: FindingSummary
}

interface JdpFailure {
  success: false
  requestId: string
  executionId?: string
  error: {
    code: string
    stage: string
    summary: string
    fingerprint: string
    retryable: boolean
    objectRefs: string[]
    evidenceRefs: string[]
    suggestedActions: Array<{
      kind: "change-input" | "use-alternative" | "request-user" | "repair" | "abort-release"
      description: string
    }>
  }
}
```

写操作必须带 `baseRevision` 和 `idempotencyKey`。缺失、stale base、precondition 失败或权限不足必须返回 typed
failure，禁止 last-write-wins。

### 5.3 Durable event

```ts
interface DurableEvent<T> {
  schemaVersion: "0.1"
  aggregateId: string
  sequence: number
  timestamp: string
  sessionId?: string
  runId?: string
  taskId?: string
  traceId?: string
  type: string
  data: T
}
```

同一 `aggregateId` 的 `sequence` 严格递增。client 重连必须先 `readAfter(sequence)`，再接 live stream。token delta
可以是易失事件；run/tool/approval/revision/task/finding 终态必须持久化。

### 5.4 Runtime 本地 API

会话 01 提供、会话 05 消费：

```text
session.create
session.get
session.input.admit(delivery=steer|queue, clientRequestId)
session.run.resume
session.run.cancel
session.events(afterSequence)
approval.list
approval.reply
artifact.get
runtime.capabilities
```

### 5.5 JDP 核心 API

会话 02 提供；会话 01、03、04、05 消费或扩展：

```text
project.create/get
design.compile_jdl
design.query/explain
design.patch.propose/apply
design.diff
design.validate
task.get/cancel/events
artifact.get
workspace.capabilities
```

会话 03/04 通过模块注册贡献领域工具，不直接增加另一个 daemon endpoint。

### 5.6 Rust 扩展边界

精确 Rust 类型由会话 02 落地，但语义必须覆盖：

```rust
trait DomainToolModule {
    fn descriptors(&self) -> Vec<ToolDescriptor>;
    async fn execute(&self, ctx: ToolContext, request: ToolRequest) -> ToolResult;
}

trait ValidationProvider {
    async fn validate(&self, revision: RevisionId, scope: ValidationScope) -> ValidationRun;
}

trait ViewProvider {
    async fn view(&self, revision: RevisionId, request: ViewRequest) -> DisplayList;
    async fn hit_test(&self, revision: RevisionId, request: HitTestRequest) -> HitTestResult;
}

trait ProjectionBackend {
    async fn project(&self, revision: RevisionId, request: ProjectionRequest) -> ProjectionResult;
}
```

领域模块返回 candidate、operation、finding、artifact 或 evidence；只有 kernel transaction service 可以移动
revision pointer。

### 5.7 工作台语义命令

会话 02 定义信封；会话 03/04 实现语义；会话 05 只编排输入：

```ts
interface EditCommandGroup {
  editSessionId: string
  baseDraftRevision: string
  commands: SemanticEditCommand[]
  expectedPostconditions: string[]
}

type SemanticEditCommand =
  | { kind: "schematic.move_symbol"; entityId: string; position: Point }
  | { kind: "schematic.connect"; from: Endpoint; to: Endpoint }
  | { kind: "schematic.set_property"; entityId: string; key: string; value: unknown }
  | { kind: "pcb.move_footprint"; entityId: string; position: Point; rotation: Angle }
  | { kind: "pcb.route_segment"; netId: string; layerId: string; geometry: Polyline }
  | { kind: "pcb.add_via"; netId: string; position: Point; viaClassId: string }
  | { kind: "design.set_edit_control"; entityIds: string[]; mode: "ai-editable" | "ask-before-change" | "locked" }
```

pointer move/drag frame 只产生 transient preview，不写入 JIR。最终 snap、connectivity、规则和坐标由 daemon 返回。

## 6. 跨会话依赖图

```text
                 ┌──────────────┐
                 │ 02 Core/JDP  │
                 └───┬─────┬────┘
                     │     │
          ┌──────────▼┐   ┌▼───────────┐
          │03 Logical │   │04 Physical │
          └─────┬─────┘   └─────┬──────┘
                └──────┬─────────┘
                       │
┌────────────────┐     ▼       ┌────────────────┐
│01 Agent Runtime├──── JDP ────►│05 Workbench UI│
└────────────────┘             └────────────────┘
```

并行策略：

- 会话 02 先发布 `contract-pack-v0` 小提交；完整 kernel/daemon 与其余会话继续并行；
- 会话 01 先用 `MockJdpClient` 开发，最终切换会话 02 生成 binding；
- 会话 03/04 先依照本任务书的 trait/wire shape 实现独立 crate 和 contract test，最终由会话 02 的 registry
  接入 daemon；
- 会话 04 消费会话 03 输出的规范 logical netlist fixture，不直接读取其内部数据库；
- 会话 05 先使用 versioned mock runtime/JDP server，最终只替换 client transport，不改 UI state model；
- 任何跨域不兼容写入 `docs/interface-change-proposals/task-0N-<slug>.md`，留待统一验收会话裁决。

## 7. 各会话统一工作规则

1. 先检查本 worktree 的 `AGENTS.md`、当前 diff 和现有测试，不覆盖用户/其他会话修改；
2. 只修改专属目录；共享文档和共享协议不得在任务分支直接重写；
3. 新增文件保持单一职责；TypeScript 程序文件超过 600 行必须拆分，React 组件目标不超过 300 行；
4. UI 禁止用 emoji 作图标，使用 SVG 图标；
5. 所有外部输入按不可信处理，所有 secret 只使用 credential reference/environment，不写入仓库或日志；
6. 网络/外部工具测试必须能在无凭据环境明确 skip，并说明未验证门禁，禁止伪造通过；
7. 不以 mock、snapshot、进程存在或文件已生成替代真实 contract/domain/UI 验收；
8. 不执行 Production Release，不真实下单，不上传私有设计，除非用户另行明确授权；
9. 提交前运行本任务要求的格式、静态检查、单测、集成测试和 `git diff --check`；
10. 只提交本任务文件；提交信息建议 `feat(jingwei/<domain>): <summary>`。

## 8. 每个会话的交接包

每个会话结束时在自己的 `docs/handoffs/task-0N/` 生成：

```text
README.md                 # 完成/未完成/风险/下一步
changed-files.txt         # 本任务实际修改文件
commands-and-results.md   # 执行过的测试命令、结果与环境
interfaces.md             # 实际提供/消费的接口与版本
known-gaps.md             # mock、skip、未验证外部门禁
artifacts.md              # 可复现制品路径、hash、截图/报告索引
```

交接必须写明 commit SHA、分支、基线 commit、是否存在未提交修改。不能只说“测试通过”。

## 9. 单会话完成标准

每个会话都必须满足：

- 专属任务书中的 P0 必交项完成；
- 正常、失败、权限、取消、恢复/重试或冲突测试与任务风险相匹配；
- 没有越过文件所有权；没有新建第二套正式协议类型；
- 公开 API 有文档/示例，错误是结构化且可诊断；
- mock 和真实能力有明确标记；外部工具未验证时不宣称完成对应门禁；
- 交接包齐全，工作树可审计；
- 不包含 secret、个人数据、未授权库或商业标准全文。

## 10. 统一验收与合并顺序

统一验收会话按以下顺序执行，不使用无审查的批量合并：

1. **合并会话 02**：先冻结 Rust schema、JDP、codegen、daemon 和 extension registry；
2. **适配并合并会话 01**：用生成 binding 替换 mock，验证 durable runtime → JDP；
3. **适配并合并会话 03**：注册 logical module，跑 native ERC 和 schematic projection；
4. **适配并合并会话 04**：消费 logical netlist，注册 PCB/manufacturing module；
5. **最后合并会话 05**：替换 mock transport，做真实 runtime/daemon UI 联调；
6. 运行全仓格式、lint、typecheck、Rust clippy/test、协议 codegen clean check；
7. 运行故障注入和黄金板端到端；
8. 单独执行真实 Tauri 窗口/Computer Use 验收；自动化测试和安装包不能替代该门禁；
9. 如需发布安装包，再执行签名、内容检查和干净机安装；不把构建成功当作已安装 UI 验收。

## 11. 联合端到端验收场景

### E2E-01：需求到制造候选

1. runtime durable admission 一条黄金板需求；
2. Agent 调用 `requirement_compile`，获得结构化 DesignIntent；
3. logical module 完成 MPN 候选、库绑定、schematic 和 native ERC；
4. physical module 从 logical netlist 创建 PCB、应用 JLCPCB v0 profile、生成布局/路由候选和 DRC；
5. 生成 Gerber/drill/BOM/CPL/manifest 候选；
6. UI 可查看阶段、对象、findings、artifact 和 revision；
7. 全程未创建 Production Release。

### E2E-02：人工编辑后 AI 继续

1. UI 打开 draft，移动一个 symbol/footprint，并提交两个语义命令；
2. daemon 创建 draft revision，增量 ERC/DRC 指向稳定对象 ID；
3. 用户锁定对象；
4. Agent 提议局部修改，锁定对象产生 typed conflict 或保持不变；
5. UI 可按 patch hunk 接受/拒绝，undo/redo 与异常恢复不污染 current revision。

### E2E-03：故障恢复

分别在 input admitted、provider 半流、tool recorded、approval pending、daemon commit 后未响应、外部 adapter
运行中和 UI 断线处终止进程。恢复后不得重复设计 revision、丢失审批或把未知状态显示为成功。

### E2E-04：恶意/错误输入

- MCP instructions 尝试扩大权限；
- datasheet/库文本包含提示注入；
- stale base revision；
- 错 pin-pad、错 footprint、未连接 net、DRC 违规、BOM/CPL 不闭合；
- 路径穿越和超大工具输出。

系统必须结构化拒绝/报告，不能通过模型解释、重试或自动 waiver 隐藏。

## 12. 统一验收输出

统一验收会话最终需要交付：

- 合并 commit 与五个原始任务 commit 映射；
- 接口变更裁决记录；
- 全仓测试矩阵和真实未通过项；
- 黄金板 JDL/JIR/revision/tool/event trace；
- 原理图/PCB 编辑证据；
- ERC/DRC/制造候选 manifest；
- 故障注入与安全负向报告；
- 尚未满足的 DFM、样板和 Production Release 门禁。

## 13. 权威文档

发生冲突时按以下顺序：

1. 已接受的 `docs/08-open-questions-and-decisions.md`；
2. `docs/00-product-requirements.md` 与安全/生产门禁；
3. `docs/01-system-architecture.md`、`09`、`10`、`12` 架构基线；
4. 本总任务书的共享协议和文件所有权；
5. 各会话任务书；
6. 模型生成的解释和临时 mock。
