# 会话 01 任务书：Agent Runtime 控制面

> 建议分支：`feat/jw-01-agent-runtime`  
> 主要语言：TypeScript / Bun  
> 任务性质：从现有 Eval runtime 抽取、加固并适配 Jingwei；不是从零重写。  
> 统一合并顺序：第 2 个合并，位于 Rust JDP/kernel 之后。

## 1. 可直接交给会话的任务描述

你负责 Jingwei-Agent 的通用 Agent 控制面。请基于当前 `embedded-agent` 真实源码和测试抽取可复用 runtime，
实现 durable session/run、provider、单一工具结算、权限审批、事件回放、MCP 隔离和 OpenTelemetry，并通过
`JdpClient` 调用 Rust EDA daemon。不得实现 EDA 领域规则，不得直接写 JIR/workspace，不得整体复制或 fork
OpenCode/Codex。先用 contract tests 固定 Eval 行为，再做共享 package 抽取；JDP binding 尚未可用时使用明确
标注的 mock，交接前提供切换真实 binding 的 adapter 和测试。

## 2. 目标与完成结果

本会话完成后应得到一个与品牌无关、可被 Eval 和 Jingwei 共同使用的 Agent runtime 基线：

- 用户输入先持久化再执行；
- 同一 session 串行、不同 session 可并行；
- `steer` 与 `queue` 有不同且可恢复的语义；
- provider 半流失败不会污染下一 attempt；
- 内置/JDP/MCP/plugin 工具只有一个 registry/settlement 入口；
- ToolExecution 和 approval 持久化顺序正确；
- Main Agent 拥有会话，专业任务作为受限工具；
- UI 可以按 sequence 回放 run/tool/approval 事件；
- runtime 只能通过 JDP client 提交设计动作；
- MCP instructions/annotations 不能提升权限；
- trace 可与后续 JDP/daemon span 关联。

## 3. 开始前必读

### Jingwei

- `docs/tasks/00-five-session-execution-plan.md`
- `docs/12-agent-runtime-reference-architecture.md`
- `docs/01-system-architecture.md`
- `docs/04-tool-contracts.md`
- `docs/06-security-compliance-and-audit.md`
- `docs/07-roadmap-and-acceptance.md`

### 当前真实参考源码

- `../embedded-agent/AGENTS.md`
- `../embedded-agent/packages/core/src/runtime/agent-loop.ts`
- `../embedded-agent/packages/core/src/runtime/agent-loop-tool-execution.ts`
- `../embedded-agent/packages/core/src/runtime/agent-authority.ts`
- `../embedded-agent/packages/core/src/runtime/tool-approval.ts`
- `../embedded-agent/packages/core/src/runtime/session-command-queue.ts`
- `../embedded-agent/packages/core/src/store/tool-execution.ts`
- `../embedded-agent/packages/core/src/mcp/manager.ts`

OpenCode V2 的 session/input/run-coordinator/event/tool/permission 源码只用于验证设计不变量，不作为运行依赖。

## 4. 文件所有权

本会话可以修改：

```text
package.json
pnpm-lock.yaml                   # 仅本分支依赖生成差异；最终按总任务书 4.1 重生成
pnpm-workspace.yaml
packages/agent-runtime/**
packages/agent-tools/**
packages/agent-policy/**
packages/agent-events/**
packages/agent-mcp/**
packages/agent-observability/**
packages/agent-protocol/**
testdata/runtime/**
docs/handoffs/task-01/**
docs/interface-change-proposals/task-01-*.md
```

禁止修改：

- `crates/**`、`bindings/typescript/**`、`app/**`；
- 五会话任务书和产品/架构基线；
- `embedded-agent` 业务代码，除非用户明确要求把抽取结果同步回 Eval；
- `opencode-dev/**`。

如会话 02 的 `contract-pack-v0` 尚未到达，只能把同 shape JDP fixture 放在 test/mock；不得在 runtime package 中
发布另一套正式协议类型。`pnpm-lock.yaml` 是总任务书 4.1 的生成文件例外，不代表本会话拥有其他会话依赖。

## 5. 数据所有权

Agent runtime 数据库只保存：

- session、input、run、turn、model attempt；
- tool call、tool execution、approval；
- durable runtime event、artifact/evidence reference；
- provider/MCP 配置的非秘密部分和 credential reference。

禁止保存为权威数据：

- JIR snapshot、operation journal、current revision；
- 设计对象几何/连通性完整副本；
- 发布签名密钥、供应商 token；
- Production Release 业务终态。

## 6. 必须提供的包与接口

### 6.1 `agent-protocol`

提供 versioned runtime API 类型：

```ts
interface RuntimeClient {
  createSession(input: CreateSessionInput): Promise<SessionView>
  admitInput(input: AdmitInput): Promise<AdmittedInput>
  resumeRun(sessionId: string): Promise<RunView>
  cancelRun(runId: string, reason?: string): Promise<void>
  events(sessionId: string, afterSequence?: number): AsyncIterable<DurableEvent<unknown>>
  listApprovals(sessionId?: string): Promise<ApprovalView[]>
  replyApproval(input: ApprovalReply): Promise<void>
  getArtifact(artifactId: string): Promise<ArtifactView>
  capabilities(): Promise<RuntimeCapabilities>
}
```

`AdmitInput` 必须包含 `clientRequestId` 和 `delivery: "steer" | "queue"`。相同 request ID 只能返回相同输入，
内容不同则 typed conflict。

### 6.2 `agent-runtime`

需要提供：

- `SessionService`：session 创建、读取、输入 admission；
- `SessionRunCoordinator`：同 session join/coalesced wake/interrupt；
- `AgentLoop`：一个 provider turn、工具 continuation、正常 no-tool completion；
- `ProviderRegistry`：能力声明、fault domain、模型尝试；
- `ContextProjector`：小上下文、artifact 引用、compaction；
- `RecoveryService`：owner lease、进程 epoch、重启对账。

核心状态建议：

```text
Run: queued → running → waiting-approval | waiting-user | completed | failed | cancelled | quarantined
ToolExecution: recorded → authorizing → waiting-approval → running → succeeded | failed | denied | unknown
ModelAttempt: created → streaming → completed | failed | cancelled | discarded
```

### 6.3 `agent-tools`

实现总任务书中的 `RuntimeToolDescriptor` 语义，以及唯一 settlement pipeline：

```text
normalize/schema validate
  → advertised identity/stale check
  → runtime authorization
  → durable approval if required
  → persist ToolExecution
  → execute handler/JDP/MCP
  → output schema/redaction/bounding
  → artifact overflow
  → persist terminal outcome
  → durable event
```

不得保留第二个 legacy executor。工具可见性与授权必须分别测试。

### 6.4 `agent-policy`

至少支持：

- effect：read/design-write/workspace-write/process/network/hardware/release；
- resource claim；
- `allow` / `ask` / `deny`；
- project/session/agent/tool 多层规则，deny 是上限；
- root-only orchestration tools；
- 一次批准、限定范围持续批准、拒绝和过期；
- approval 请求和决定“先持久化后展示/唤醒”。

`allow-all` 只能限定在同 session、同 permission、同资源范围内，禁止批准所有未来工具。

### 6.5 `agent-events`

提供：

- 聚合内递增 sequence；
- durable append；
- `readAfter(aggregateId, sequence)`；
- history + live stream 无缝衔接；
- projection/rebuild；
- schema version 和 migration 测试。

至少持久化：input、run、model attempt 终态、tool、approval、artifact reference、recovery。token delta 可易失。

### 6.6 `agent-mcp`

实现或抽取：

- 每 server 独立连接、namespace 和 allowlist；
- credential reference，不在配置/日志暴露 secret；
- instructions/description/annotations/output 标为不可信；
- 远程工具默认最坏 effects，除非本地可信 policy 明确收窄；
- timeout、取消、输出上限、审计；
- tool list 更新产生新 identity，旧调用返回 stale-tool；
- sampling/递归 Agent 默认禁止。

### 6.7 `agent-observability`

提供 OTel span：`agent.run`、`model.attempt`、`tool.execution`、`approval.wait`、`mcp.call`、`jdp.request`。
propagate `traceId/spanContext` 给 JDP client；禁止记录完整 prompt、私有设计、密钥和大工具结果。

### 6.8 `JdpClient` 消费接口

```ts
interface JdpClient {
  capabilities(): Promise<JdpCapabilities>
  invoke<TInput, TOutput>(request: JdpRequest<TInput>): Promise<JdpSuccess<TOutput> | JdpFailure>
  events(aggregateId: string, afterSequence?: number): AsyncIterable<DurableEvent<unknown>>
  cancel(taskId: string): Promise<void>
  reconcile(executionId: string, idempotencyKey: string): Promise<ExecutionReconciliation>
}
```

第一阶段可提供 `MockJdpClient`。真实 binding 合并后，mock 只能保留在 test/fixture，不得成为生产默认。

## 7. Provider 要求

### 7.1 统一能力声明

每个 provider/model 必须显式声明：

- native tools、structured output、streaming、reasoning、image/PDF；
- context/output 上限；
- cancellation/retry/idempotency；
- fault domain；
- 数据策略 metadata。

需要 native tool 的 run 不得 fallback 到不支持工具的模型并继续假装完成。

### 7.2 本轮实现

- 必须：`FakeProvider`，可脚本化 text/tool/partial stream/error/cancel；
- 建议：OpenAI Responses provider adapter；没有凭据时 live test 明确 skip；
- 可复用现有 Eval provider 归一化和失败域逻辑，但要用 contract test 证明行为；
- 不把 API key 写入 fixture、数据库、日志或交接文档。

## 8. Main Agent 与专业任务

实现：

- root run 必须由 `primary` Agent 拥有；
- Main Agent 可调用专业能力工具；
- 专业 run 只得到显式 tools/context/budget；
- 专业 run 不得获得 root-only tools，不得创建子 Agent；
- 专业结果回到 Main Agent，由 Main Agent 继续/结束；
- 普通 no-tool model completion 正常结束，不引入旧 Claim/评分/task_review 流程。

## 9. 恢复不变量

必须对以下 kill point 建立自动化测试：

1. input 已 durable、run 未启动；
2. provider 已产生 partial text/tool call、未 finish；
3. ToolExecution 已记录、handler 未开始；
4. approval 已请求、等待中重启；
5. JDP 已提交、runtime 未收到响应；
6. MCP/外部工具终态未知；
7. UI/event consumer 断线；
8. runtime schema migration 失败。

恢复不能依赖模型总结。JDP 已提交场景必须调用 `reconcile`，不能创建第二次设计副作用。

## 10. 不在本会话范围

- JDL/JIR/JDP Rust 实现和 daemon IPC；
- 器件、symbol/footprint、ERC/DRC、布局布线、制造包；
- Tauri/React UI；
- Production Release、waiver 或工程师签名；
- 整体迁入 Codex/OpenCode/LangGraph/Microsoft Agent Framework/Temporal；
- 把 Eval 业务 UI/office/avatar 迁入 Jingwei runtime。

## 11. 实施步骤

1. 读取 Eval 当前代码、测试和 diff，列出现有行为与缺口；
2. 为 session/tool/approval/authority/recovery 建 contract tests；
3. 建 pnpm workspace 和七个无品牌 package；
4. 抽取单一 tool registry/settlement，删除/隔离重复入口；
5. 完成 durable input、run coordinator 和 event replay；
6. 完成 approval durability、root authority 和 recovery；
7. 接入 FakeProvider、MockJdpClient；
8. 接入 MCP untrusted policy 和 OTel；
9. 用生成 binding 可用后的 adapter contract 做兼容测试；
10. 生成 task-01 交接包。

## 12. 测试与验收

### 必须自动化

- duplicate request ID 等价返回/冲突；
- 同 session 不并发、不同 session 并发；
- steer/queue 顺序和重启恢复；
- provider partial stream discard；
- no-tool 正常完成；
- tool visible but denied；
- approval 请求/决定持久化失败 fail closed；
- daemon commit 后 reconcile 不重复 revision；
- MCP metadata 提权失败；
- output 超限 artifact 化且模型只收到 bounded result；
- root/subagent 权限；
- history + live event 不丢不重；
- secret redaction 和 OTel 属性白名单。

### 建议命令

```text
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm build
git diff --check
```

如果命令因尚未合并的 workspace scaffold 调整，必须在交接中记录实际等价命令，不能省略静态检查和测试。

## 13. 必交制品

- 七个 package 及 public API 文档；
- Eval 行为 contract tests；
- FakeProvider、MockJdpClient 和真实 JDP adapter 边界；
- SQLite schema/migrations 或等价持久层；
- 故障注入测试报告；
- MCP 安全负向测试；
- OTel trace 示例（脱敏）；
- `docs/handoffs/task-01/` 完整交接包；
- 任何接口差异的 change proposal。

## 14. 会话完成时必须明确报告

- 哪些代码是从 Eval 移动/改写/新建的；
- 是否修改了 Eval 源仓库；
- 当前真实 provider 是否跑过，使用何种非秘密配置；
- 哪些测试使用 mock，哪些是进程/SQLite/网络真实测试；
- JDP 仍使用 mock 还是已经对接生成 binding；
- 所有未关闭的恢复、安全和兼容缺口。
