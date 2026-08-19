# 会话 02 任务书：Rust JDL/JIR/JDP 语义内核与 Daemon

> 建议分支：`feat/jw-02-kernel-daemon`  
> 主要语言：Rust  
> 任务性质：建立所有其他会话依赖的设计可信计算基和协议规范。  
> 统一合并顺序：第 1 个合并。

## 1. 可直接交给会话的任务描述

你负责 Jingwei-Agent 的 Rust 语义核心：建立 Cargo workspace，实现最小 JDL、JIR、JDP、revision/store、
原子 transaction、operation journal、daemon、CLI、事件和 TypeScript binding codegen。`jingwei-daemon` 必须
成为工作区唯一 JIR 写入者，并提供会话 03/04 可注册的 domain tool/validator/view/projection 扩展边界。
本任务不实现具体器件、ERC/DRC、PCB 求解或 UI；重点是稳定、可恢复、可生成绑定的内核和协议。

## 2. 目标与完成结果

本会话完成后，干净环境应能：

1. 创建 Jingwei project/workspace；
2. 编译一份最小 JDL 为规范 JIR revision；
3. 局部 query/explain；
4. 基于明确 base revision propose/apply 原子 patch；
5. 产生 operation journal、semantic diff、artifact/evidence reference；
6. stale base、precondition 失败、重复幂等请求得到确定结果；
7. daemon 崩溃/重启不污染 current revision；
8. Windows named pipe 或测试 transport 提供 JDP；
9. Rust CLI 和生成 TypeScript client 消费同一 schema；
10. 会话 03/04 的独立 crate 可通过 registry 插入工具、validator、view 和 projection provider。

## 3. 开始前必读

- `docs/tasks/00-five-session-execution-plan.md`
- `docs/09-ai-native-eda-architecture.md`
- `docs/10-rust-tauri-platform-architecture.md`
- `docs/02-domain-model-and-artifacts.md`
- `docs/01-system-architecture.md`
- `docs/04-tool-contracts.md`
- `docs/06-security-compliance-and-audit.md`
- `docs/07-roadmap-and-acceptance.md`

## 4. 文件所有权

本会话可以修改：

```text
Cargo.toml
Cargo.lock                         # 本分支生成；统一合并按总任务书 4.1 重生成
rust-toolchain.toml
rustfmt.toml
clippy.toml
crates/jingwei-jdl/**
crates/jingwei-jir/**
crates/jingwei-jdp/**
crates/jingwei-kernel/**
crates/jingwei-store/**
crates/jingwei-daemon/**
crates/jingwei-cli/**
bindings/typescript/**
scripts/codegen-*/**
testdata/kernel/**
docs/handoffs/task-02/**
docs/interface-change-proposals/task-02-*.md
```

禁止修改会话 01/03/04/05 专属目录。Cargo workspace 使用 `members = ["crates/*"]`，不要替其他会话预建空 crate。

## 5. Crate 职责

### 5.1 `jingwei-jdl`

Phase 0 最小语言：

- project/module/interface/block/component/net；
- typed property；
- 物理量和单位；
- connect；
- constraint/assertion；
- source span、diagnostic、formatter；
- 确定性 parse → AST → format → parse round-trip。

JDL 表面语法尚未最终冻结；内部 AST 和 diagnostic 必须版本化。JDL 无文件、网络、进程或任意代码执行能力。

### 5.2 `jingwei-jir`

规范对象：

```rust
struct JirEntity {
    id: EntityId,
    kind: EntityKind,       // 开放的 namespaced string，不使用封闭总枚举阻塞领域扩展
    schema_version: SchemaVersion,
    properties: CanonicalValue,
    provenance: Provenance,
}

struct JirEdge {
    id: EdgeId,
    kind: EdgeKind,
    from: EntityId,
    to: EntityId,
    properties: CanonicalValue,
}
```

必须支持：

- 稳定 ID；
- canonical serialization 和 hash；
- typed quantity/unit；
- entity/edge schema registry；
- graph index、引用完整性；
- revision snapshot；
- schema migration；
- provenance 和对象 edit-control 元数据。

领域属性可以由会话 03/04 的 namespaced schema 注册，但不能把第三方 KiCad/solver 私有类型直接设为核心类型。

### 5.3 `jingwei-jdp`

实现总任务书的请求/响应/event 信封，至少覆盖：

```text
project.create/get
design.compile_jdl
design.query
design.explain
design.patch.propose
design.patch.apply
design.diff
design.validate
edit_session.open
edit_command.preview/commit
edit.undo/redo/checkpoint/publish
task.get/cancel/events
artifact.get
workspace.capabilities
```

Phase 0 可以为未接入的领域命令返回 typed `capability_unavailable`，但协议、权限、版本和错误形状必须稳定。

### 5.4 `jingwei-store`

必须实现：

- workspace metadata；
- 单写锁和 instance/owner lease；
- operation journal；
- immutable revision metadata；
- content-addressed artifact store；
- temp transaction directory；
- crash recovery/reconciliation；
- migration backup 和失败只读模式。

禁止使用原地覆盖 current snapshot 作为提交。推荐流程：prepare → validate → fsync/commit metadata → move pointer。

### 5.5 `jingwei-kernel`

职责：

- transaction service；
- revision/branch/diff；
- precondition/postcondition；
- idempotency ledger；
- capability/permission input；
- dependency invalidation；
- tool/validator/view/projection registry；
- draft edit session、lease、inverse/reapply operation；
- task/cancel state machine。

只有 kernel transaction service 可以创建已提交 revision 或移动 project pointer。

### 5.6 `jingwei-daemon`

职责：

- 每工作区唯一实例/单写；
- 用户级本地 endpoint；
- handshake：protocol/build/workspace/capabilities/read-write；
- request ID、deadline、cancel；
- JDP dispatch 和 event stream；
- client 断线不自动取消 durable task；
- OS ACL/会话 token 边界；
- structured health/readiness。

Windows 首发使用 named pipe；测试可提供 in-memory transport。localhost TCP 默认禁止。会话 token 不出现在命令行、
URL、日志或 lock file。

### 5.7 `jingwei-cli`

至少提供：

```text
jingwei project create
jingwei compile <file.jdl>
jingwei query <selector>
jingwei patch --base <revision> <patch.json>
jingwei diff <left> <right>
jingwei validate <revision>
jingwei events --after <sequence>
jingwei doctor
```

CLI 必须使用 JDP client；不得直接打开 store 写入。

## 6. 协议与 codegen

### 6.1 Rust 是规范源

- Rust 类型生成 JSON Schema/OpenAPI-like catalog 和 TypeScript binding；
- binding 输出稳定、格式化、可重复生成；
- CI 执行 codegen 后必须 clean；
- TypeScript 禁止手写镜像正式类型；
- 协议升级包含 version negotiation、兼容矩阵和 migration note。

### 6.2 Tool catalog

生成给会话 01 的内容至少包括：

- name/description；
- input/output schema；
- permission/effects；
- resource claim template；
- idempotency/cancellation/timeout；
- protocol/schema version。

### 6.3 Display/edit 类型

提供通用：

- `ViewRequest`：revision、viewport、layers、LOD、selection；
- `DisplayPrimitive`：line/polyline/polygon/text/glyph/marker，携带 entity ID；
- `HitTestRequest/Result`；
- `EditCommandGroup`、`SemanticEditCommand` extensible envelope；
- preview/commit/validation delta；
- cross-probe result。

具体 schematic/PCB 几何由会话 03/04 provider 生成。

## 7. 事务不变量

每个写请求：

1. 校验 workspace/client/capability；
2. 校验 base revision 与 current/draft head；
3. 查询 idempotency ledger；
4. 校验 schema、precondition 和 resource claim；
5. 在隔离 transaction 中应用 operation；
6. 调用影响范围内的 validator；
7. 检查 postcondition；
8. 原子写 revision、journal、artifact manifest；
9. 移动允许的 pointer；
10. 发布 durable event。

失败不得移动 pointer。相同 `idempotencyKey + baseRevision + normalizedInput` 返回同一结果；相同 key 不同输入返回
冲突。未知事务不自动重复。

## 8. Draft 编辑模型

实现 Phase 0 最小能力：

- `edit_session_open(baseRevision)` 创建 draft branch 和 lease；
- preview 不写 JIR；
- commit 每个 command group 产生 draft revision；
- undo/redo 通过 inverse/reapply operation 产生新 draft revision；
- checkpoint 持久化；
- publish 从批准 draft 创建 project candidate；
- stale lease/base 返回 typed conflict；
- daemon 重启可恢复未发布 draft，但不污染 project current。

具体 move/connect/route 语义由领域 module 提供。

## 9. Domain 扩展接口

会话 03/04 必须能在不直接修改 store/IPC 的情况下注册：

- entity/property schema；
- domain tools；
- validators；
- semantic edit handlers；
- view/hit-test provider；
- projection backend；
- dependency invalidation rules。

建议 registry 使用显式 builder/layer，不使用运行时任意动态库加载。模块的 `register_*_module()` 返回明确 descriptor
集合；统一验收会话在 daemon composition root 接线。

## 10. 安全要求

- 解析 JDL/JDP/项目文件时设置深度、大小、数量上限；
- canonical path 和工作区边界；
- ZIP/外部文件不在本任务处理，但 store API 不接受任意绝对路径；
- error/log 不回显 secret/token；
- 前端/runtime token 只能授予声明 capability；
- release/waiver 默认没有实现能力，返回明确不可用，不能提供通用 bypass；
- panic 不跨服务边界；外部输入不能造成 daemon 崩溃。

## 11. 不在本会话范围

- provider/Agent loop/MCP；
- 元器件源、symbol/footprint、schematic ERC；
- PCB geometry/placement/routing/DRC/manufacturing；
- React/Tauri 交互工作台；
- 完整 KiCad parser/exporter；
- Production Release、组织签名和真实 DFM。

## 12. 实施步骤

1. 建立 Cargo workspace、toolchain、fmt/clippy/test CI；
2. 先发布 `contract-pack-v0` 小提交：IDs/quantity、JDP 信封、extension traits、空 registry、schema/codegen 命令；
3. 实现 canonical value/hash、JIR graph；
4. 实现 JDL 最小 parser/type checker/formatter；
5. 实现 store、revision、journal、transaction/idempotency；
6. 实现 JDP schema、structured error 和 in-memory transport；
7. 完成 extension registry 和 minimal built-in tools；
8. 实现 daemon named-pipe spike、handshake/events/cancel；
9. 实现 CLI 和 TypeScript binding/tool catalog codegen；
10. 做 crash/property/fuzz/negative tests 和交接包。

## 13. 测试与验收

### 单元/属性测试

- unit conversion 和非法量纲；
- canonical serialization/hash 稳定；
- entity/edge 引用完整性；
- JDL parse/format round-trip；
- operation apply/inverse；
- semantic diff；
- idempotency 等价/冲突；
- schema migration；
- resource claim conflict。

### 集成/故障测试

- 一个 CLI/daemon 纵向切片；
- 第二个 daemon 无法取得同 workspace 写锁；
- stale base 被拒绝；
- 在 prepare/journal/revision/pointer 边界 kill/restart；
- client 断线后 durable task 终态正确；
- cancel 不留下未知 pointer；
- codegen 后无 diff；
- TypeScript fixture 能编译消费生成 binding；
- malformed/oversized input 不 panic。

### 建议命令

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test --workspace --doc
<codegen command> && git diff --exit-code -- bindings/typescript
git diff --check
```

对 parser、migration、operation 建议加入 property/fuzz target；未能长时间运行 fuzz 时记录 seed/corpus 和实际时长。

## 14. 必交制品

- Rust workspace 和七个核心 crate；
- named-pipe/in-memory daemon；
- CLI；
- JSON Schema、tool catalog、TypeScript binding；
- extension registry 示例 module；
- minimal JDL/JIR project fixture；
- crash recovery/idempotency/schema migration 报告；
- `docs/handoffs/task-02/` 完整交接包；
- 所有跨任务接口提案。

## 15. 会话完成时必须明确报告

- protocol/schema 版本与兼容策略；
- 实际使用的 storage/serialization/IPC 依赖及许可证；
- Windows named pipe 是否真实跑过，还是仅 in-memory；
- 哪些 JDP operation 已实现，哪些返回 capability unavailable；
- codegen 命令和 binding 是否 clean；
- 所有 crash point 的真实测试结果；
- 会话 03/04/01/05 接入所需的具体步骤。
