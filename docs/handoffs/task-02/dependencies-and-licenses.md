# 直接依赖与许可证

以下版本来自本次 `Cargo.lock` 的实际解析结果，而不是仅抄写 version requirement。

| 依赖 | Lock 版本 | 许可证 | 用途 |
| --- | --- | --- | --- |
| `async-trait` | 0.1.92 | MIT OR Apache-2.0 | transport/extension async trait |
| `chrono` | 0.4.45 | MIT OR Apache-2.0 | revision、journal、event 时间 |
| `clap` | 4.6.6 | MIT OR Apache-2.0 | CLI/daemon 参数解析 |
| `fs2` | 0.4.3 | MIT/Apache-2.0 | OS writer file lock |
| `futures-util` | 0.3.34 | MIT OR Apache-2.0 | async panic containment |
| `hex` | 0.4.3 | MIT OR Apache-2.0 | hash/artifact wire encoding |
| `schemars` | 1.2.2 | MIT | Rust-owned JSON Schema |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | JDL/JIR/JDP/store serialization |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 | canonical JSON 与 wire framing |
| `sha2` | 0.10.9 | MIT OR Apache-2.0 | canonical/content-address hash |
| `thiserror` | 2.0.20 | MIT OR Apache-2.0 | typed internal errors |
| `tokio` | 1.53.1 | MIT | runtime、deadline、Windows named pipe |
| `uuid` | 1.24.1 | Apache-2.0 OR MIT | 本地 opaque ID/instance token material |
| `proptest` | 1.11.0 | MIT OR Apache-2.0 | dev-only 属性测试 |
| `tempfile` | 3.27.0 | MIT OR Apache-2.0 | dev-only 隔离 workspace/故障测试 |

Storage 没有引入数据库：持久层为标准文件系统上的 append-only JSON/bytes、临时事务目录、原子 rename 与 `fs2` OS lock。Serialization 是 `serde`/`serde_json`；Windows IPC 是 Tokio named pipe，测试另有同信封的 in-memory transport。localhost TCP 没有实现。

发布前仍应由统一会话运行完整 transitive dependency/license/SBOM 扫描；本表只声明本任务直接依赖。
