# Task 02 交接索引

本目录交付 Jingwei Phase 0 的 Rust 设计内核、JDP 本地协议、单写存储、daemon、CLI 和 TypeScript binding。Rust 类型是协议与生成物的唯一规范源。

## 制品导航

| 制品 | 位置 | 用途 |
| --- | --- | --- |
| JDL | `crates/jingwei-jdl` | 有界声明语言、诊断、formatter、JIR 编译 |
| JIR | `crates/jingwei-jir` | 开放图模型、稳定 ID、物理量、canonical hash、迁移 |
| JDP | `crates/jingwei-jdp` | 信封、operation 类型、tool catalog、codegen、client trait |
| Store | `crates/jingwei-store` | 单写锁、不可变 revision、journal、artifact、事务恢复 |
| Kernel | `crates/jingwei-kernel` | transaction、patch、draft、验证与扩展 registry |
| Daemon | `crates/jingwei-daemon` | handshake、dispatch、health、in-memory/Windows named pipe |
| CLI | `crates/jingwei-cli` | 仅经 JDP client 调用 daemon 的命令行入口 |
| Binding | `bindings/typescript` | 由 Rust 生成的 TypeScript、JSON Schema、tool catalog |
| Fixture | `testdata/kernel` | 最小 JDL、错误 JDL、原子 patch |

## 快速验证

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --no-fail-fast
cargo run -p jingwei-jdp --bin jingwei-codegen -- bindings/typescript
npx --yes --package typescript@5.9.2 tsc --project bindings/typescript/tsconfig.json --noEmit
git diff --check
```

生成文件不得手工修改。仓库内 `scripts/codegen-typescript.ps1` 会重新生成并检查已跟踪 binding 是否出现 diff。

## 交接文档

- `acceptance.md`：实际验收结果、已实现与不可用 operation、剩余边界。
- `protocol-and-compatibility.md`：版本、信封、兼容与跨会话接入步骤。
- `storage-and-recovery.md`：目录布局、事务不变量、crash recovery。
- `dependencies-and-licenses.md`：直接依赖、实际 lock 版本、许可证与用途。
- `../../interface-change-proposals/task-02-contract-pack-v0.md`：供会话 01/03/04/05 评审的接口提案。

## 运行边界

- daemon 是工作区唯一 JIR 写入者；CLI、GUI、Agent runtime 和领域 module 不得直接写 store。
- 本交付没有 provider/Agent loop、KiCad、schematic/PCB 领域实现、Production Release 或通用 waiver bypass。
- `task.get` 与 `task.cancel` 的信封和 catalog 已稳定，但当前显式返回 `capability_unavailable`；`task.events` 当前重放项目聚合的持久 revision 事件。
- tool catalog 的 Phase 0 input/output schema 是稳定 object envelope；具体 operation Rust 类型已经生成到 TypeScript，后续若要把每项 catalog schema 精化，需走接口变更提案。
