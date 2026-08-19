# 精卫 Agent Slice 0：规范类型与迁移验收

> 日期：2026-08-20  
> 结论：Slice 0 Pass；不代表 Phase 0 product exit 或 Production Release。

## 1. 已闭合边界

- JIR schema 升级到 `0.2`，`RevisionSnapshot` 只保存 canonical design bytes、parents、schema、graph hash 和
  operation journal hash；actor、时间、成熟度和发布状态不再改变同一个 revision 的含义。
- 新增独立 `RevisionCommitRecord`、`ChangeProposal`、`ProposalDecisionRecord`、`MaturityAssessment`、
  `GateResult`、`ApprovalAttestation`、`ReleaseRequest`、`ReleaseRecord` 和 `RevokeRecord` schema。
- revision verification 重算 graph hash 和完整 RevisionId；store transaction 验证 journal operations hash、
  commit、request、idempotency 与 revision 的一一关联。
- 编译、patch 和 draft publish 只创建 immutable proposal，不移动 project head；accept decision 与 project-head
  compare-and-swap 由 daemon/store 原子完成。reject/withdraw 为 append-only decision。
- store schema 升级到 `2`。旧版只能只读打开；迁移创建备份，活动 writer 或 pending transaction 时拒绝；新版
  只读打开并拒绝 downgrade。
- commit correlation 记录 authenticated actor、invocation source/id、request、daemon instance 和持久 daemon
  epoch。测试证明 daemon 重启后 instance 改变、epoch 单调递增。
- TypeScript binding、tool catalog、revision/lifecycle JSON Schema 均由 Rust codegen 重建。
- CLI 增加 `jingwei migrate` 和 `jingwei proposal ... accept|reject|withdraw`。

## 2. 验收证据

- `cargo test -p jingwei-jir`：canonical substitution/journal corruption 负向测试通过。
- `cargo test -p jingwei-store`：7 项事务、迁移、锁、downgrade、proposal CAS、crash recovery 测试通过。
- `cargo test -p jingwei-daemon --test vertical_slice`：真实 JDP proposal、idempotent replay、stale base、draft
  publish/accept 通过。
- `cargo test -p jingwei-daemon --test daemon_epoch`：真实 journal correlation 和 daemon restart epoch 通过。
- `pnpm --dir bindings/typescript typecheck`：生成 binding 类型检查通过。
- `cargo fmt --all -- --check` 与 `cargo clippy --workspace --all-targets -- -D warnings`：通过。
- `cargo test --workspace --no-fail-fast`：Rust workspace 全部通过。
- `pnpm check`：42 pass、1 个商业 OpenAI live test 因缺少环境配置明确 skip、0 fail；全部 package build 通过。
- codegen SHA-256 前后比较：4 个正式生成物重复生成后完全一致。
- CLI 临时工作区纵向门禁：compile 后 head 保持不变；accept 后 head 精确移动到 proposed revision。

## 3. 未冒充完成的后续项

- OS/IAM identity adapter 与组织角色签名属于 Slice 1/6；当前本地 actor 明确标记为 local development。
- assessment 计算、invalidation graph、离线签名验证属于 Slice 6；本 Slice 只冻结规范 schema。
- Runtime 正式 pipe RPC、真实 UI transport 和 reconciliation 属于 Slice 1/2。
- 商业模型、供应商、Freerouting、工厂 DFM、签名证书和真实样板仍为外部输入门禁。
