# Jingwei-Agent 开发文档

> 文档基线：2026-08-19  
> 状态：设计基线 v1.0，五会话 foundation 已收拢；完整 Phase 0 product exit 仍在推进
> 产品中文名：精卫 Agent（取自“精卫填海”）
> 目标：从自然语言需求生成可审查、可复现、可制造的电子设计工程与制造包。

## 1. 当前结论

Jingwei-Agent 不是“画图机器人”或传统 EDA 的鼠标自动化外挂，而是以 AI 为首要使用者、同时支持工程师
可视化编辑的原生电子设计系统。用户可以从一句需求开始，但系统必须逐步形成需求合同、器件决策、库来源、
原理图、PCB、验证证据和制造发布记录。

核心架构采用：

- `JDL`（Jingwei Design Language）：面向 AI 与工程师的声明式设计语言；
- `JIR`（Jingwei Intermediate Representation）：唯一规范化设计图与 revision 真相；
- `JDP`（Jingwei Design Protocol）：查询、事务修改、验证和求解器调用协议；
- Rust 实现的 Jingwei 原生 headless 设计内核、约束系统、ERC/DRC/DFM、adapter 与制造工具；
- `jingwei-daemon` 作为每个工作区唯一 JIR/JDP 写入者，Agent、GUI、CLI 通过认证的本地协议访问；
- 复用 Eval Agent runtime 作为控制面，参考 Codex App Server/SDK 和 OpenCode V2 的线程、审批、事件、
  工具与恢复设计；不整体 fork 通用 Agent 框架，也不让其拥有 JIR；
- Tauri 2 + React/TypeScript 作为首版跨平台工作台，前端只负责展示和交互，不拥有设计真相；
- AI 生成的原理图和 PCB 在工作台中是可预览、可选择、可人工编辑、可撤销并可继续交给 AI 修改的候选 revision；
- KiCad 作为首期成熟格式桥、工程师兼容工具和独立交叉验证后端，不作为 Jingwei 的核心语言。

首个可交付版本建议聚焦：

- 低压直流、非安全关键产品；
- 2 层或 4 层、常规 FR-4、常见 SMT/THT 工艺；
- MCU/SoC 模组、传感器、通信接口、基础电源和普通控制电路；
- Jingwei 原生工程、KiCad 10.x 兼容工程、Gerber/钻孔、BOM、CPL/PnP、装配与制造说明；
- 以 JLCPCB 类常规打样/装配厂能力为首个制造商配置模板，但核心模型不绑定单一工厂。

“可量产”不能由 Agent 自行宣告。自动检查全部通过后，仍要经过工程师签核、制造商 DFM、样板制造、
上电/功能/可靠性验证，才可以提升为生产发布版本。

## 2. 文档索引

| 文档 | 用途 | 当前状态 |
| --- | --- | --- |
| [00-product-requirements.md](docs/00-product-requirements.md) | 产品范围、用户流程、功能与非功能需求 | 待评审 |
| [01-system-architecture.md](docs/01-system-architecture.md) | 系统分层、运行时复用、设计流水线和服务边界 | 已收拢，受 13 号规范约束 |
| [02-domain-model-and-artifacts.md](docs/02-domain-model-and-artifacts.md) | 需求、器件、库、网络、规则、版本与制品模型 | 已收拢，规范类型以 Rust schema 为准 |
| [03-eda-toolchain-and-integrations.md](docs/03-eda-toolchain-and-integrations.md) | KiCad、自动布线、供应链、库与制造商适配 | 待技术验证 |
| [04-tool-contracts.md](docs/04-tool-contracts.md) | Agent 工具列表、输入输出、副作用与权限 | 待技术验证 |
| [05-validation-and-production-gates.md](docs/05-validation-and-production-gates.md) | ERC/DRC/DFM、样板和量产发布门禁 | 待硬件评审 |
| [06-security-compliance-and-audit.md](docs/06-security-compliance-and-audit.md) | 安全、许可证、审计、合规与风险边界 | 待法务/安全评审 |
| [07-roadmap-and-acceptance.md](docs/07-roadmap-and-acceptance.md) | 开发阶段、纵向切片与验收场景 | 待排期 |
| [08-open-questions-and-decisions.md](docs/08-open-questions-and-decisions.md) | 需要产品负责人确认的问题和决策日志 | 持续更新 |
| [09-ai-native-eda-architecture.md](docs/09-ai-native-eda-architecture.md) | AI 原生设计内核、语言、IR、协议与自研边界 | 架构基线 |
| [10-rust-tauri-platform-architecture.md](docs/10-rust-tauri-platform-architecture.md) | Rust workspace、daemon、JDP 本地 IPC、Tauri 与跨平台打包决策 | 实现基线 |
| [11-native-workbench-preview-and-editing.md](docs/11-native-workbench-preview-and-editing.md) | 原理图/PCB 交互预览、人工编辑、人机协作、事务和验收 | 产品与交互基线 |
| [12-agent-runtime-reference-architecture.md](docs/12-agent-runtime-reference-architecture.md) | Eval/Codex/OpenCode 参考架构、成熟框架选型、会话/工具/审批/恢复与实施门禁 | Agent 控制面实现基线 |
| [13-authoritative-boundaries-and-lifecycle.md](docs/13-authoritative-boundaries-and-lifecycle.md) | 状态所有权、身份、事务、sidecar 生命周期、revision/成熟度/release 和兼容规则 | 最高优先级架构规范 |
| [14-design-convergence-audit-and-plan.md](docs/14-design-convergence-audit-and-plan.md) | 全局缺陷审计、Phase 0 状态矩阵与后续统一纵向实施顺序 | 当前执行计划 |
| [integration-2026-08-19.md](docs/handoffs/integration-2026-08-19.md) | 五会话汇总、提交映射和基础集成证据 | Foundation integration 历史基线 |
| [slice-1-real-local-transport-2026-08-20.md](docs/handoffs/slice-1-real-local-transport-2026-08-20.md) | Runtime/daemon 正式本地 RPC、真实 UI adapter 与已安装应用验收 | Slice 1 已通过 |

### 五会话开发任务（已归档）

总任务书先冻结共同协议、文件所有权、依赖和统一合并顺序；五个会话必须使用独立 Git branch/worktree：

| 会话 | 任务文档 | 功能域 |
| --- | --- | --- |
| 01 | [Agent Runtime 控制面](docs/tasks/01-agent-runtime-control-plane.md) | session/run/provider/tool/approval/event/MCP/OTel |
| 02 | [Rust 语义内核与 daemon](docs/tasks/02-rust-jir-jdp-kernel-daemon.md) | JDL/JIR/JDP/revision/store/IPC/CLI/bindings |
| 03 | [器件、库与原理图](docs/tasks/03-component-library-schematic.md) | DesignIntent/MPN/library/schematic/ERC |
| 04 | [PCB、验证与制造](docs/tasks/04-pcb-validation-manufacturing.md) | geometry/placement/routing/DRC/KiCad/manufacturing |
| 05 | [Tauri 原生工作台](docs/tasks/05-tauri-native-workbench.md) | Agent UI、语义画布、人工编辑、diff/recovery |

共同接口、并行规则、交接包、集成测试和统一验收见
[五会话并行开发总任务书](docs/tasks/00-five-session-execution-plan.md)。

这些任务文档与 handoff 现在只作为历史输入。后续不再等待独立任务会话，所有架构、实现、测试和发布门禁
统一按 [全局收拢计划](docs/14-design-convergence-audit-and-plan.md) 在当前集成主线推进。

### 当前跨运行时追踪门禁

`runtime → JDP → Rust daemon` 跨运行时 trace-context 接收门禁已经解除：正式 Rust schema 生成的
`bindings/typescript/**` 被 Runtime adapter 直接消费，TypeScript/Bun 通过 Windows named pipe 向真实
`jingwei-daemon` 发送 W3C trace context，daemon 校验 trace/span ID 并记录同 trace 的 child span。集成测试
`packages/agent-tools/test/rust-daemon-trace-integration.test.ts` 覆盖了该真实跨进程路径；Runtime OTLP 导出的
脱敏也由独立 wire test 覆盖。

这不代表全部生产可观测性已经完成：collector 部署、高可用采集、长期留存与操作告警仍属于后续运维门禁。
商业 OpenAI live test 也因当前环境缺少 `OPENAI_API_KEY` 和 `JINGWEI_OPENAI_MODEL` 而明确跳过。

## 3. 文档权威顺序

1. 已签核的产品需求、风险/发布政策和组织权限；
2. [权威边界与生命周期规范](docs/13-authoritative-boundaries-and-lifecycle.md)；
3. Rust 生成的 JIR/JDP schema、Runtime protocol 与持久化 migration；
4. 制造商针对具体订单确认的能力、当前 revision 和确定性验证证据；
5. 专题架构、工具合同、handoff 和模型解释。

当模型判断与数据手册、封装图、制造商规则或确定性检查冲突时，模型判断必须让位并把冲突暴露给用户。

## 4. 与 Eval Agent 的关系

Jingwei-Agent 复用 Eval 已有的 Tauri/React 桌面经验、主 Agent 会话所有权、工具权限、队列、持久化、取消、
结构化失败恢复和 revisioned artifact 能力。Jingwei 新增 Rust AI 原生 EDA 设计内核、领域模型、编译器和
确定性设计工具，不复制一套新的通用 Agent 调度器，也不把 EDA 设计逻辑复制进前端。

Agent runtime 保持为 Eval 派生的 TypeScript/Bun 控制面，Rust `jingwei-daemon` 保持为 EDA 数据面。Codex
App Server/SDK 用作产品内嵌、线程、审批和事件协议参考，OpenCode V2 用作 durable input、steer/queue、
单一工具结算与事件回放的源码参考；LangGraph、Microsoft Agent Framework 和 Temporal 只保留为后续云端/
企业 adapter 候选。详细结论见
[12-agent-runtime-reference-architecture.md](docs/12-agent-runtime-reference-architecture.md)。

Eval 现有 `schematic_read`、`datasheet_read`、`pin_extract` 主要解决“读取和理解已有资料”；Jingwei
需要新增“生成、编辑、验证和发布完整 ECAD 工程”的能力。两者可以共享资料解析和证据存储，但必须区分：

- 视觉/LLM 结果是候选事实或辅助解释；
- JIR revision、规则报告和制造输出是可机器验证的设计事实；KiCad 等外部 ECAD 文件是有 provenance 的投影；
- 生产发布必须由确定性门禁和具名人员签核共同决定。

## 5. 下一步

已完成 [Slice 0：规范类型和迁移收拢](docs/14-design-convergence-audit-and-plan.md)、Slice 1 真实 Runtime/daemon
本地 RPC 与 UI adapter，以及 Slice 2 跨进程 execution reconciliation、取消和崩溃恢复。当前进入黄金板真实
logical/schematic 闭环（Slice 3）。商业模型凭据、Freerouting 分发法务、制造商 DFM 和真实样板仍需要相应外部输入；
这些外部阻塞不会再成为架构边界不清或继续实现其他安全工作项的理由。

## 6. 外部依据

- [KiCad 10.0.5 发布说明](https://www.kicad.org/blog/2026/07/KiCad-10.0.5-Release/)
- [KiCad 命令行接口](https://docs.kicad.org/10.0/en/cli/cli.html)
- [KiCad 文件格式文档](https://dev-docs.kicad.org/en/file-formats/index.html)
- [KiCad IPC API](https://dev-docs.kicad.org/en/apis-and-binding/ipc-api/)
- [Ucamco Gerber 官方资料](https://www.ucamco.com/en/gerber/downloads)
- [IPC-2581C 简介](https://www.ipc.org/news-release/ipc-releases-ipc-2581-revision-c-generic-requirements-printed-board-assembly-products)
- [JLCPCB Gerber 准备要求](https://jlcpcb.com/help/article/gerber-files-preparation)
- [JLCPCB BOM/CPL 准备建议](https://jlcpcb.com/help/article/advice-for-bom-and-cpl-files-preparation)
- [tscircuit：TypeScript/React 电子设计工具链](https://docs.tscircuit.com/)
- [atopile：声明式电子设计语言与编译器](https://github.com/atopile/atopile)
- [JITX：代码化电路设计与约束](https://docs.jitx.com/en/latest/contents.html)
- [Tauri 2：外部二进制/sidecar](https://v2.tauri.app/develop/sidecar/)
- [Tauri 2：Security 与 capabilities](https://v2.tauri.app/security/)
- [Slint：Desktop platforms](https://docs.slint.dev/latest/docs/slint/guide/platforms/desktop/)
- [OpenAI Codex App Server](https://developers.openai.com/codex/app-server)
- [OpenAI Codex SDK](https://developers.openai.com/codex/codex-sdk)
- [OpenAI Agents SDK orchestration](https://developers.openai.com/api/docs/guides/agents/orchestration)
- [OpenCode](https://github.com/anomalyco/opencode)
- [LangGraph](https://docs.langchain.com/oss/python/langgraph/overview)
- [Microsoft Agent Framework](https://learn.microsoft.com/en-us/agent-framework/overview/)
- [Temporal](https://docs.temporal.io/)
- [Model Context Protocol 规范](https://modelcontextprotocol.io/specification/2025-11-25)
- [OpenTelemetry 规范概览](https://opentelemetry.io/docs/specs/otel/overview/)
