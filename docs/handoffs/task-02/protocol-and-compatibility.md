# JDP/JIR 协议与兼容策略

## 当前版本

- JDP protocol：`0.1`
- JIR schema：`0.1`
- JDL AST：`0.1`
- Rust workspace/package：`0.1.0`

opaque ID 的具体字符串格式不属于协议；消费者只能保存、比较和回传。JIR entity/edge kind 使用 namespaced string，避免核心封闭枚举阻塞领域扩展。JSON 字段采用 camelCase，枚举 wire value 按各类型声明使用 kebab-case。

## Negotiation 与演进

1. client 先通过 handshake 提交支持的 protocol version、workspace ID、client build 和 session token。
2. daemon 选择共同版本并返回 build、schema、capability、read-write 与 transport 状态；无共同版本直接失败，不做猜测性降级。
3. `workspace.capabilities` 是 operation 可用性的运行时真值；catalog 中存在不表示当前 composition root 已接入实现。
4. additive optional field 可在 `0.1` 内兼容演进；删除、改义、required field 或 wire name 变化必须提升 protocol/schema，并提供 migration note。
5. JIR 只沿 `MigrationRegistry` 中显式登记的路径迁移；缺失或歧义路径失败。高于当前 store schema 的工作区只读打开。

## 错误与恢复

失败信封固定包含 `code`、`stage`、`summary`、稳定 `fingerprint`、`retryable`、对象/证据引用和建议动作。secret、session token 与任意输入原文不进入 fingerprint 或日志。stale base、idempotency 冲突、resource claim 冲突、schema 错误和 capability unavailable 均为 typed error。

## 会话接入步骤

### 会话 01：Agent runtime / MCP

1. 消费 `bindings/typescript/schemas/tool-catalog.json` 和 `src/generated.ts`，不要镜像手写正式类型。
2. 先调用 handshake 与 `workspace.capabilities`，只暴露状态为 available 且满足 permission 的 tool。
3. 每次写请求提供唯一 request ID、稳定 idempotency key、显式 base revision 和 deadline。
4. 使用 response 的 `newRevision`、artifact/evidence refs 与 structured error；不要从消息文本推断状态。

### 会话 03/04：schematic/PCB domain module

1. 在自己的 crate 实现 `DomainToolModule`、`ValidationProvider`、`ViewProvider`、`ProjectionBackend`、`SemanticEditHandler` 或 `DependencyInvalidationRule`。
2. entity/edge kind 与 property schema 使用稳定 namespaced 名称。
3. 导出一个明确的 `register_*_module(ExtensionRegistryBuilder) -> Result<ExtensionRegistryBuilder, RegistryError>` 组合函数。
4. 统一 composition root 调用 builder 接线；module 不得引用 `jingwei-store` 或 IPC 实现。
5. semantic edit handler 返回 core `PatchOperation` 和 inverse operation；kernel 负责 precondition、validator、commit、journal 和 pointer。

参考：`crates/jingwei-kernel/examples/extension_module.rs`。

### 会话 05：Tauri/workbench/installer

1. 启动每 workspace 唯一 daemon，通过 stdin 注入 session token；不得放入 argv、URL、日志或 lock file。
2. Windows 使用用户级 named pipe；installer/launcher 负责把 pipe ACL 收紧到当前用户 SID。
3. UI/CLI 只持有 JDP client，不直接打开 `.jingwei` store。
4. 启动后检查 structured health/readiness；protocol/schema 不兼容或只读原因必须显示给用户。

## Codegen

规范命令：

```powershell
cargo run -p jingwei-jdp --bin jingwei-codegen -- bindings/typescript
```

生成内容包括 request/response JSON Schema、tool catalog 和 TypeScript types。CI 应在生成后检查目录 clean，并编译 `bindings/typescript/test/generated-fixture.ts`。
