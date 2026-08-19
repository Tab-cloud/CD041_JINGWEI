# 精卫 Agent 全局设计收拢审计与实施计划

> 版本：1.0
> 日期：2026-08-19
> 执行方式：后续统一由当前对话、当前独立 Git 仓库和集成主线推进，不再依赖任务会话交付

## 1. 审计结论

五个任务的代码和 handoff 已经形成可运行基础，但并未达到完整 Phase 0 产品退出。审计将状态拆成三层：

- **Foundation integrated**：crate/package/daemon/runtime/workbench 可以构建、测试和打包；指定技术 spike 通过；
- **Phase 0 product exit**：真实 UI/Runtime/JDP 纵向写路径、对账、黄金板 round-trip、签名安装等退出条件全过；
- **Production released**：制造商 DFM、样板、测试、供应链和组织签名全部有效。

当前状态是第一层通过，第二层和第三层均阻断。

## 2. 全局边界缺陷与处置

| ID | 级别 | 审计发现 | 设计处置 | 实现状态 |
| --- | --- | --- | --- | --- |
| AUD-001 | P0 | “Phase 0 集成通过”容易被理解成完整 Phase 0 exit | 统一为 `foundation integration accepted`，单列 product exit gates | 本轮文档收拢 |
| AUD-002 | P0 | Runtime 控制面与 daemon 都被描述为 revision pointer/阶段 owner | daemon 唯一拥有设计 pointer、assessment 和阶段投影；Runtime 只缓存观察值 | 本轮设计完成，代码需适配 |
| AUD-003 | P0 | Tauri 文档要求退出不杀 sidecar，但 desktop 实现必须杀进程 | 定义 desktop-owned/attached/headless 三种 owner mode | 本轮设计完成；当前仅 desktop-owned |
| AUD-004 | P0 | Runtime 正式本地 RPC 未定义，stdio handshake/RPC 与监控混流 | bootstrap 仅走 stdin；正式 Runtime API 使用独立 pipe/UDS 与生成协议 | 待实现 |
| AUD-005 | P0 | UI 仍直接构造 MockRuntimeClient/MockDesignClient | 建立真实 RuntimeTransport、DesignTransport 与显式 fixture mode | 待实现 |
| AUD-006 | P0 | Runtime 已有 reconciliation 抽象，真实 sidecar 却报告 false | 增加 daemon execution reconciliation operation 和真实 adapter | 待实现 |
| AUD-007 | P0 | Revision metadata 同时承担设计内容、draft/candidate/verified/released/superseded | 分离 RevisionSnapshot、CommitRecord、DraftSession、ChangeProposal/Decision、MaturityAssessment、ReleaseRecord | 本轮设计完成，需 store migration |
| AUD-008 | P0 | 文档声称所有 ID 都是 UUID，并描述了已过时的 JIR entity shape | wire ID 改为 opaque；Rust schema 是唯一规范源 | 本轮文档收拢 |
| AUD-009 | P0 | `createdBy`/actor 字符串没有生产身份与角色签名边界 | actor 来自 OS/IAM claims；发布由独立组织 signer 执行 | 本轮设计完成，待身份 adapter |
| AUD-010 | P0 | 门禁阶段可能被当成可写工作流状态 | 阶段由 daemon 根据 assessment 计算；Runtime workflow 只能请求动作 | 本轮设计完成，待 assessment domain |
| AUD-011 | P0 | JDP/JIR/store/runtime/event/profile/toolchain 兼容策略分散 | 建立独立版本矩阵、只读/拒绝/migration 规则 | 本轮设计完成，待兼容测试 |
| AUD-012 | P0 | 当前文档中的 Phase 0 清单有大量未完成项但缺少状态矩阵 | 建立下节统一退出矩阵与执行顺序 | 本轮完成 |
| AUD-013 | P1 | Artifact 普通 frame、分页 cursor 和大型 display list 的传输边界未冻结 | CAS/stream ticket、revision/query-bound cursor、独立 display channel | Slice 1 设计/实现 |
| AUD-014 | P1 | 外部格式 unknown feature 的保留与阻断粒度不够明确 | UnknownFeatureInventory + opaque只读 fragment + capability coverage | 本轮设计完成，待 adapter 实现 |
| AUD-015 | P1 | 供应链 TTL、离线快照、许可证与替代料再验证尚未闭环 | Provider snapshot contract + data policy + impact graph | Slice 4 |
| AUD-016 | P1 | 安装包已可安装但未签名、无升级/回滚/SBOM | 独立 release engineering gate | Slice 7 |
| AUD-017 | P0 | JDP 注释称 publish 到 candidate pointer，但实现实际移动 `ProjectCurrent` | 只保留 project head；待审设计进入 ChangeProposal，接受时 CAS 移动 head | 本轮设计完成，需 kernel migration |
| AUD-018 | P0 | RevisionId 哈希未覆盖 createdAt/actor/status/artifact/evidence，同 ID 可对应不同 metadata | canonical snapshot 与 commit/assessment/manifest 分离；ID 覆盖 snapshot 全字段 | 本轮设计完成，需 schema migration |
| AUD-019 | P0 | 当前 revision `verify()` 只校验 graph hash，未验证完整 ID 与 journal linkage | 增加 full identity/journal verification 和 corruption fixtures | Slice 0 |

P2 的 RF/SI/PI、高压/安规、HDI/BGA、刚挠、多人协作和云端服务不进入当前纵向 MVP；它们通过 support profile
保持显式不可用，不能以通用提示词提前开放。

## 3. Phase 0 product exit 状态矩阵

| 退出项 | 当前状态 | 证据/阻塞 |
| --- | --- | --- |
| 独立 Rust workspace、daemon 单写、事务/恢复 | Partial pass | vertical slice 与 store tests 已过；仍需真实磁盘满/升级故障矩阵 |
| Runtime durable input/run/tool/approval/event | Pass（基础） | 42 项 Runtime 测试；商业 provider live test 未执行 |
| 正式 Rust→TypeScript binding | Pass | reproducible codegen 与真实 daemon integration test |
| Runtime→daemon trace context | Pass | 正式 binding + Windows named pipe + Rust child span |
| Runtime→daemon reconciliation | Blocked | sidecar capability 为 `false` |
| CLI/Agent/GUI 同一 patch 同 revision | Blocked | GUI 仍是 mock DesignClient |
| 真实 Runtime/UI adapter 与事件回放 | Blocked | UI 仅显示 sidecar process health |
| JDL/JIR 最小编译与 migration | Pass（Slice 0） | JIR 0.2 canonical identity、显式备份 store migration、newer read-only/downgrade refusal、proposal CAS 和负向测试通过；黄金板 round-trip 仍属 Slice 3 |
| KiCad schematic/PCB projection | Partial pass | KiCad 10 PCB DRC/Gerber/Drill 通过；完整 JIR↔KiCad round-trip 未通过 |
| 原生 ERC/DRC 与 reference 差异映射 | Partial pass | domain rules/fixtures 已有；真实同板双后端 equivalence 未完成 |
| Freerouting DSN/SES 真实闭环 | Blocked | adapter 存在，受控可执行文件与真实 gate 未交付 |
| 供应商与真实器件证据 | Blocked | 当前以 fixtures/本地模型为主，无已批准商业 provider snapshot |
| 制造包完整格式与独立 parser | Partial pass | reference fabrication gate 通过；智能格式与独立全包验证未闭环 |
| Windows 签名安装、升级/回滚/卸载 | Partial pass | 安装/卸载和 sidecar 路径通过；签名、升级、回滚未完成 |
| 商业模型真实工具调用 | Blocked by environment | 缺少 `OPENAI_API_KEY` 与 `JINGWEI_OPENAI_MODEL`，live test 明确跳过 |
| 真实黄金板人工审查/样板 | Blocked | 尚无具名硬件/PCB 审查、工厂 DFM、实物样板和测试记录 |

## 4. 统一实施顺序

后续不再按原五会话边界推进，而按一个纵向产品闭环推进。每个 slice 都由本对话审计、实现、测试、提交并更新
同一份状态矩阵。

### Slice 0：规范类型和迁移收拢

状态（2026-08-20）：**Pass**。Rust 已成为 revision/lifecycle schema 的唯一规范源；旧 store 显式迁移、活动
writer/pending transaction 拒绝、完整 identity/journal 校验、proposal 不移动 head 与接受时 CAS、daemon
instance/epoch/invocation correlation、生成 TypeScript/JSON Schema 和 CLI migration/decision 都有自动化与真实 CLI
纵向证据。详细记录见 `handoffs/slice-0-canonical-lifecycle-2026-08-20.md`。

目标：消除继续开发会放大的数据模型歧义。

- 将 `RevisionStatus` 迁移为 RevisionSnapshot、CommitRecord、DraftSession、ChangeProposal/Decision 和 project head；
- 新增 MaturityAssessment、GateResult、ReleaseRequest/ReleaseRecord/RevokeRecord schema；
- 为 workspace store 提供 migration、备份、newer-version read-only 与 downgrade refusal 测试；
- 将 actor、`toolExecutionId | commandInvocationId`、daemon instance/epoch 纳入 operation correlation；
- 重算 RevisionId、graph hash 和 journal linkage，加入 metadata substitution/corruption negative tests；
- 从 Rust 重新生成 bindings，禁止手改正式类型。

退出条件：同 RevisionId 不能对应不同 canonical snapshot；旧 Phase 0 store 可迁移；draft/candidate/release 不再是
revision enum；proposal 接受使用 project head compare-and-swap。

### Slice 1：真实本地控制协议和 UI adapter

目标：移除“进程是真的、数据是 mock”的主断点。

- 为 Runtime sidecar 建立 Windows named pipe、后续 UDS 的 versioned RPC server；
- 从 `agent-protocol` 生成/验证 request、response、event schema；
- 实现 Tauri lifecycle + endpoint discovery，不代理大对象；
- 实现 `RuntimeTransportClient` 与 `DesignTransportClient`；
- fixture mode 必须显式启动，生产安装默认禁止自动回退 fixture；
- UI 初始化、重连和刷新从真实 capabilities/event sequence 恢复。

退出条件：安装版创建真实 session/project，读取 daemon 工程并执行一个真实 draft patch；UI 中没有伪装连接。

### Slice 2：跨进程幂等、取消和恢复

目标：关闭 daemon 已提交但 Runtime 未结算的重复副作用风险。

- 新增 JDP `execution.reconcile`、task cancel/terminal wait 和 response-hash 查询；
- Runtime sidecar 报告真实 reconciliation capability；
- 故障注入响应丢失、Runtime 崩溃、daemon 崩溃、取消超时和 unknown external process；
- quarantine 有可见原因、修复动作和人工解除权限。

退出条件：commit 后丢响应不会生成第二 revision；所有未知外部副作用 fail closed。

### Slice 3：黄金板真实 logical 闭环

目标：自然语言需求到受控原理图 candidate，不使用 UI fixture。

- 冻结温湿度/RS-485 黄金板 DesignIntent；
- 真实 MPN/证据快照、库解析/生成、pin-pad 验证；
- JDL→JIR logical graph→原生原理图 display list；
- native ERC + KiCad schematic projection/import/equivalence；
- UI 人工编辑、锁、undo/redo 和 AI 后续 patch 全走真实 JDP。

退出条件：硬件工程师可在安装版内审查、修改并复现同一 schematic candidate。

### Slice 4：供应链和库治理

目标：把“能生成符号/封装”提升为可追溯且可撤销的受控库。

- provider adapter、缓存 TTL、币种/数量阶梯、API ToS 与 data classification；
- LibraryAsset approval/revoke、项目级与组织级作用域；
- 错误库影响查询、替代料再验证和 offline frozen snapshot；
- 连接器/机械关键器件 3D 模型门禁。

退出条件：过期/撤销/冲突数据能阻断候选，不能靠相似名称静默绑定。

### Slice 5：真实 physical 与制造候选

目标：黄金板从约束到可上传制造候选。

- manufacturer profile/stackup、板框、placement、critical-net route；
- 受控 Freerouting 可选候选与原生 route fallback；
- native DRC + KiCad DRC + semantic equivalence；
- Gerber/drill/BOM/CPL/IPC-356/STEP/manifest 从同一 revision 导出；
- 独立 parser、stale/mixed artifact negative gates。

退出条件：参考工厂能正确识别制造候选；任何 router/DRC 失败均不会创建可接受 proposal 或移动 project head。

### Slice 6：评审、成熟度和 Release Candidate

目标：让 UI 和 headless verifier 显示同一 gate 真相。

- assessment/invalidation graph、issue/waiver 生命周期和 role claims；
- 工程审查、供应链、制造、测试 approval attestations；
- signed release request 与离线 verifier；
- UI 显示 L0-L5、stale reason 和受影响对象。

退出条件：任一设计/profile/rule/evidence 变化都会使旧 assessment/release request 失效。

### Slice 7：Windows 发布工程

目标：从开发安装包提升为可受控分发的 Alpha 包。

- UI、Runtime、daemon、updater 签名与 manifest；
- 干净机安装、升级、回滚、卸载、离线启动和 WebView/GPU fallback；
- SBOM、第三方 notice、source/provenance attestation；
- sidecar 替换、版本错配、管道抢占、磁盘满和崩溃负向测试。

退出条件：签名/更新/回滚门禁全过，不能用当前 unsigned installer 替代。

### Slice 8：真实样板闭环

目标：把 Manufacturing Candidate 提升到 Prototype Verified；Production Release 仍按组织政策执行。

- 制造商 DFM/CAM 回写；
- 订单/批次/样板与 revision/manifest 绑定；
- 限流上电、功能、接口、热和必要可靠性测试；
- 问题回写新 revision 并自动失效旧 gate；
- 具名硬件、PCB/制造、测试、供应链与 Release Manager 签核。

退出条件：真实样板证据完整；没有证据时产品只能停在 Manufacturing Candidate。

## 5. 执行规则

- 当前集成分支是唯一收拢主线；旧 task 分支和 handoff 只读归档；
- 每个 slice 开始先更新本矩阵，结束时提交代码、schema、migration、测试、真实证据和剩余缺口；
- 不以 mock、单元测试、文件存在或进程健康替代真实 adapter/工具/安装/硬件门禁；
- 不并行打开会改变同一权威 schema 的多个实现方向；先冻结接口，再推进纵向实现；
- 发现新的 P0 边界缺陷时先修边界再叠加功能；
- 未获外部凭据、法务结论、工厂反馈或硬件样板时，明确记录 external blocker，不伪造通过。

## 6. 下一步

Slice 0 已闭合，下一步进入 Slice 1：实现 Runtime 正式本地 RPC、Tauri endpoint discovery 和真实 UI adapter，以便
立即替换 mock clients。两项完成前，不继续扩展新的 UI fixture、router heuristic 或高级板型。
