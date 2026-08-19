# Task 02 验收记录

验收环境：Windows，Rust 1.97.1，2026-08-19。

## 已验证结果

| 验收项 | 结果 | 证据 |
| --- | --- | --- |
| Cargo workspace | 通过 | 七个核心 crate 均参与 build/test/clippy |
| 格式与静态检查 | 通过 | `cargo fmt --all -- --check`；全 workspace/all targets/all features 的 Clippy `-D warnings` |
| Rust 测试 | 通过 | workspace 单元、集成、Windows named-pipe 和 doc tests 全部通过 |
| Windows named pipe | 真实通过 | 建立 `\\.\pipe\jingwei-test-*`，完成 token handshake，并经同一 pipe 调用 `workspace.capabilities` |
| CLI 纵向切片 | 通过 | create → compile → query → patch → diff → validate → events；revision pointer 和事件序列连续 |
| stale base | 通过 | typed conflict，current pointer 不移动 |
| idempotency | 通过 | 同 key/同 base/同规范输入返回原结果；同 key/不同输入冲突 |
| draft | 通过 | open/preview/commit/undo/redo/checkpoint/publish；draft pointer 与 project pointer 隔离 |
| crash recovery | 通过 | AfterPrepare、AfterRevision、AfterJournal、AfterPointer 四个注入点均恢复到一致状态 |
| schema migration | 通过 | 只沿显式 registry 相邻路径迁移；未知路径失败 |
| TypeScript | 通过 | Rust codegen 后，TypeScript 5.9.2 fixture `--noEmit` 编译通过 |
| diff hygiene | 通过 | `git diff --check` |

Windows named-pipe 测试不是 in-memory 替代品；完整设计写入纵向测试使用 in-memory transport，但仍经过 JDP 序列化/反序列化与同一个 daemon dispatcher。

## JDP operation 状态

已实现：

- `project.create`、`project.get`
- `design.compile_jdl`、`design.query`、`design.explain`
- `design.patch.propose`、`design.patch.apply`、`design.diff`、`design.validate`
- `edit_session.open`、`edit_command.preview`、`edit_command.commit`
- `edit.undo`、`edit.redo`、`edit.checkpoint`、`edit.publish`
- `task.events`、`artifact.get`、`workspace.capabilities`

协议存在但显式不可用：

- `task.get`、`task.cancel`：没有注册可查询/取消的长时 durable task，返回 typed `capability_unavailable`，并在 `workspace.capabilities` 中标记为不可用。

任何未知 operation 都返回 typed `capability_unavailable`。Production Release 与 waiver 不在本会话 operation catalog 中，未提供绕过入口。

## 已知 Phase 0 边界

- JDL 是有界手写 parser，不执行文件、网络、进程或任意代码；语法尚未承诺 v1 冻结。
- daemon 实现 request deadline 和 panic containment；尚无可供真实长时任务使用的 scheduler/cancel executor。
- 本地握手使用每实例随机 session token。Windows named pipe 依赖当前用户会话和随机 token；安装器层面的显式 pipe ACL 收紧仍需会话 05 完成。
- store schema 当前为 v1。遇到更高版本时以只读模式打开；v1 尚无需要执行的历史迁移，因此没有产生迁移备份。首次引入有损/结构迁移前必须补充备份实现与故障测试。
- `artifact.get` 仅接受 opaque artifact ID；store 不接受客户端任意绝对路径。
- 没有运行长时间 fuzz campaign。固定 fixture、属性测试和输入限制覆盖了 Phase 0 核心路径；后续 parser/operation 变更应建立持续 fuzz corpus。

## CLI 实测摘要

一次全新临时工作区的实测产生三条不可变 revision：空项目、JDL 编译、label patch。编译图包含 8 个 entity 和 8 个 edge；query 返回稳定 entity ID；diff 只报告目标 entity 修改；validate 返回 valid；事件序列为 1、2、3。
