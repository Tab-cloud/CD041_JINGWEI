# 待确认问题与决策日志

> 版本：0.5  
> 日期：2026-08-19  
> 用法：每次需求评审先关闭 P0 问题，再把结论追加到决策日志。未确认项不得伪装为已定需求。

## 1. 当前提案，不是最终决策

以下同时包含本轮已完成的架构决策和仍需产品负责人确认的提案：

| 编号 | 提案 | 状态 |
| --- | --- | --- |
| P-001 | 产品名称 Jingwei-Agent / 精卫 Agent，语义取自“精卫填海” | Accepted |
| P-002 | 复用 Eval 通用 Agent runtime，新增独立 Rust EDA 领域层 | Accepted |
| P-003 | 自研 Jingwei Design Kernel，JIR 为唯一设计真相 | Accepted |
| P-004 | MVP 聚焦低压、非安全关键、2/4 层常规刚性板 | Proposed |
| P-005 | JLCPCB 为首个 manufacturer profile，但架构不绑定单一工厂 | Proposed |
| P-006 | 默认本地处理设计，云端调用逐项目授权 | Proposed |
| P-007 | Freerouting/其他 solver 作为候选后端，结果必须写入 JIR 并交叉验证 | Proposed |
| P-008 | 生产交付必须包含 JDL/JIR 原生工程及 KiCad 兼容 projection，不只交付 Gerber | Accepted |
| P-009 | “生产发布”需要工厂 DFM、样板测试和具名多角色签核 | Proposed |
| P-010 | 首个黄金板为 9-24V 输入的温湿度 + RS-485 采集板 | Proposed |
| P-011 | JDL/JIR/JDP 分别承担声明式语言、规范化 IR 和事务协议 | Accepted |
| P-012 | KiCad 10.0.5 为首个 reference backend，不是核心语言或唯一 ECAD | Accepted |
| P-013 | Rust 为核心实现语言；`jingwei-daemon` 唯一写入；Tauri 2 + React 为首版薄客户端 | Accepted |
| P-014 | AI 生成的原理图和 PCB 必须在原生工作台中可预览、人工编辑、撤销并继续与 AI 协作 | Accepted |
| P-015 | Agent 控制面采用 Eval-derived runtime，参考 Codex/OpenCode；MCP 只做外部集成，固定工程门禁使用确定性 workflow | Accepted |

## 2. P0 产品问题

### Q1. 首发用户是谁？

- 推荐默认：内部硬件工程师/方案团队优先，先做 engineer-in-the-loop；
- 选项：面向无硬件经验终端用户、面向专业工程师、企业内部工具；
- 影响：交互、责任、默认自动化程度、审查和支持成本；
- Owner：产品负责人。

### Q2. MVP 明确支持哪些板型？

- 推荐默认：低压 24 VDC 以下、2/4 层、常规 FR-4、非安全关键控制/传感板；
- 需要明确排除：高压、大功率、RF、DDR/BGA、刚挠、汽车/医疗；
- 影响：规则体系、黄金数据、工程人员和对外承诺；
- Owner：产品 + 硬件负责人。

### Q3. 用户的一句话需求需要自动走多远？

- 推荐默认：自动生成候选，但关键器件确认和每个发布级别由人签核；
- 选项：全自动草稿、逐阶段确认、仅专业工程师模式；
- 影响：体验速度与错误风险；
- Owner：产品负责人。

### Q4. 什么状态可以在产品文案中称为“可量产”？

- 推荐默认：只对 `production-released` 使用，必须有 DFM、样板和签核；
- 影响：法律责任、用户信任和 UI 术语；
- Owner：产品 + 质量 + 法务。

### Q5. 首个真实黄金板需求是什么？

- 推荐默认：本文温湿度/RS-485 板；
- 需要确认：输入范围、MCU/国产化偏好、传感精度、尺寸、数量、成本和目标工厂；
- 影响：Phase 0/1 所有验收；
- Owner：硬件负责人。

### Q6. Jingwei 与 KiCad 的关系是什么？

- 决策：Jingwei 原生 JIR 是唯一设计真相；KiCad 10.0.5 是首个 reference backend；
- 首期：JIR ↔ KiCad、外部人工编辑回写、ERC/DRC 和制造输出交叉验证；
- 后续：可增加嘉立创 EDA/Altium/IPC-2581 adapter，不改变核心 JIR；
- 状态：Closed by DEC-2026-006；
- Owner：技术负责人。

### Q7. 与 Eval 的产品/代码关系是什么？

- 决策：抽取共享 runtime packages，Jingwei 有独立品牌、配置、JDP tools 和发布包；禁止复制 fork 后双线演进；
- 实现：TypeScript/Bun runtime 作为控制面，Rust `jingwei-daemon` 作为 EDA 数据面；
- 状态：Closed by DEC-2026-010；
- Owner：架构负责人。

### Q8. 数据默认是否允许发送到云端？

- 推荐默认：ECAD/验证本地；LLM/供应/API 按项目授权并显示外发内容；
- 影响：模型能力、隐私、企业客户、成本和离线模式；
- Owner：产品 + 安全。

### Q9. 首发制造商和装配厂有哪些？

- 推荐默认：JLCPCB/JLC Assembly 第一家，再增加一家不同规则工厂；
- 需要确认：是否也支持 PCBWay、Seeed、企业自有工厂；
- 影响：profile、BOM/CPL、DFM 和真实验收；
- Owner：制造负责人。

### Q10. 首发器件供应商和地区？

- 推荐默认：中国采购以 LCSC/JLC 料号为主，DigiKey/Mouser 做全球参数/备选；
- 需要确认：API 账户、商业条款、目标币种、库存 TTL；
- 影响：选型、成本、数据许可和可用性；
- Owner：供应链负责人。

### Q11. 自动生成的库由谁批准？

- 推荐默认：项目级硬件/PCB 双人检查；提升组织共享库再做一次 librarian 审查；
- 影响：速度与严重逃逸风险；
- Owner：硬件/PCB 负责人。

### Q12. Production Release 需要哪些角色？

- 推荐默认：硬件、PCB/制造、测试、供应链；低风险小团队可由授权人员兼任但身份仍分角色记录；
- 影响：权限模型和用户流程；
- Owner：质量负责人。

### Q13. 是否允许 Agent 自动创建 warning waiver？

- 推荐默认：不允许。Agent 只能提出带证据的 waiver draft；
- 影响：安全与自动化完成率；
- Owner：质量 + 硬件负责人。

### Q14. 首发是否真实下单和制作样板？

- 推荐默认：必须；没有样板就只能完成到 Manufacturing Candidate；
- 影响：预算、周期和量产声明；
- Owner：项目负责人。

### Q15. 商业发布是否分发 Freerouting？

- 推荐默认：Phase 0 法务评审后决定；可先作为独立可选依赖；
- 影响：GPL 合规、安装体验和自动布线可用性；
- Owner：法务 + 技术负责人。

## 3. P1 工程问题

| 编号 | 问题 | 推荐默认 | Owner |
| --- | --- | --- | --- |
| Q16 | 首发支持 2 层还是同时 4 层？ | MVP 验收 2 层，架构支持 4 层，Alpha 实验 4 层 | PCB |
| Q17 | 最小被动封装？ | 0603 默认，0402 需要 profile 和装配确认 | 制造 |
| Q18 | 是否支持双面 SMT？ | MVP 单面，Alpha 双面 | 制造 |
| Q19 | 是否支持用户已有 KiCad 工程增量修改？ | MVP 读/分析，Phase 1 后半实现安全增量编辑 | 技术 |
| Q20 | 是否需要企业私有器件库/AVL？ | 架构必须支持，首发可用本地目录导入 | 供应链 |
| Q21 | 是否支持国产替代优先策略？ | 作为显式评分/profile，不硬编码品牌 | 产品/供应链 |
| Q22 | 成本目标如何计算？ | 指定数量阶梯、币种、时间和不含/包含项 | 财务/供应链 |
| Q23 | 库 3D 模型是否 P0 必选？ | 连接器/机械关键器件必选，其余可 warning | 机械/PCB |
| Q24 | 是否支持 DXF/STEP 外壳约束？ | DXF 板框 P0，STEP 干涉 P1 | 机械 |
| Q25 | 原理图仿真是否进入 MVP？ | 不做通用 SPICE；只做选定电源/模拟规则与计算 | 硬件 |
| Q26 | 自动布线必须达到什么质量？ | 以黄金板人工基线定义，不用单一“100% routed” | PCB |
| Q27 | 需要哪些 DFT 能力？ | 电源/接口/关键网测试点和最低覆盖 P0 | 测试 |
| Q28 | 是否输出 ODB++/IPC-2581？ | Gerber 为 P0，IPC-2581C P1 并行，ODB++ 可选 | 制造 |
| Q29 | 是否支持拼板？ | MVP 交由工厂；后续按工厂 profile 支持 | 制造 |
| Q30 | 项目是否允许完全离线？ | 设计/验证/导出可离线；在线选型使用冻结快照 | 安全/产品 |

## 4. P2 扩展问题

- 何时支持高压/隔离，采用哪些安规与 creepage/clearance 求解器？
- 何时支持受控阻抗，如何取得板厂实际叠层与 field solver？
- 何时支持 RF/天线、SI/PI、热仿真和 EMC 预检查？
- 何时支持 BGA/HDI/盲埋孔、刚挠和双面高密度装配？
- 是否与固件 pinmap/设备树/配置代码形成硬件-软件协同生成？
- 是否与 PLM、ERP、采购、MES、测试台和序列号系统集成？
- 是否支持组织级设计块和经过验证的参考电路库？
- 是否需要云端协作、网页评审或 API 服务形态？

这些问题不应阻塞 MVP，但架构不得把扩展路径写死。

## 5. 决策记录模板

```markdown
### DEC-YYYY-NNN：标题

- 日期：YYYY-MM-DD
- 状态：Proposed / Accepted / Superseded / Rejected
- 决策人：
- 背景：
- 约束：
- 选择：
- 备选：
- 选择理由：
- 负面影响：
- 需要重新评审的触发条件：
- 关联需求/代码/测试：
```

## 6. 初始决策日志

### DEC-2026-001：生产状态必须独立于文件生成

- 日期：2026-08-19
- 状态：Proposed
- 背景：产品目标包含“生成可以用于量产的 Gerber”。
- 选择：将 Draft、ECAD Candidate、Engineering Verified、Manufacturing Candidate、Prototype Verified、
  Production Released 分级；只有最后一级可称生产发布。
- 理由：ERC/DRC/Gerber 不能替代 DFM、样板、测试和签核。
- 重新评审触发：质量体系或产品责任模型变化。

### DEC-2026-002：以 KiCad 10.0.5 建立首个锁定基线

- 日期：2026-08-19
- 状态：Superseded by DEC-2026-006
- 背景：需要可脚本化、可验证、可交付源工程的 ECAD。
- 选择：原计划以 KiCad 10.0.5 验证文件 AST、CLI、IPC 和制造输出；现收窄为 reference backend 基线。
- 理由：公开文本格式、CLI/ERC/DRC/制造输出和 Windows 支持。
- 负面影响：多 ECAD 用户首发需要转换或等待适配器。
- 重新评审触发：Phase 0 round-trip/自动化 blocker，或商业需求强制其他 ECAD。

### DEC-2026-003：复用 Eval 轻量运行时

- 日期：2026-08-19
- 状态：Accepted；由 DEC-2026-010 细化
- 背景：Eval 已有会话、工具、权限、队列、持久化、取消和 artifact 能力。
- 选择：复用通用 runtime，Jingwei 新增 EDA domain 与确定性服务。
- 理由：减少重复状态机，保持主 Agent 所有权和结构化失败恢复。
- 负面影响：需要明确共享 package、品牌和发布边界。
- 重新评审触发：依赖耦合阻碍独立发布或 runtime 不满足 EDA 事务需求。

### DEC-2026-004：自动布线只生成候选

- 日期：2026-08-19
- 状态：Proposed
- 背景：外部自动布线成功不代表电气/制造质量。
- 选择：所有结果导回 KiCad，并经过全量 DRC、自定义 PCB 检查和人工关键区域审查。
- 理由：把路由器限制在候选生成职责。
- 重新评审触发：未来自研路由器具备形式化约束证明和独立验证。

### DEC-2026-005：默认本地优先

- 日期：2026-08-19
- 状态：Proposed
- 背景：原理图、PCB 和 BOM 通常属于敏感知识产权。
- 选择：ECAD、验证和导出默认本地；外部模型/API/DFM 单独授权并审计外发内容。
- 理由：满足企业隐私和离线需求。
- 负面影响：本地安装包和工具链管理更复杂。
- 重新评审触发：部署形态或客户数据政策改变。

### DEC-2026-006：采用 AI 原生 Jingwei Design Kernel

- 日期：2026-08-19
- 状态：Accepted
- 决策人：Jingwei 项目架构评估
- 背景：硬件设计应最大限度适配 AI 接口与语言，可自行开发原生 EDA 架构。
- 选择：自研 JDL、JIR、JDP 和 headless Design Kernel；JIR 是唯一设计真相；Agent、GUI、CLI 和 importer
  统一通过 JDP transaction；KiCad 10.0.5 作为首个 reference backend。
- 备选：KiCad GUI 外挂、直接生成 KiCad 文件、直接采用 tscircuit/atopile、全栈从零重写。
- 选择理由：兼顾 AI 语义接口、可查询事务、长期产品控制力和首期生产安全。
- 负面影响：Phase 0/1 增加编译器、schema、原生编辑工作台和等价性验证投入。
- 自研边界：自研语义内核/语言/协议/工作台；成熟几何、格式、router、library 和 reference backend 优先复用。
- 重新评审触发：JIR 无法表达黄金板、JIR↔KiCad 无法达成等价、性能或总成本超出阶段门槛。
- 关联文档：`09-ai-native-eda-architecture.md`。

### DEC-2026-007：JDL 默认声明式，不以任意代码作为生产真相

- 日期：2026-08-19
- 状态：Accepted
- 背景：TypeScript/Python 对 AI 友好，但任意执行带来安全、可重现和静态分析风险。
- 选择：JDL 使用声明式模块、接口、单位、公差、assertion 和约束；高级 SDK 可生成 JDL/JDP，但在沙箱中
  运行，输出仍需编译和验证。
- 理由：保留 AI/code-first 效率，同时让生产设计可确定性检查。
- 重新评审触发：沙箱和可重现构建能够对更强可编程语言提供等价保证。

### DEC-2026-008：Rust-first 内核与 Tauri 2 薄客户端

- 日期：2026-08-19
- 状态：Accepted
- 决策人：Jingwei 项目架构评估
- 背景：工具需要 AI 原生 EDA 能力、确定性高性能内核和跨平台桌面交付，且用户推荐 Rust/Tauri。
- 选择：JDL/JIR/JDP、设计事务、器件/库、几何、ERC/DRC、布局布线 adapter、格式转换和制造工具统一
  使用 Rust；`jingwei-daemon` 是工作区唯一状态所有者；Tauri 2 + React/TypeScript 作为首版工作台，前端只
  保存视图状态并通过认证 JDP 调用 daemon。
- 传输：Windows named pipe，macOS/Linux Unix domain socket；Phase 0 采用版本化长度前缀 JSON，Rust 类型
  生成 JSON Schema 和 TypeScript binding，大对象使用 artifact reference。
- 平台顺序：Windows 首发；macOS Apple Silicon 和 Ubuntu LTS 在 Phase 2 建立独立安装、签名和图形门禁。
- 备选：Slint/Rust 原生客户端或 `wgpu` 画布、Flutter、Electron、Qt Quick。
- 选择理由：核心逻辑保持单语言和内存安全；复用现有 Tauri/React 经验；sidecar 隔离唯一状态所有者；稳定
  JDP 使 UI 框架未来可替换。
- 负面影响：增加 daemon 生命周期、本地认证、IPC 兼容和多目标 sidecar 打包复杂度；系统 WebView 的 CAD
  性能与一致性必须实测。
- 重新评审触发：目标硬件画布性能、WebView 图形正确性、原生输入/辅助功能或企业部署出现不可规避阻断。
- 不随 UI 复审改变的部分：Rust 核心、JIR 唯一真相、daemon 单写入者和统一 JDP 事务路径。
- 关联文档：`10-rust-tauri-platform-architecture.md`。

### DEC-2026-009：原理图和 PCB 是可编辑语义画布

- 日期：2026-08-19
- 状态：Accepted
- 决策人：产品需求确认 + Jingwei 项目架构评估
- 背景：AI 生成原理图/PCB 后，用户需要预览并进行人工修订，而不是只能接受静态结果。
- 选择：首版原生工作台同时提供 schematic editor 和 PCB editor；人工编辑与 AI patch 使用同一 JDP 事务、
  revision、provenance、锁、undo/redo 和增量验证机制。
- P0：原理图支持 symbol、连接/线/标签、属性和层次基础编辑；PCB 支持 footprint、板框/区域、track/via、
  zone、丝印和普通约束基础编辑；视图间通过稳定 JIR ID cross-probe。
- 人机协作：人工对象可设 `ai-editable`、`ask-before-change`、`locked`；AI 建议按对象/网络/模块/区域/hunk
  接受或拒绝，默认不覆盖人工保护内容。
- 架构约束：前端负责交互与 transient preview；Rust daemon 负责 hit-test/snap 的规范结果、连通性、规则、
  semantic command commit 和 revision。
- 负面影响：Phase 1 范围显著增加，需要独立 CAD 交互、性能和可恢复编辑投入。
- 不接受的替代：只读 viewer、PNG/PDF/SVG 预览、仅导出 KiCad 后编辑、通过鼠标自动化把 KiCad 当主编辑器。
- 关联文档：`11-native-workbench-preview-and-editing.md`。

### DEC-2026-010：Agent 控制面复用成熟 runtime，不整体 fork 通用框架

- 日期：2026-08-19
- 状态：Accepted
- 决策人：产品方向确认 + Jingwei 项目架构评估
- 背景：Jingwei 需要商业可落地的会话、工具、审批、恢复、多 Agent、MCP 和观测能力；用户明确要求参考
  Codex、OpenCode 或其他成熟开源 Agent 源码，避免从零构造未经生产验证的通用框架。
- 选择：
  1. 直接复用并抽取 Eval 的 TypeScript/Bun runtime 作为 `jingwei-agent-runtime` 控制面；
  2. Codex App Server/SDK 用作 thread/resume/approval/event 产品协议参考；
  3. OpenCode V2 用作 durable input、steer/queue、per-session coordinator、event replay 和单一 tool
     settlement 的源码参考；
  4. Rust `jingwei-daemon` 仍是 JIR/JDP 唯一写入者和领域最终授权者；
  5. Main Agent 保持唯一会话 owner，专业 Agent 作为有预算/schema 的工具；固定发布流程由确定性 workflow；
  6. MCP 只用于外部集成，metadata/output 不可信；OpenTelemetry 作为跨进程观测协议；
  7. LangGraph、Microsoft Agent Framework 和 Temporal 不进入首版桌面核心，保留为后续云端/企业 adapter。
- 首个 provider：建议优先实现 OpenAI Responses API，但通过能力声明 adapter 接入，不绑定单一供应商或
  fault domain。
- 语言边界：Rust-first 指 EDA 可信计算基和 JDP 合同；Phase 0 不重写已有成熟 provider streaming、MCP、
  approval 和 session store。是否迁移 runtime 到 Rust 必须由实测 blocker 触发。
- 备选：整体 fork Codex、整体迁入 OpenCode、以 LangGraph/Agent Framework 重写、从零 Rust Agent runtime。
- 选择理由：最大化复用已经存在且可测试的商业化通用能力，同时让最危险的设计写入、验证和发布继续由
  Rust 确定性内核控制；避免第二套 session/tool/JIR owner。
- 负面影响：安装包包含 Agent runtime 与 daemon 两个 sidecar；需要共享 package 治理、跨进程 trace、协议
  兼容和 TypeScript/Rust 双栈维护。
- 安全边界：模型可见性不等于权限；ToolExecution/approval 先持久化；runtime allow 不覆盖 daemon deny；
  MCP annotation 不作为权限事实；Production Release 始终需要领域门禁和具名签核。
- 重新评审触发：共享 runtime 无法独立发布、Bun/TypeScript 形成不可接受的安全/性能/部署 blocker、需要
  多主机数日工作流，或客户强制 Azure/私有 durable workflow 基础设施。
- 关联文档：`12-agent-runtime-reference-architecture.md`。

## 7. 建议第一次需求评审议程

建议 90 分钟，参与者包括产品、硬件、PCB/制造、Agent/runtime 和供应链：

1. 10 分钟：确认产品用户和“生产发布”定义；
2. 15 分钟：确认 MVP 支持/禁止板型；
3. 20 分钟：确认黄金板完整需求；
4. 15 分钟：确认 JDL/JIR/JDP 命名、Rust/Tauri 实现基线、Eval runtime 抽包与本地/云边界；
5. 10 分钟：确认工厂、供应商和库策略；
6. 10 分钟：确认签核角色和真实样板预算；
7. 10 分钟：分配 Phase 0 spike owner、截止时间和验收证据。

评审后更新所有 Proposed 项，并为每个结论新增 DEC 记录。P-002、P-003、P-008、P-011、P-012、P-013、
P-014、P-015 已由本轮架构评估关闭。
