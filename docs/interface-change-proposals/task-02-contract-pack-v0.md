# ICP：Task 02 Contract Pack v0

- 状态：proposed for integration
- 提案方：Task 02 Rust kernel/daemon
- 目标会话：01、03、04、05
- 当前协议：JDP 0.1 / JIR 0.1 / JDL AST 0.1

## 决策

采用 Rust 类型作为 JDP/JIR 的唯一规范源，并通过显式 extension registry 把 Agent runtime、schematic、PCB 与 workbench 接到单写 kernel。跨会话模块不得写 store、实现第二套 IPC 信封或手写 TypeScript 正式类型。

## 稳定接口

1. opaque ID：workspace/project/revision/entity/edge/request/idempotency/task/execution/session/artifact/evidence。
2. JDP request/success/failure/event envelope 与 structured error。
3. Phase 0 operation 名称、permission、effect、resource claim、timeout/cancel 元数据。
4. JIR 开放 namespaced entity/edge、typed quantity、provenance、edit-control、canonical hash。
5. `ExtensionRegistryBuilder` 及 tool/validator/view/projection/edit/invalidation/schema 注册边界。
6. draft session 的 preview/commit/undo/redo/checkpoint/publish 语义。
7. Windows named-pipe handshake 与 transport-independent `JdpClient`。

## 请求各会话执行

- 会话 01：只从生成 catalog/binding 构造 tools；按 capability、permission、base revision 和 idempotency 发送请求。
- 会话 03/04：返回 core patch/inverse 与 provider output；使用 namespaced schema；不依赖 store/daemon crate。
- 会话 05：在 composition root 注册 modules，负责 daemon 生命周期和当前用户 pipe ACL，UI 只使用 JDP client。

## 兼容与变更门槛

新增 optional 字段或新 namespaced kind 可做 additive 变更。下列变更必须提交新的 ICP、升级 protocol/schema 并更新 codegen fixture：operation/wire name 改动、required 字段、字段语义变化、ID 格式依赖、pointer/transaction 语义变化、permission/effect 降级、release/waiver 能力增加。

## 当前非承诺

- JDL surface syntax 尚未冻结为 v1。
- `task.get`/`task.cancel` 没有 durable scheduler 实现，明确为 capability unavailable。
- Production Release、组织签名、通用 waiver bypass、KiCad 与领域几何不属于本 contract pack。
- tool catalog 的 per-operation input/output schema 在 v0 为 object envelope；强类型 operation 定义以生成 TypeScript/Rust types 为准，精化 catalog schema 需保持向后兼容。

## 验收方式

各会话接入 PR 必须证明：没有直接 store 写入；codegen 后 clean；生成 binding fixture 可编译；capability unavailable 不被伪装成 disabled UI-only 控件；至少一个真实 JDP 请求经过统一 daemon dispatcher。
