# 精卫 Agent Phase 0 五会话集成验收

> 日期：2026-08-19
> 分支：`integration/jingwei-phase0`
> 结论：五个任务提交均已物化到独立 Jingwei Git 仓库的集成分支；registry、trace-context、KiCad 10 PCB 和
> Windows 真实 sidecar 四项指定门禁已关闭。Production Release 仍被明确阻止。

## 1. Git 隔离与提交映射

`jingwei-agent` 是独立 Git 仓库，未与 `D:\Projects\CD041_LUBAN` 主目录的 Git 混用。验收与合并在独立
`jingwei-agent-integration` worktree 中进行，原 `feat/jw-05-workbench` 脏工作区未被覆盖。

| 会话 | 正式任务提交 | 集成结果 |
| --- | --- | --- |
| 01 Agent Runtime | `cc607b29` | Runtime、工具结算、审批、事件、MCP 与可观测性进入集成基线 |
| 02 Rust Kernel/Daemon | `73b3fac` | JIR/JDP、单写 daemon、IPC、CLI 与生成 bindings 进入集成基线 |
| 03 器件/库/原理图 | `424142fb` | 逻辑域实现合入，随后注册为 daemon 可执行操作 |
| 04 PCB/制造 | `c28d6043` | 物理域实现合入，随后完成 KiCad 10 实板格式与制造导出门禁 |
| 05 Tauri 工作台 | `094ffa25` | 精卫品牌、Apple 工业风、双主题、模型配置与 mock 边界进入集成基线 |

关键集成提交：

- `a2a9119`：组合生产 registry，daemon 实际发布并执行逻辑域、物理域工具；
- `b70cd5d`：正式 TypeScript binding 通过 named pipe 向 Rust daemon 传播 trace context；
- `4421dc0`：修正 KiCad 10 PCB 投影并通过 pinned CLI DRC/Gerber/Drill 门禁；
- `81d1018`：捆绑真实 Windows Runtime/daemon sidecar，完成安装、监督、刷新与退出清理。
- `af6edeb`：在内容已经由等价提交导入、Task 04 又被后续 KiCad 修复覆盖之后，以不改变当前 tree 的
  ancestry-only merge 记录 01/03/04/05 正式交付提交；至此五个正式任务提交均为集成 HEAD 的祖先。

## 2. 指定门禁结果

| 门禁 | 状态 | 可复核证据 |
| --- | --- | --- |
| 生产 registry | PASS | `crates/jingwei-daemon/tests/domain_registry.rs` 对真实 daemon 创建工程并调用领域工具；共发布 23 个可用操作 |
| 跨运行时 trace-context | PASS | `packages/agent-tools/test/rust-daemon-trace-integration.test.ts` 走正式 binding、Windows named pipe 与真实 Rust handler，校验同 trace child span |
| KiCad PCB | PASS（参考板） | KiCad `10.0.5` pinned；参考板 DRC 为 0 violation/0 unconnected；生成 7 层 Gerber、job 与 PTH/NPTH drill；见 `testdata/physical/reports/kicad-10.0.5-reference-gate.json` |
| Windows sidecar | PASS（进程链路） | 两个 manifest SHA-256、版本化 stdout handshake、Runtime→daemon capabilities、安装目录资源解析、一次重检健康与退出零残留；见 `testdata/sidecars/` |

## 3. 验证摘要

- Agent Runtime：42 项通过、1 项商业 OpenAI live test 因缺少环境变量而明确跳过；构建、类型检查和 Biome
  检查通过。
- Rust daemon：单元、registry、trace-context、vertical slice 共 5 项测试通过；Clippy `-D warnings` 通过。
- Tauri Rust：13 项测试通过；Clippy `-D warnings` 通过。
- React 工作台：14 个测试文件、40 项测试通过；ESLint、TypeScript 与 Vite production build 通过。
- KiCad 参考板：`kicad-cli 10.0.5` DRC 和 fabrication export 通过，工具哈希写入
  `toolchains/kicad-10.0.5-windows.json`。
- Windows NSIS：静默安装返回 0；安装版通过 Computer Use 验证深/浅主题、双 sidecar 健康、模型/API 配置
  缺失原因、mock 数据边界和 Production Release 阻断；应用退出后无残留进程。

## 4. 已实现的边界

Windows 桌面包中的 Runtime sidecar 是真实 Bun 编译二进制，使用 SQLite durable runtime、deny-default policy
和正式生成的 JDP binding；daemon 是真实 Rust release 二进制。bootstrap token 仅通过 stdin 传递，二进制在
启动前进行受控 manifest SHA-256 校验，协议、版本与 target 在 handshake 中再次校验。daemon 先启动，Runtime
随后连接 daemon 并实际查询 capabilities。

工作台顶部严格区分两类事实：绿色 runtime/daemon 指示仅表示真实进程链路健康；原理图、PCB、ERC/DRC、
Agent 会话与编辑画布仍标记为 `VERSIONED MOCK · NOT REAL DAEMON EVIDENCE`，不会把 fixture 冒充成真实设计结果。

## 5. 仍未关闭的生产门禁

1. React 工作台尚未把 `MockRuntimeClient` / `MockDesignClient` 替换为真实 sidecar 数据 adapter；真实进程健康
   不等于 UI 数据链路已经接通。
2. 当前环境缺少 `OPENAI_API_KEY` 和 `JINGWEI_OPENAI_MODEL`，商业 OpenAI endpoint live test 未执行；配置 UI
   已明确显示该原因并阻止测试。
3. Runtime→daemon reconciliation 当前为 `false`；崩溃后的跨进程幂等对账仍需实现。
4. 外部自动布线器、真实供应链库存/生命周期、制造商在线 DFM 尚未接入。
5. 当前 NSIS 安装包未签名，也没有完成更新、回滚、SBOM/来源证明和 macOS/Linux sidecar 打包。
6. 参考板的自动检查通过不能提升为量产结论；仍需制造商 DFM、样板、上电/功能/可靠性验证及具名工程师签核。

因此本轮验收结论是 **Foundation integration accepted / Phase 0 product exit blocked / Production Release blocked**。
完整边界审计和后续统一计划见 `../13-authoritative-boundaries-and-lifecycle.md` 与
`../14-design-convergence-audit-and-plan.md`。
