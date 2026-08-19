# Store、事务与恢复说明

## 目录职责

工作区状态集中在 daemon 管理的 `.jingwei` 根目录：

- workspace metadata 和 writer lock：工作区身份、schema、实例所有权。
- projects：项目 metadata、append-only current/draft pointer 记录。
- revisions：按 opaque revision ID 保存不可变 snapshot。
- journals：每次 operation 的请求、base、输入 hash、operation、输出和 revision 关联。
- idempotency：按 key 保存规范输入身份与原结果。
- artifacts：按 SHA-256 寻址的 metadata 与 bytes。
- events：按 aggregate 单调 sequence 追加的 durable event。
- transactions：prepare manifest 与待安装文件；恢复完成后清理。
- quarantine：无法安全判定的事务，绝不猜测性重放。

所有路径由 store 根据 validated opaque ID 构造；外部调用方不能提交任意绝对路径。

## 单写与提交顺序

`WorkspaceStore::acquire_writer` 同时使用进程内 registry 与 OS file lock，阻止同一进程或不同进程的第二 writer。只有 kernel transaction service 构造 commit；daemon 是工作区内唯一持有 writer guard 的服务。

每个 commit 的关键顺序：

1. 校验 expected current/draft pointer。
2. 写 transaction prepare manifest 和临时 payload。
3. 安装不可变 revision。
4. 追加 operation journal 与 idempotency record。
5. 追加目标 pointer record。
6. 追加 durable event并完成事务。

失败在 pointer 之前不会改变可见 head；失败在 pointer 之后由 journal/idempotency/event 补齐到同一已知结果。current snapshot 从不原地覆盖。

## Recovery 规则

启动时扫描 transaction manifest：

- 尚无 revision：丢弃未提交 prepare。
- 有 revision、无 journal：从 manifest 安装 journal/idempotency，再推进指定 pointer。
- 有 journal、无 pointer：验证 base/expected pointer 后推进指定 pointer。
- pointer 已推进：补齐 event 并把事务标记完成。
- manifest、revision、journal 或 pointer 互相矛盾：移入 quarantine，工作区保持已知 pointer，不自动重复未知 side effect。

故障测试通过四个确定性 crash injection：`AfterPrepare`、`AfterRevision`、`AfterJournal`、`AfterPointer`。每个测试都重新打开 store、执行 recovery，并校验 revision/journal/pointer/event 一致；这模拟 kill/restart 的持久边界，不是只检查内存错误返回。

## Schema 与只读

当前 store schema 是 v1。等于 v1 时读写；高于本程序支持版本时返回明确只读原因，writer 获取失败。当前没有旧 store schema 需要升级；首次添加 v1→v2 migration 时必须在迁移前创建完整备份、用临时目录生成新布局、原子切换，并补充每个阶段的 crash test。
