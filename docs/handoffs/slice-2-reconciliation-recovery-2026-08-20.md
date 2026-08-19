# Slice 2：跨进程对账、取消与恢复交接

> 日期：2026-08-20
> 状态：Pass
> Production Release：Blocked

## 1. 已关闭的边界缺口

- Runtime 持久化的 ToolExecution ID 直接成为 JDP `executionId`，daemon RequestContext、operation journal、terminal
  execution ledger 与 trace 使用同一身份，不再从模型 `toolCallId` 另造关联 ID。
- JDP 正式增加 `execution.reconcile`、`task.get`、`task.cancel`、`task.wait`；Rust codegen 同步生成 TypeScript envelope、
  JSON Schema 和 tool catalog，真实 Runtime sidecar 按 daemon capability 报告 reconciliation。
- daemon 在向客户端返回前以 canonical SHA-256 持久化完整 terminal response；同 execution identity 重放时验证
  project、operation、idempotency key 和 response hash，任何冲突或损坏均 fail closed。
- cancellable operation 使用持久 task projection。Runtime abort 通过独立认证 pipe 发出 out-of-band cancel，避免被
  主连接的串行长请求阻塞。
- 内部任务只有在 daemon 确认取消后进入 `cancelled`。外部副作用取消、请求截止、panic、owner 丢失或 daemon
  重启无法证明终态时统一进入 `needs-reconciliation`，包含 quarantine reason 和 repair actions。

## 2. 真实故障注入证据

`packages/agent-runtime-sidecar/test/runtime-pipe-integration.test.ts` 启动正式 Rust daemon 和 Runtime sidecar，在
`edit_command.commit` 请求完整写入 named pipe 后立即销毁客户端连接。新连接调用 `execution.reconcile` 得到
`succeeded`、64 位 response hash 与原 terminal response；随后使用完全相同 execution/idempotency identity 重放，
响应逐字段相同，project head 仍未被 draft commit 移动。

`crates/jingwei-daemon/tests/recovery_control.rs` 覆盖：

- `task.wait` 有界等待与内部 confirmed cancellation；
- external-process cancellation 无法证明副作用终态时安全隔离；
- request deadline 立即把 task 转为隔离终态，且 execution failure 可对账；
- daemon 启动把磁盘中未完成 task 恢复为 `needs-reconciliation`。

`crates/jingwei-daemon/tests/vertical_slice.rs` 证明同 execution identity 返回原 response、不同 execution 加相同
idempotency identity 只返回原 revision，并可按 response hash 对账。

## 3. 验收结果

- `cargo fmt --all -- --check`：Pass
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：Pass
- `cargo test --workspace --all-targets --all-features`：Pass
- `pnpm check`：Pass；商业 OpenAI live test 因环境未配置明确 skip
- 正式 Runtime/daemon Windows pipe 丢响应 integration：Pass
- `scripts/build-windows-sidecars.ps1`：Pass
- `scripts/verify-windows-sidecars.ps1`：Pass；`reconciliation: true`，28 个 operation

## 4. 未越界声明

- quarantine 只能由后续具备可信 OS/IAM actor claim、证据绑定和审计策略的人工恢复动作解除；当前版本不提供
  无认证的“强制继续”接口。
- 原生 schematic/PCB display-list、黄金板人工编辑与 KiCad schematic equivalence 属 Slice 3。
- `OPENAI_API_KEY` 和 `JINGWEI_OPENAI_MODEL` 未配置，商业 OpenAI live test 未执行。
- 签名发布、制造商 DFM、样板与组织审批仍未闭环，不能声明 Production Release。
