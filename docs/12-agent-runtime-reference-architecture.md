# Agent Runtime 参考架构与成熟方案选型

> 版本：1.0  
> 日期：2026-08-19  
> 状态：Agent 控制面实现基线

## 1. 决策摘要

Jingwei 不再从零开发第二套通用 Agent 框架，也不整体 fork Codex、OpenCode、LangGraph 或其他框架。
首版采用以下组合：

1. **Eval runtime 是直接复用的产品控制面**：复用会话、模型适配、工具循环、权限、审批、任务、队列、
   持久化、取消、恢复、MCP 和审计能力，并抽取为与产品品牌无关的共享 package；
2. **Codex App Server/SDK 是产品协议与交互基线**：参考其线程、续跑、审批和流式事件设计，不把
   coding-specific 工具或文件模型当成 EDA 内核；
3. **OpenCode V2 是源码级可靠性参考**：吸收“输入先持久化、同会话串行、事件回放、单一工具注册/结算
   边界、可区分 steer/queue”等原则，不把其仍在演进的 V2 core 直接作为生产依赖；
4. **Rust `jingwei-daemon` 是 EDA 数据面和最终授权者**：JIR、事务、约束、验证、求解、projection、制造
   和发布门禁始终由 Rust 掌握；Agent runtime 只能提交 JDP 请求；
5. **固定工程流程用代码和状态机，开放推理才用 Agent**：ERC/DRC/DFM/签核/发布是确定性 workflow；
   器件方案比较、问题解释和候选生成才交给模型；
6. **MCP 只用于受控外部集成**：高频、事务性的内部 EDA 能力使用 JDP；MCP server 的描述、注解和输出
   一律是不可信输入，不能提升权限；
7. **OpenTelemetry 是统一观测协议**：贯通 UI、Agent runtime、JDP、daemon、外部 adapter 和模型调用；
8. **LangGraph、Microsoft Agent Framework、Temporal 不进入桌面核心依赖**：保留为 Phase 3 以后云端、
   Azure 或多主机长工作流的可选 adapter。

这套组合保留 Rust-first 的 EDA 核心，同时避免为了语言统一而重写已经成熟的 TypeScript/Bun Agent 控制面。
是否把通用 runtime 迁移到 Rust，必须由实际的安全、性能、部署或维护证据触发，不能作为 Phase 0 前置任务。

## 2. 为什么不能把“Agent 框架”和“EDA 内核”混成一层

两类系统处理的真相不同：

| 控制面 Agent runtime | EDA 数据面 `jingwei-daemon` |
| --- | --- |
| 会话、run、turn、模型尝试 | JIR entity/edge/revision |
| 用户命令、steer、queue | JDP query/transaction/task |
| 工具发现、模型可见性 | 工具的真实权限与领域前置条件 |
| 审批等待与恢复 | workspace lock、原子提交、回滚 |
| 上下文压缩、artifact 引用 | 设计依赖图、增量验证、projection |
| 子任务、模型 fallback | ERC/DRC/DFM、求解器、制造门禁 |
| 流式事件和可观测性 | 可复现证据与发布签名输入 |

Agent runtime 被攻破、模型越权或 MCP 描述恶意时，daemon 仍必须拒绝非法 JDP 请求。反过来，daemon 不负责
提示词、对话历史或模型选择，避免把非确定性模型逻辑引入设计可信计算基。

## 3. 成熟方案评估

### 3.1 选型矩阵

| 方案 | 已验证的有效能力 | 对 Jingwei 的采用方式 | 不采用为核心的原因 |
| --- | --- | --- | --- |
| Eval runtime | 已有工具循环、模型 fallback、根 Agent 所有权、审批持久化、ToolExecution、队列、MCP、恢复 | **直接复用并产品化抽包** | 需要消除 Eval 品牌/领域耦合，补充耐久事件回放与 JDP 生成工具 |
| Codex App Server/SDK | 产品内嵌、认证、线程历史、审批、流式事件、线程续跑/恢复 | **协议、UX、API 参考；可选 coding specialist** | coding-specific；不能拥有 JIR 或替代 EDA 门禁 |
| OpenCode V2 | durable input、steer/queue、同 session 协调器、durable event、单一 tool settlement、权限规则 | **源码与不变量参考** | 当前 V2 源码仍有明确 TODO；整体引入会形成第二套 runtime 与 Effect/Bun 耦合 |
| OpenAI Responses API | 商业模型、原生工具、MCP、长任务/状态能力和正式 API 生命周期 | **首个优先商业 provider**，经统一 provider adapter 接入 | 不绑定单一模型供应商；数据区域、成本和客户策略可能不同 |
| OpenAI Agents SDK | manager/agents-as-tools、handoff、trace 等成熟模式 | **编排模式参考或云服务 adapter** | Eval 已有 runtime，直接再引入会形成双重会话/工具状态 |
| LangGraph/LangSmith | 持久执行、streaming、human-in-the-loop、checkpoint、调试和评估平台 | **可选云端实验/观测 adapter** | Python/JS graph runtime 与桌面现有控制面重复；Jingwei 固定发布流程已有领域状态机 |
| Microsoft Agent Framework | Agent、middleware、session、telemetry、显式 workflow；AutoGen/Semantic Kernel 后继 | **Azure/Foundry 企业 adapter 候选** | Go 仍处 public preview，其他实现引入 C#/Python 栈；不适合首版本地核心 |
| Temporal | 跨故障、跨主机、可运行数天/数年的 durable workflow | **Phase 3 云端长工作流候选** | 桌面单机 MVP 不需要额外服务集群；不能替代 Agent 的上下文和工具安全模型 |
| MCP | 外部资源、prompt、tool 的标准协议和能力协商 | **外部集成标准** | 协议本身不执行 Jingwei 权限；不适合作为内部高频 CAD 事务总线 |
| OpenTelemetry | 跨进程 trace、span、metric、log 与 OTLP 生态 | **统一可观测标准** | 只观测，不拥有业务终态或审计真相 |

### 3.2 采用成熟方案的规则

- 优先复用公开协议、官方 SDK、provider client、OAuth/MCP、telemetry 和 durable workflow 基础设施；
- 自研只覆盖 Jingwei 的差异化领域：JDL/JIR/JDP、设计事务、器件/库语义、ERC/DRC/DFM、求解和制造发布；
- 不因某框架“支持 Agent”就同时引入它的 session store、tool registry 和 workflow engine；
- 同一职责只允许一个权威实现。迁移必须先定义旧/新 owner、双写禁令、回放策略和删除旧路径的验收；
- 引入或复制开源代码前记录精确 commit、许可证、NOTICE、修改清单和安全维护 owner；
- Codex 当前仓库为 Apache-2.0，工作区内 OpenCode 为 MIT；任何源码复制仍需单独法务和第三方依赖审查。

## 4. 目标进程架构

```text
┌───────────────────────────────────────────────────────────────┐
│ Tauri 2 / React Workbench                                     │
│ chat | plan | tool timeline | approval | schematic/PCB editor │
└───────────────┬──────────────────────────────┬────────────────┘
                │ runtime API + replay cursor  │ JDP view/query
┌───────────────▼─────────────────┐  ┌────────▼─────────────────┐
│ jingwei-agent-runtime           │  │ jingwei-daemon (Rust)    │
│ Eval-derived TypeScript/Bun     │  │ sole JIR/JDP writer      │
│ session/run/model/tool/approval │  │ auth/transaction/rules   │
│ queue/event/MCP/trace/artifact  │  │ solver/adapter/release   │
└───────────────┬─────────────────┘  └────────┬─────────────────┘
                │ generated JDP tools          │ atomic revisions
                └──────────────────────────────►│
                         authenticated IPC      ▼
                                      JIR + artifact + evidence
```

建议安装包包含三个可独立故障和恢复的进程：

- `jingwei-app`：Tauri 壳和低权限 WebView；
- `jingwei-agent-runtime`：由现有 Eval server 抽取/重命名的 Agent 控制面 sidecar；
- `jingwei-daemon`：Rust EDA daemon，工作区唯一写入者。

首版由 Tauri 以 `desktop-owned` 模式管理两个 sidecar 的启动、健康和退出清理，但不得把“前端窗口仍存在”
当成任一任务的真实状态。未来 `attached`/`headless` 模式才使用外部 owner lease 让任务独立于 UI 存活；三种
模式不得混用。权威生命周期见 `13-authoritative-boundaries-and-lifecycle.md`。

## 5. 核心运行时对象

```text
Session
  ├─ Input (steer | queue)
  ├─ Run (root | specialist | workflow-node)
  │    ├─ Turn
  │    │    ├─ ModelAttempt
  │    │    ├─ ToolCall
  │    │    └─ ToolExecution
  │    └─ Checkpoint / ArtifactRef / EvidenceRef
  ├─ ApprovalRequest
  └─ DurableEvent(seq)
```

最低数据要求：

- `Session`：用户会话的持久 owner、项目、工作区和数据策略；
- `Input`：不可变输入、client request ID、delivery 语义、admitted sequence；
- `Run`：根/专业任务关系、预算、状态、取消原因和最终输出；
- `ModelAttempt`：provider/model/capability、请求摘要、用量、延迟、错误域和完成状态；
- `ToolCall`：模型提出的原始调用和规范化调用；
- `ToolExecution`：授权快照、effect/resource claims、JDP execution ID、结果、证据和恢复状态；
- `ApprovalRequest`：脱敏输入、目标、理由、范围、过期时间、决定者和决定；
- `DurableEvent`：聚合 ID、严格递增 sequence、schema version、时间和 payload reference。

任何 UI timeline 都是这些持久对象的投影，不是另一套状态机。

## 6. 必须成立的耐久性不变量

1. **输入先持久化，后唤醒执行**：模型请求前先写入 `Input` 和 durable event；相同 request ID 要么返回同一
   admitted input，要么返回 typed conflict；
2. **同一 session 只有一个活动 continuation**：重复 resume 加入同一个执行，新增 wake 合并为一次后续 drain；
   不同 session 可以并行；
3. **`steer` 与 `queue` 分离**：steer 在当前安全边界后的下一次 provider turn 生效；queue 等当前根 run 终结
   后再启动，不能把新需求偷偷混进正在执行的 JDP transaction；
4. **一个 provider turn 只有一个流事务**：中途 transport 失败产生的草稿 text/reasoning/tool call 不进入下一
   attempt；只有明确 finish 或结构化失败才能提交 turn；
5. **工具调用先记录，副作用后发生**：运行时先创建 durable ToolExecution，再授权和调用 daemon；
6. **审批先持久化，后向 UI 可见/等待**：审批决定也必须先提交，再唤醒 coroutine；持久化失败时 fail closed；
7. **设计提交以 daemon 为幂等边界**：`idempotencyKey + baseRevision + normalizedInput` 只能提交一次；runtime
   重试只能查询已有 execution，不能盲目重复副作用；
8. **事件可从 cursor 回放**：断线 UI 先读取 `after=seq` 的历史，再订阅 live event；历史和 live 不丢不重；
9. **未知副作用不自动重试**：进程在外部工具或 daemon 响应前崩溃时，先通过 execution ID 查询；无法证明
   终态就进入 `needs-reconciliation` 或 `safety-quarantine`；
10. **持久事实不依赖模型记忆**：需求基线、revision、审批、finding、签核和 release 始终通过对象引用查询。

## 7. 单一工具注册与结算边界

所有内置工具、JDP 生成工具和 MCP 工具进入同一个规范表示：

```ts
interface RuntimeToolDescriptor {
  name: string
  description: string
  inputSchema: JsonSchema
  outputSchema?: JsonSchema
  source: "builtin" | "jdp" | "mcp" | "plugin"
  permission: string
  effects: Array<"read" | "design-write" | "workspace-write" | "process" | "network" | "hardware" | "release">
  resourceClaims: string[]
  idempotency: "required" | "supported" | "unknown"
  cancellation: "supported" | "cooperative" | "unsupported"
  timeoutMs: number
  outputBudget: { inlineBytes: number; overflow: "artifact" | "reject" }
}
```

工具执行只能经过一个 settlement pipeline：

```text
model tool call
  → normalize + schema validate
  → confirm advertised identity is not stale
  → runtime policy + user/project authorization
  → optional durable approval
  → persist ToolExecution(running)
  → invoke handler/JDP/MCP
  → validate and bound output
  → store large result as artifact
  → persist typed terminal outcome
  → publish durable event
```

关键规则：

- “模型能看到工具”只是 discoverability，不是执行授权；leaf handler/daemon 必须再次鉴权；
- JDP tool schema 从 Rust 类型生成，runtime 不手写第二份领域 schema；
- 工具输出先做 schema、大小、敏感信息和 artifact 化处理，再进入模型上下文；
- MCP `annotations` 只是 hint；不得依据第三方声明的 read-only/idempotent 自动放宽权限；
- 不建立第二个 plugin executor、legacy tool path 或直接调用 Rust store 的捷径；
- 普通无工具回复可以正常结束；未通过发布门禁只限制发布工具，不把对话强行判失败。

## 8. 权限与审批模型

### 8.1 双层授权

1. Agent runtime 判断当前用户、session、Agent role、tool、effects 和资源是否允许发起；
2. `jingwei-daemon` 独立判断 workspace、base revision、JDP capability、领域阶段和签核是否允许执行。

runtime 的 allow 不能覆盖 daemon 的 deny；项目持久 deny 也不能被一次性审批改写。

### 8.2 默认级别

| 等级 | 例子 | 默认行为 |
| --- | --- | --- |
| R0 本地只读 | 查询局部 JIR、查看 diff/finding | 项目内自动允许，仍审计 |
| R1 候选写入 | 创建设计候选、移动器件、生成库草稿 | 项目策略自动或一次批准；永不直接发布 |
| R2 外部/高副作用 | 网络外发、外部进程、硬件访问、组织库提升 | 显示目标和输入，持久审批 |
| R3 制造候选 | 导出制造包、提交工厂 DFM | 工程权限 + 完整前置门禁 |
| R4 生产业务状态 | waiver、Production Release、revoke | 具名角色/多角色签核；模型不得代签 |

审批 UI 必须显示工具、目标、脱敏输入、effect、数据接收方、原因、有效范围和过期时间。`allow-all` 只允许在
明确定义的同 session/同 permission/同资源边界内展开，不能批准全部未来工具。

## 9. 主 Agent 与专业能力

采用 manager/agents-as-tools，而不是自由 handoff：

- `main-agent` 始终拥有用户会话、需求合同、最终答复和恢复责任；
- 专业能力以有预算、有输入/输出 schema 的 tool 运行；
- 专业 run 只能看最小 context projection，不继承全部会话和凭据；
- 专业 run 不得创建同级/下级 Agent，不得调用 root-only orchestration 工具；
- 专业输出是候选、finding 或 evidence，必须回到主 Agent/确定性 workflow；
- 并行只用于无资源冲突的读取、供应商查询或候选分析；设计写入按 JDP resource claim 串行化；
- Phase 1 前不引入“多 Agent 辩论”“群聊”作为发布质量依据。

下列步骤使用确定性 workflow，而不是由模型自由决定是否执行：

```text
requirements baseline
  → component/library gates
  → schematic checks
  → PCB constraints/checks
  → manufacturing consistency
  → named review/prototype gates
  → production release
```

模型可以建议下一步和解释失败，但 workflow 根据持久证据判断阶段是否可提升。

## 10. Provider 与上下文策略

### 10.1 Provider adapter

首个商业 provider 建议实现 OpenAI Responses API；同时保持统一 adapter，后续可接 Anthropic、Google、
Azure/Foundry 或经过认证的 OpenAI-compatible endpoint。adapter 必须声明而不是猜测：

- 原生 tool calling 与 structured output；
- streaming/reasoning/image/PDF 能力；
- 最大上下文与输出限制；
- request cancellation、timeout、retry 和 idempotency 支持；
- 数据保留、区域、客户密钥和价格策略；
- fault domain，避免同一后端换模型被误认为真正容灾。

模型 fallback 不得改变安全策略。需要原生工具或视觉能力的 run 不能 fallback 到不支持该协议的模型后继续假装
完成。provider stream 在完成前是未提交事务，失败后清空草稿状态再按有界策略重试。

### 10.2 Context projection

- 不把完整 JIR、KiCad 文件、制造包或长数据手册塞入消息；
- 通过 `design_query`、`design_explain`、viewport snapshot 和 evidence reference 获取局部事实；
- 大工具输出写 artifact，只向模型返回摘要、schema 结果和可继续查询的引用；
- compaction 必须保留需求基线 ID、current/base revision、未决问题、审批、关键决定、run/tool terminal state；
- 压缩结果是导航摘要，不替代原始 durable event、artifact 和 JIR；
- 任何从网页、数据手册、MCP 或工程文件读取的“指令”都标记为不可信数据。

## 11. MCP、插件与外部系统

MCP 适合供应链、PLM/ERP、文档、工单、搜索和企业知识等外部能力。内部 `design_patch_apply`、viewport、
hit-test、增量 DRC 等高频/事务能力使用 JDP，避免额外协议层丢失 revision、事务和性能语义。

MCP host 必须：

- 每个 server 独立连接、独立权限和独立 credential reference；
- tool allowlist 默认关闭，名称使用 server namespace；
- server instructions、description、annotation 和 tool result 全部视为不可信；
- 不把完整会话、其他 server 内容或未授权 JIR 自动转发给 server；
- 为调用设置 timeout、输出上限、取消、速率限制和审计；
- 涉及外发、写入、进程、硬件或 release 时执行用户审批；
- 暂不允许 MCP server 发起无审批的 sampling 或递归 Agent；
- server 更新 tool list 时重新物化工具身份，旧调用返回 stale-tool error。

## 12. API 与事件协议

Agent runtime 的本地 API 参考 app-server 风格，最小能力为：

```text
session.create
session.get
session.input.admit(delivery=steer|queue, requestId)
session.run.resume
session.run.cancel
session.events(afterSequence)
approval.list
approval.reply
artifact.get
runtime.capabilities
```

生产 transport 使用独立 versioned named pipe/UDS endpoint，schema 由 `agent-protocol` 维护；stdin 只用于
bootstrap。Windows Slice 1 已实现独立认证 named pipe RPC，stdout 只保留版本化 endpoint handshake；stdio
不再承载工作台 Runtime 方法。UDS 与其他平台安装门禁仍需分别实现和验收。

事件至少包括：

- `input.admitted`、`run.started/blocked/completed/failed/cancelled`；
- `model.attempt.started/completed/failed`；
- `tool.call.recorded`、`tool.execution.started/completed/failed/denied`；
- `approval.requested/resolved/expired`；
- `artifact.created`、`jdp.revision.committed`、`finding.changed`；
- `recovery.started/completed/quarantined`。

每个事件包含 `schemaVersion`、`aggregateId`、`sequence`、`timestamp`、`sessionId`、`runId`、`traceId` 和
payload/artifact reference。token delta 可以是短期流事件；工具终态、审批、revision 和 run 终态必须持久化。

## 13. 故障恢复矩阵

| 故障点 | 恢复策略 |
| --- | --- |
| 输入已提交、runtime 未启动 run | owner lease 恢复后 drain pending input |
| provider 未产生 finish | 丢弃草稿 attempt；按 fault-domain 有界重试或显式失败 |
| tool call 已记录、未执行 | 重新授权后执行；必须保持同一 toolCall/idempotency key |
| 审批已请求、UI/runtime 重启 | 从 durable approval 恢复等待；过期则 fail closed |
| daemon 已提交、runtime 未收到响应 | 用 JDP execution/idempotency key 查询并关联已有 revision |
| 外部进程状态未知 | 不自动重跑；检查进程/临时目录/输出哈希，无法证明则 quarantine |
| UI 断线 | 以最后 sequence 回放，再接 live stream |
| current revision 与 run base 不同 | 返回 typed stale-base conflict，走 rebase/新 proposal |
| runtime DB/JIR schema 升级失败 | 停止写入，恢复备份/只读诊断，不启动模型执行 |

## 14. 可观测性与评估

使用 OpenTelemetry 贯通以下 span：

```text
user.request
  └─ agent.run
      ├─ model.attempt
      ├─ specialist.run
      └─ tool.execution
          ├─ approval.wait
          ├─ jdp.request
          │   └─ kernel.transaction / validation / solver
          └─ external.adapter
```

span 属性只保存 ID、版本、状态、延迟、token/cost、effect、错误码和哈希；不得保存秘密、完整私有设计或原始
模型上下文。不可变审计与 OTel 观测分离：telemetry 丢失不影响业务真相，审计记录也不等于性能 trace。

至少建立四类 eval：

- runtime：顺序、恢复、审批、取消、幂等、事件回放；
- tool policy：可见但未授权、MCP 提权、资源冲突、敏感数据；
- Agent 行为：追问质量、工具选择、失败恢复、不过度重试；
- EDA 结果：黄金板/坏板的确定性 domain gate，不能由语言评分替代。

## 15. 代码复用实施边界

### 15.1 直接从 Eval 抽取

建议形成与产品无关的 packages：

```text
packages/
  agent-runtime/        # loop、session/run、provider、context、recovery
  agent-tools/          # canonical registry、settlement、output bounding
  agent-policy/         # permission、approval、effects、resource claims
  agent-events/         # durable events、replay、projection
  agent-mcp/            # MCP host/client、credential refs、tool adapter
  agent-observability/  # OTel、audit bridge、redaction
  agent-protocol/       # runtime API schema 和生成 client
```

Jingwei 只新增：

- `jingwei-agent-config`：system policy、专业任务、模型/provider 配置；
- `jingwei-jdp-tools`：消费 Rust 生成 schema/binding 的 adapter；
- `jingwei-agent-ui`：对话、run、tool、审批与 EDA 对象 cross-probe；
- 产品级黄金 eval 和故障注入。

抽包时先用 contract tests 固定现有行为，再移动代码；禁止复制一份后分别演进。

### 15.2 仅参考、不复制

工作区内 OpenCode V2 的以下设计值得持续对照：

- `opencode-dev/packages/core/src/session.ts` 与 `session/input.ts`：durable prompt admission、steer/queue；
- `session/run-coordinator.ts`：同 key 串行、join、coalesced wake、interrupt；
- `event.ts`：durable sequence、read-after、replay 和 live wake；
- `tool/tool.ts` 与 `tool/registry.ts`：单一 tool representation、materialization、settlement/output bounding；
- `permission.ts`：规则合并、ask/allow/deny 和持久 allow 语义。

这些文件用于测试不变量和架构评审，不建立运行时源码同步分支。OpenCode V2 中仍标注未完成的 durable
continuation、post-run maintenance 等能力，不能被文档描述误认为当前已完整可用。

### 15.3 Codex 集成边界

- 参考 App Server 的 thread、approval、event stream 和恢复协议；
- 可将 Codex 作为“编写 adapter/规则/测试”的开发 Agent，通过受控工具工作；
- 不允许 Codex shell/apply-patch 直接编辑 current JIR、签核记录或 Production Release；
- 如果未来嵌入 Codex App Server，它是可替换 specialist，不成为 Jingwei session/JIR 的双重 owner。

## 16. Phase 0 实施顺序

1. 为 Eval runtime 建立行为清单和 contract test，冻结 session/tool/approval/recovery 语义；
2. 抽取无产品品牌的共享 packages，保留一个数据库和一个 tool settlement 入口；
3. 定义 runtime API、durable event schema、sequence/replay 和进程 owner lease；
4. 实现 `steer`/`queue`、同 session join/coalesced wake、不同 session 并行；
5. 从 Rust JDP schema 生成 runtime tools，接通 read → candidate write → validation 的最小闭环；
6. 接通一个真实商业 provider，验证 native tool calling、stream failure、fallback 和 cancellation；
7. 完成持久审批、双层授权、MCP untrusted policy 和 output bounding；
8. 接通 OpenTelemetry 和本地审计，完成敏感信息测试；
9. 在每个持久化边界做 kill/restart 故障注入；
10. Tauri UI 使用 replay cursor 展示 run/tool/approval/revision 的真实状态。

### 16.1 Phase 0 退出门禁

- 相同 client request ID 不产生两条用户输入；
- 同一 session 无并行 provider turn，不同 session 可并行；
- steer 和 queue 在自动化测试中表现不同且可恢复；
- provider 中途断流不会把 partial tool call/text 混入下一 attempt；
- 工具副作用前已有 durable ToolExecution；daemon 已提交但 runtime 崩溃不会生成第二个 revision；
- 审批在 app/runtime 重启后仍可查看、拒绝和恢复；
- 工具即使被模型看到，缺少 runtime 或 daemon 授权仍被拒绝；
- 恶意 MCP instructions/annotations 不能扩大权限或取得完整会话/JIR；
- UI 从任意已保存 sequence 断线重连后得到一致投影；
- kill provider/runtime/daemon/adapter 后没有未知 current revision；无法证明时进入 quarantine；
- OTel trace 可跨 runtime → JDP → daemon 关联，日志和 trace 不含密钥或完整私有设计；
- Agent 普通对话可以无工具正常结束，Production Release 仍只能由领域门禁和具名签核创建。

## 17. 明确不做

- 不为了“全 Rust”在 Phase 0 重写 provider streaming、MCP/OAuth、session store 和 approval UI；
- 不同时运行 Eval session store 与 Codex/OpenCode/LangGraph 的第二套 session owner；
- 不把每个 EDA 阶段都包装成会自由递归的 Agent；
- 不把 MCP tool annotation 当权限事实；
- 不允许模型生成任意 shell 命令修改 JIR、外部 EDA projection 或 release 目录；
- 不依靠自然语言总结恢复业务状态；
- 不把 LangSmith、Temporal Cloud 或 Azure 服务变成离线桌面 MVP 的强依赖；
- 不用“模型自评成功”代替工具终态、EDA 检查和工程签核。

## 18. 主要参考与当前源码证据

### 18.1 官方资料

- [OpenAI Codex App Server](https://developers.openai.com/codex/app-server)
- [OpenAI Codex SDK](https://developers.openai.com/codex/codex-sdk)
- [OpenAI Codex approvals and security](https://developers.openai.com/codex/agent-approvals-security)
- [OpenAI Agents SDK orchestration](https://developers.openai.com/api/docs/guides/agents/orchestration)
- [OpenAI Responses API reference](https://developers.openai.com/api/reference/resources/responses)
- [Codex repository license](https://github.com/openai/codex/blob/main/docs/license.md)
- [LangGraph overview](https://docs.langchain.com/oss/python/langgraph/overview)
- [Microsoft Agent Framework overview](https://learn.microsoft.com/en-us/agent-framework/overview/)
- [Temporal documentation](https://docs.temporal.io/)
- [MCP 2025-11-25 specification](https://modelcontextprotocol.io/specification/2025-11-25)
- [MCP tool security considerations](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
- [OpenTelemetry specification overview](https://opentelemetry.io/docs/specs/otel/overview/)

### 18.2 工作区源码证据

- `../../embedded-agent/packages/core/src/runtime/agent-loop.ts`
- `../../embedded-agent/packages/core/src/runtime/agent-loop-tool-execution.ts`
- `../../embedded-agent/packages/core/src/runtime/agent-authority.ts`
- `../../embedded-agent/packages/core/src/runtime/tool-approval.ts`
- `../../embedded-agent/packages/core/src/runtime/session-command-queue.ts`
- `../../embedded-agent/packages/core/src/store/tool-execution.ts`
- `../../embedded-agent/packages/core/src/mcp/manager.ts`
- `../../opencode-dev/packages/core/src/session.ts`
- `../../opencode-dev/packages/core/src/session/input.ts`
- `../../opencode-dev/packages/core/src/session/run-coordinator.ts`
- `../../opencode-dev/packages/core/src/event.ts`
- `../../opencode-dev/packages/core/src/permission.ts`
- `../../opencode-dev/packages/core/src/tool/tool.ts`
- `../../opencode-dev/packages/core/src/tool/registry.ts`

工作区源码只证明当前 checkout 的实现，不自动代表上游最新版本。进入代码复用阶段时必须记录 commit、diff、
许可证和 contract test 结果。
