# 精卫 Agent 权威边界与生命周期规范

> 版本：1.0
> 日期：2026-08-19
> 状态：Accepted，作为后续统一收拢的最高优先级架构规范

## 1. 目的与权威顺序

本文解决五会话合并后仍存在的跨层歧义：谁拥有状态、哪些 ID 必须关联、一次修改如何跨 Runtime 与 daemon
结算、sidecar 何时应退出、验证成熟度是否属于 revision，以及谁有资格创建 Production Release。

出现冲突时使用以下顺序：

1. 已签核的产品范围、风险 profile 和 Production Release 政策；
2. 本文的状态所有权、身份、事务、生命周期和版本兼容规则；
3. Rust 生成的 JIR/JDP schema、Runtime protocol schema 和持久化 migration；
4. `00` 至 `12` 号专题设计文档；
5. handoff、测试记录和实现说明；
6. UI 文案、fixture 和模型生成的解释。

Handoff 是历史证据，不再作为新的架构来源。后续设计与实现统一在当前集成主线收拢。

## 2. 不可破坏的不变量

| 编号 | 不变量 |
| --- | --- |
| BND-001 | `jingwei-daemon` 是 JIR graph、immutable revision、draft head、change proposal 和 project head pointer 的唯一写入者。 |
| BND-002 | Runtime 只拥有 session/run/input/model-attempt/tool-execution/approval/runtime-event；其中保存的 revision 仅是带版本的观察值，不是设计指针。 |
| BND-003 | React/WebView 只拥有 viewport、selection、panel、transient ghost 等视图状态；不得保存权威设计、门禁或审批终态。 |
| BND-004 | Design Revision、Maturity Assessment 和 Production Release 是三个不同对象，不能用一个 `status` 字段替代。 |
| BND-005 | 所有设计写操作必须同时绑定 workspace、project、actor、base revision、request ID、idempotency key 和可信调用来源；Agent 使用 ToolExecutionId，UI/CLI 使用 daemon 签发的 CommandInvocationId。缺少任一权威关联时 fail closed。 |
| BND-006 | Runtime allow 不能覆盖 daemon deny；人类 approval 不能覆盖不支持能力、锁定对象、风险 profile 或强制发布门禁。 |
| BND-007 | 外部 EDA、router、供应商和模型输出都是候选输入；只有被规范化、验证并提交到新 revision 的内容才成为设计事实。 |
| BND-008 | 任何制造输出都绑定一个 immutable revision、toolchain lock、profile lock 和 validation set；混用即阻断。 |
| BND-009 | Production Release 只能由组织身份、角色政策和独立签名边界创建；Agent、普通 Runtime approval 和本地 actor 字符串都不能签名发布。 |
| BND-010 | 协议或持久化版本不兼容时显式拒绝或只读打开，不静默降级、不猜测字段、不在两个进程中维护镜像语义。 |

## 3. 状态所有权矩阵

| 状态 | 权威所有者 | 可缓存/投影位置 | 禁止行为 |
| --- | --- | --- | --- |
| Workspace identity、store schema、writer lease | daemon/store | Tauri、Runtime capability view | Runtime 或 UI 创建第二个 writer |
| Project metadata、project head、change proposal | daemon/store | Runtime session、UI header | 以缓存 revision 覆盖 daemon project head |
| JIR graph、revision、operation journal、draft | daemon/kernel/store | display list、query result、artifact | React、Tauri command 或 Runtime SQLite 直接写 JIR |
| Validation run、finding、waiver validity、gate assessment | daemon validation/release domain | Runtime/UI projection | UI 根据计数自行宣布通过 |
| Session、input、run、model attempt | Runtime SQLite | UI event projection | daemon 保存 provider 会话或模型输出终态 |
| ToolExecution、Runtime approval、MCP connection | Runtime SQLite/policy | UI projection、daemon correlation ref | 模型可见工具自动获得执行权 |
| Viewport、selection、hover、transient preview | UI memory | 可选本地视图偏好 | 将中间帧写成 design operation |
| Artifact bytes、evidence bytes | daemon CAS 或受控 evidence store | Runtime 只保存 opaque reference | 在普通事件或模型上下文内嵌大文件/秘密 |
| OS process、window、installer/update | Tauri/supervisor | UI health view | 项目文件指定 sidecar 路径或参数 |
| Human identity、organization role | OS identity/organization IAM | Runtime/daemon 短期 claims | 信任请求体中的 `createdBy`/`actor` 字符串 |
| Production Release、签名、撤销 | organization release ledger/signer | daemon/UI 只读验证结果 | Agent 或 workspace 内普通密钥签名发布 |

Runtime 可以保存 `observedRevision`、`observedGateAssessmentId` 和 `releaseRef` 以支持回放，但必须携带 daemon
instance/epoch 与 sequence。重连后先对账再展示；观察值不能反向移动权威指针。

## 4. 身份与关联模型

### 4.1 Opaque ID

所有 wire ID 都是有类型、不可解释的 opaque string。实现可以生成带前缀 UUID，但客户端不得解析前缀、长度或
UUID 结构。fixture 可以使用稳定可读 ID；这不构成生产格式承诺。

最低关联关系：

```text
WorkspaceId
  └─ ProjectId
      ├─ RevisionId / DraftSessionId / ArtifactId / EvidenceId
      └─ Runtime SessionId
          └─ RunId
              └─ ToolExecutionId
                  └─ JDP RequestId + IdempotencyKey + ExecutionId
```

- Runtime `SessionView` 必须同时保存 `workspaceId` 与 `projectId`；
- 每个 JDP connection 绑定 workspace、daemon instance、client identity 和 capability scope；
- 每个 JDP request 的 workspace/project 必须与 connection scope 一致；
- `ToolExecutionId` 或 `CommandInvocationId` 在 daemon journal 中作为可信 correlation reference，不能由模型或
  WebView 请求体自由填入；
- `ActorRef` 来自认证上下文，至少包含 subject、issuer、authentication method 和 role claims；
- 本地开发可使用 OS principal 派生 actor，但该 actor 没有 Production Release 签名能力。

### 4.2 三层授权

1. **进程连接授权**：OS ACL、bootstrap token、protocol/target/build handshake；
2. **行为授权**：actor/session/agent role、permission、effect、resource claim、approval；
3. **领域授权**：base revision、object edit policy、support/risk profile、validation gate、release role。

三层取交集。任何上层 allow 都不能扩大下层能力。

## 5. Revision、成熟度和发布

### 5.1 Design Revision

Revision 是 immutable JIR graph 与规范 operation history 的内容身份。规范模型固定拆为：

```text
RevisionSnapshot     = RevisionId + projectId + parents + schemaVersion + graphHash + journalHash
RevisionCommitRecord = CommitId + revisionId + authenticated actor + issuedAt + correlation + signature?
DraftSession         = sessionId + baseProjectHead + draftHead + lease + state
ChangeProposal       = proposalId + baseProjectHead + proposedRevision + creation record
ProposalDecision     = decisionId + proposalId + accept/reject/withdraw + authenticated reviewer
ProjectHead          = daemon 以 compare-and-swap 移动的已接受设计指针
```

`RevisionId` 必须由 `RevisionSnapshot` 全部规范字段重算；`createdAt`、actor、review、artifact/evidence 引用不进入
同一个 snapshot metadata，而由 commit、assessment、manifest 等 append-only record 表达。这样相同内容可以有
多个提交证明，但同一 RevisionId 永远只有一份 canonical snapshot。

draft/candidate/verified/released/superseded 都不是 revision snapshot 的可变状态。draft 是 `DraftSession` 的 head；
待审设计是 immutable `ChangeProposal`；accept/reject/withdraw 由 append-only `ProposalDecisionRecord` 表达；accept
decision 与 CAS 移动 `ProjectHead` 必须同一 daemon transaction。Manufacturing Candidate 是成熟度投影，不是另一
个含义不明的 project pointer。

`verified`、`released`、`superseded` 不属于设计内容真相：

- 验证可能因规则、工具、profile、证据过期而失效；
- release 可能撤销但 revision bytes 不变；
- superseded 是 pointer/release 关系，不是 snapshot 内容。

Slice 0 已移除 `RevisionStatus`：canonical snapshot 不再包含 `createdAt`、actor、成熟度或 artifact/evidence 引用；
commit、draft、proposal/decision、assessment 和 release 使用独立记录。`verify()` 会重算完整 RevisionId、graph
hash，并由 store 验证 journal linkage。旧 store 只能只读打开，必须通过显式命令在完整数据备份后迁移；新版
store 会只读打开并拒绝降级。迁移时存在活动 writer 或未恢复 transaction 会 fail closed。

### 5.2 Maturity Assessment

成熟度是可重算的签名输入集合：

```text
AssessmentKey = hash(
  revisionId,
  intentRevision,
  toolchainLock,
  ruleSetLock,
  manufacturerProfileLock,
  evidenceSet,
  waiverSet
)
```

Assessment 只输出 L0-L4 中的最高工程成熟度、通过/阻断 gate、finding 和失效原因。L5 不是 assessment 自身的
结果；只有 L4 assessment 与有效、未撤销的组织 `ReleaseRecord` 组合后，release projection 才显示 L5。UI 只能
显示 daemon/release domain 返回的投影，不能从 `DRC errors == 0` 自行推导成熟度。

### 5.3 Production Release

Production Release 是 append-only record，不是修改 revision：

```text
ReleaseRecord
  releaseId
  projectId
  revisionId
  assessmentId
  artifactManifestHash
  toolchain/profile/rule locks
  approval attestations[]
  organization signature
  issuedAt
```

签名服务重新计算 manifest/assessment hash 并验证角色、分离职责和证据有效期。daemon 可生成 release request、
验证签名和记录引用，但不能持有组织发布私钥。撤销是独立签名的 append-only `RevokeRecord`，不得修改原
`ReleaseRecord`；`active | revoked | stale | invalid` 是 verifier 根据 release、revoke、assessment 和 artifact bytes
计算的 `EffectiveReleaseStatus`。

## 6. 阶段投影与失效传播

领域阶段不是 Runtime 可自由写入的状态机，而是 daemon 根据目标 revision 与 gate records 计算的 projection。
Runtime workflow 可以请求执行下一节点，但不能直接把项目设为 `pcb-verified`。

每个 validation/artifact/approval 声明依赖集合。新 revision 或 lock 变化后，daemon 通过 dependency graph 生成
`valid`、`stale`、`invalid` 或 `blocked`：

| 变化 | 最低失效范围 |
| --- | --- |
| 硬需求/风险 profile | intent、架构、器件、库、logical、physical、manufacturing、release |
| MPN/pin/footprint | library、ERC、PCB connectivity、DRC、BOM/CPL、release |
| 仅 value/丝印且无电气影响 | 对应 logical/physical display、BOM 或 manufacturing outputs |
| placement/route/copper | PCB DRC、KiCad equivalence、manufacturing、prototype/release |
| manufacturer profile/stackup | PCB constraints、DRC/DFM、manufacturing、release |
| tool/rule version | 由声明的 provider coverage 选择性失效；不能沿用旧通过状态 |
| waiver scope/fingerprint | 对应 finding 重新打开并使 gate 失效 |

失效本身必须产生 durable daemon event，Runtime/UI 通过 sequence 回放。

## 7. 跨进程事务与对账

一次有副作用的 Agent 工具调用使用同一 correlation chain：

```text
durable input
  → Run / ToolExecution(recorded)
  → policy + optional approval
  → JDP request(baseRevision, requestId, idempotencyKey, toolExecutionId | commandInvocationId)
  → daemon prepare / commit / journal / pointer
  → JDP terminal response or uncertain transport failure
  → Runtime reconcile(executionId, idempotencyKey)
  → ToolExecution terminal state
  → UI event projection
```

人工 UI/CLI 写入不伪造 Runtime ToolExecution：客户端先在认证 connection 上向 daemon 申请绑定 actor、project、
base revision 和 effect scope 的短期 `CommandInvocationId`，再携带它提交同样的 JDP transaction。daemon journal、
幂等和 reconciliation 规则相同；只有 Runtime 的 ToolExecution 结算步骤被客户端自己的 command terminal record
替代。

对账接口必须返回 `not-found | prepared | committed | failed | quarantined`，以及原 terminal response/response hash。
Runtime 只在 `not-found` 且工具策略允许时重试；`committed` 直接结算原结果；未知状态进入 quarantine，禁止再次
提交。当前 Runtime adapter 已有对账语义，但真实 sidecar 报告 `reconciliation: false`，这是 Phase 0 产品退出的
P0 阻塞项。

## 8. Sidecar 生命周期与传输

### 8.1 三种 owner mode

| 模式 | owner | Tauri 退出行为 | 用途 |
| --- | --- | --- | --- |
| `desktop-owned` | 当前 Tauri 进程 | 关闭其启动的 Runtime/daemon 子进程 | 当前 Windows Phase 0、单窗口本地工作台 |
| `attached` | 外部 supervisor/已有 owner lease | 仅断开，不杀进程 | 后续 CLI、多个 UI 或长任务 |
| `headless` | CLI/CI/service supervisor | 与 UI 无关，按任务/idle policy 退出 | 自动化和 CI |

当前实现只承诺 `desktop-owned`。文档中“UI 退出后 sidecar 必须继续运行”不适用于该模式。支持持久任务前必须
实现 owner lease、client refcount、idle timeout 和 attach authorization，不能仅删除退出清理。

### 8.2 传输边界

- stdin 只用于一次性 bootstrap secret/config；命令行和环境变量不携带 bootstrap secret；
- daemon 正式数据面使用 Windows named pipe / macOS-Linux UDS 与长度前缀 JDP frame；
- Runtime 正式控制面必须使用独立的本地 endpoint 和由 `agent-protocol` 生成的 RPC schema；
- 当前 Runtime JSON-lines stdin/stdout RPC 只属于受控 Phase 0 harness，不能直接作为多 client 生产 transport；
- Tauri 只负责最小 lifecycle/credential/endpoint discovery，React 通过窄 typed bridge 连接 Runtime/daemon client；
- stdout 可输出单个有上限的 startup handshake，后续日志进入受控日志管道，不能与 RPC response 混流。

## 9. 协议、schema 与升级

以下版本独立演进，release manifest 必须全部锁定：

| 版本 | 兼容规则 |
| --- | --- |
| Runtime API/event schema | additive minor；未知 event 可保存但不拥有业务终态；breaking change 升 major |
| JDP envelope/operation catalog | handshake 选择共同版本；未知 operation fail closed |
| JIR graph/entity schema | 每个 kind 独立 schema version；migration 只由 Rust registry 执行 |
| Workspace store schema | newer store 只读；升级前备份并验证 migration hash |
| Artifact/Release manifest | verifier 必须识别版本；未知强制字段阻断发布 |
| Toolchain/profile/rules | 不做协议兼容推断；变化产生新 lock 并使相关 assessment stale |

生成 TypeScript binding 的 reproducibility 是合并门禁。手写 facade 可以映射 UI 友好类型，但不得重新定义 JDP
或 Runtime wire type。

## 10. 外部 adapter 与未知特性

- importer 先生成隔离 revision、`ChangeProposal` 与 `UnknownFeatureInventory`，不直接移动 project head；
- 可无损保留但暂不能解释的字段作为有来源、有 hash、只读的 opaque projection fragment；
- unknown feature 影响 connectivity、几何、规则或制造时，原生编辑/export 必须阻断；
- 只读 opaque fragment 发生外部变化时创建新 import candidate，不能覆盖旧 fragment；
- external tool 只写 execution 临时目录；daemon 独立解析输出并提交 JIR candidate；
- reference equivalence 比较语义对象、单位和容差，不以“文件能打开”代替；
- 任何 adapter 的 capability/coverage 都进入 assessment，未覆盖能力不能默认为通过。

## 11. 风险与支持矩阵

一个操作的最终能力是以下集合的交集：

```text
ProductSupportProfile
∩ ProjectRiskProfile
∩ ToolchainCapabilities
∩ DataPolicy
∩ ActorRole
∩ RuntimePolicy
∩ DaemonDomainGate
```

用户或模型可以提高风险等级，不能自行降低。降低风险、解除禁止能力或创建 waiver 必须有具名、具 scope 的
人类决定；不支持高压/RF/安全关键等能力时，即使 ERC/DRC 为 0 也不能提升成熟度。

## 12. 必须存在的边界测试

1. Runtime/UI 缓存旧 revision 时，daemon 拒绝 stale write 且 UI 不覆盖 current pointer；
2. 同 idempotency key 的同输入返回原结果，不同输入冲突；
3. daemon commit 后响应丢失，Runtime 对账而非创建第二 revision；
4. 同 RevisionId 不允许出现不同 canonical snapshot/metadata；
5. rule/profile/toolchain 变化使旧 assessment 和 release request stale；
6. 请求体伪造 actor/role 不能获得权限；
7. desktop-owned 退出清理子进程，attached 模式不杀外部 owner；
8. runtime/daemon schema 不兼容时 UI 显示明确原因且不降级；
9. unknown external feature 影响连接或制造时阻断 native export；
10. Agent approval、Runtime allow 和 release request 均不能覆盖 daemon hard deny；
11. Release manifest 文件被替换、签名无效或 release 被撤销时离线 verifier 失败；
12. 工作台显示真实 sidecar health，但在真实 adapter 接通前仍明确标注 fixture 数据。

## 13. 当前实现边界

五会话集成已经证明 Rust daemon、生成 binding、Runtime、领域 registry、KiCad 10 reference gate 和 Windows
sidecar packaging 可以组合运行，但当前仍是 **foundation integration**：

- 工作台设计与 Agent 数据仍来自 versioned mock clients；
- Runtime 生产本地 RPC、OS/IAM actor claims 和真实 JDP reconciliation 尚未完成；
- revision/commit/proposal 与 assessment/release 规范 schema 已完成 Slice 0 迁移；assessment 计算、签名和失效图仍待 Slice 6；
- 真实 KiCad round-trip import、供应链、DFM、样板和签名发布仍未闭环。

因此当前不能声称完整 Phase 0 product exit，更不能声称 Production Release。
