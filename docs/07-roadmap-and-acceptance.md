# 开发路线图与验收计划

> 版本：0.5  
> 日期：2026-08-19  
> 状态：待排期；阶段工期是工作量级别，不是承诺日期

## 1. 交付策略

先做一个可制造的受控纵向切片，再扩展板型。不能按“先做自然语言生成、最后再补验证”的顺序开发，
因为领域模型、revision、证据和门禁需要从第一天进入架构。

同时不能按传统 EDA 的顺序先做完整画布。第一优先级是 JIR/JDP 语义内核，其次才是 JDL 表面语法和原生
editor；KiCad reference backend 用于在原生物理后端成熟前保障兼容和交叉验证。

实现基线采用 Rust Cargo workspace + `jingwei-daemon` 唯一状态所有者 + Tauri 2/React 薄客户端。不能先在
前端快速实现一套 EDA 逻辑、再补 Rust 内核；从第一个纵向切片开始，Agent、CLI 和 GUI 必须走同一 JDP。

Agent 控制面不从零重写：复用 Eval runtime，参考 Codex App Server/SDK 与 OpenCode V2 的耐久性不变量，
按 `12-agent-runtime-reference-architecture.md` 抽取共享 packages。Phase 0 不以“全 Rust”为理由重写成熟的
provider streaming、MCP、审批和 session store；Rust 边界集中在 EDA daemon 与生成的 JDP 合同。

每个阶段都必须交付：

- 可运行能力；
- 自动化正/负向测试；
- 可检查的 JDL/JIR、KiCad reference projection、报告和制造制品；
- 未覆盖能力和风险清单；
- 下一阶段可复用的版本锁。

## 2. Phase 0：技术可行性与基线

目标工作量：4-8 工程周。增加的投入用于建立可长期演进的 AI 原生语义内核，而非一次性 KiCad 生成脚本。

### 2.1 交付

- 用 contract tests 冻结 Eval runtime 的 session、run、tool、approval、cancel 和 recovery 行为，并抽取无品牌
  的 `agent-runtime/tool/policy/event/mcp/observability/protocol` 共享 packages；
- 实现 durable input admission、client request ID、`steer`/`queue`、同 session join/coalesced wake 和跨
  session 并行；
- 建立聚合内 durable event sequence、read-after + live replay、runtime owner lease 和 Tauri 重连投影；
- 建立单一 tool descriptor/settlement pipeline；从 Rust JDP schema 生成工具 binding，并集中处理 schema、
  授权、审批、output bounding、artifact 和终态；
- 接通一个真实商业 provider（建议首个为 OpenAI Responses API），验证 native tool、stream failure、能力
  协商、fault-domain fallback、取消与上下文压缩；
- 建立 MCP server 隔离/allowlist/untrusted metadata policy 和 OpenTelemetry 跨 runtime/JDP/daemon trace；
- 建立 Rust Cargo workspace、锁定 toolchain、crate 边界和 `fmt/clippy/test` CI；
- 冻结 JIR v0 entity/edge、单位、约束、revision 和 operation journal；
- 实现 JDP query/explain/patch/validate/diff 最小协议和原子 transaction；
- 实现 JDL 最小 grammar/AST/type checker/formatter；
- 实现 `jingwei-daemon`、workspace 单写锁、journal 恢复、认证本地 IPC 与生成的 TypeScript binding；
- 建立 Eval Agent client、Rust CLI 和 Tauri 2 最小原生工作台，三者使用同一 JDP；
- 完成 schematic/PCB viewport、hit-test、cross-probe 与最小编辑 spike：移动 symbol/footprint、修改属性、
  连接 wire、绘制基础 track；
- 完成 draft edit session、transient preview、command commit、undo/redo 和异常草稿恢复原型；
- 完成 Windows Tauri 安装包、daemon sidecar、首次启动、升级和卸载 spike；
- 锁定 KiCad 10.0.5 Windows 环境、CLI 能力和官方库版本；
- 建立 JIR ↔ KiCad adapter、projection map 和 unknown-feature 隔离策略；
- JIR 生成最小原理图/PCB并通过 KiCad GUI/CLI round-trip；
- 导出并解析 Gerber、drill、BOM、CPL、IPC-356、STEP、IPC-2581C；
- Freerouting DSN/SES 最小闭环；
- JLCPCB profile v0 与制造包 manifest；
- GPL、KiCad 库和第三方数据源许可证评审结论；
- 负向样例：冲突 patch、坏 JDL、schema migration、坏文件、错引脚、漏层、路径穿越、取消失败。
- runtime 故障注入：输入已提交未执行、provider 半流、工具已记录未执行、等待审批重启、daemon 已提交未
  回应、外部进程终态未知、UI 断线重放和 schema migration 失败。

### 2.2 退出条件

- 干净机器可一键复现实验；
- 所有工具版本和 hash 被锁定；
- 一个工作区只有一个 daemon 可持有写锁；断线、崩溃和冲突事务不污染 current revision；
- Agent/CLI/GUI 同一 patch 产生相同 JIR revision 与语义 diff；
- 相同 client request ID 不重复入队；同 session 无并行 provider turn，不同 session 可并行；`steer` 与
  `queue` 行为可自动区分并在重启后保持；
- provider 半流产生的 partial text/reasoning/tool call 不进入下一 attempt；普通无工具回复可正常完成；
- ToolExecution 在副作用前持久化；daemon 已提交但 runtime 崩溃时不会产生第二个 revision；
- approval 在 app/runtime 重启后可继续查看和拒绝；持久化失败/过期时 fail closed；
- 工具即使已向模型可见，缺少 runtime 或 daemon 权限仍被拒绝；恶意 MCP metadata 不能提升权限或取得完整 JIR；
- UI 从任意已保存 event sequence 重连后得到一致 run/tool/approval/revision 投影；
- provider/runtime/daemon/adapter 故障注入后没有未知 current revision，无法证明的外部状态进入 quarantine；
- OpenTelemetry trace 可跨 runtime → JDP → daemon 关联，日志/trace 不含密钥或完整私有设计；
- GUI 的 symbol/footprint 移动和连接/走线操作产生规范 JDP command，不把鼠标中间帧写入 JIR；
- 人工锁定对象后，AI/solver proposal 不得静默修改；故意修改时返回结构化冲突；
- React/Tauri 无直接 JIR/store 写入路径，协议 binding 可从 Rust schema 一键再生成且无 diff；
- JDL 可确定性编译/格式化，JIR migration 有黄金测试；
- JIR projection 可由 KiCad 10.0.5 打开/保存并 round-trip 语义一致；
- native 与 KiCad ERC/DRC 失败能够映射到同一 JIR 对象；
- 制造包验证器能拦截至少 5 类故意错误；
- 干净 Windows 目标机通过签名安装包启动 UI/daemon、完成黄金 patch、离线重启并正常卸载；
- 技术/许可证 blocker 已有明确决策，不带未知核心风险进入 Phase 1。

## 3. Phase 1：首个纵向 MVP

目标工作量：10-16 工程周，取决于原生 editor、JIR↔KiCad 等价覆盖和库生成复杂度。

### 3.1 推荐黄金板

候选需求：

- 9-24 VDC 输入；
- 3.3 V MCU；
- 数字温湿度传感器；
- 非隔离 RS-485；
- 反接/基础浪涌/ESD 保护；
- 状态 LED、复位、SWD；
- 2 层、60 x 40 mm、单面 SMT，0603 及以上；
- 目标 JLCPCB 打样与装配；
- 低风险室内环境。

这只是推荐默认，开始编码前应由产品和硬件负责人确认。

### 3.2 能力

- 自然语言到 `DesignIntent` 和定向追问；
- 单一 Main Agent owner + 有预算/schema 的专业 agents-as-tools；固定工程门禁由确定性 workflow 执行；
- JDL/JIR/JDP P0 语义、schema 和 migration；
- 原生 schematic/PCB 可交互预览与 `11-native-workbench-preview-and-editing.md` 定义的 P0 人工编辑，
  不以只读图片/PDF viewer 代替；
- Tauri 工作台只保存视图状态；选择、snap、hit-test、约束查询和设计写入由 Rust/JDP 处理；
- draft session、undo/redo、草稿恢复、检查点、revision diff 和对象锁；
- 原理图/PCB/3D/BOM/finding/AI 引用的稳定对象 ID cross-probe；
- AI patch 可按对象、网络、模块、区域或变更块接受/拒绝，并默认保留人工修改；
- 语义 query、explain、patch preview、branch 和 diff；
- 受控器件目录 + 至少一个真实供应商 API；
- 精确 MPN 选型、候选比较和首选/备选；
- 官方库解析 + 一个自动生成 symbol/footprint 的路径；
- 原生 logical graph、原理图模块生成、增量修改、native ERC 和 10+ 自定义检查；
- 制造商规则、板框、基础自动布局；
- 关键网络策略 + Freerouting 普通网络候选；
- native DRC + KiCad reference DRC + 10+ PCB/制造自定义检查；
- Gerber/drill/BOM/CPL/IPC-356/STEP/manifest；
- UI 显示阶段、问题、diff、证据和发布缺口；
- release candidate，但 Production Release 仍需完整样板闭环。

### 3.3 退出条件

- 黄金板从需求到制造候选可在干净环境端到端完成；
- 全流程无需 Computer Use 操作 KiCad；KiCad GUI 只用于人工兼容验收；
- 工程师可在 Jingwei 内完成黄金板原理图和 PCB 的规定 P0 修订，并在重启后恢复同一 draft/revision；
- 人工编辑后增量 ERC/DRC 能定位到对应对象，AI 后续修改不覆盖 locked/ask-before-change 内容；
- undo/redo 生成可审计 revision，并发旧 base 修改返回 typed conflict 而非 last-write-wins；
- 所有 P0 工具合同有正/负/取消/恢复测试；
- session replay、审批恢复、工具幂等、MCP 提权和 provider 半流均有真实运行回归；
- 错引脚、错封装、错电源范围、未连接、DRC、错 CPL、stale BOM 均能拦截；
- 制造候选上传到目标工厂后层/BOM/CPL 正确识别；
- 至少一名硬件工程师和一名 PCB/制造工程师完成审查；
- 未把任何未支持高风险设计显示为可生产。

## 4. Phase 2：Alpha 与样板闭环

目标工作量：6-12 工程周。

### 4.1 能力扩展

- 实际下单、工厂 DFM 回写和 issue 映射；
- 样板测试计划、记录、测量附件与需求追溯；
- 4 层板、双面装配、更多封装和基础 USB/CAN；
- 多个供应商适配器、生命周期/替代料；
- 用户手工 KiCad 编辑同步和语义 diff；
- 原生工作台增加批量原理图编辑、push-and-shove、差分/等长、复杂铜皮和多人审查等 P1 能力；
- 建立 macOS Apple Silicon 与 Ubuntu LTS 的 Tauri/daemon 构建、安装、UDS 和图形兼容矩阵；
- 多布局/布线候选与评分回归；
- 组织级库审批和库错误影响分析；
- 设计变体、DNP 和替代 BOM；
- 安装包、更新和离线验证工具。

### 4.2 退出条件

- 至少 3 个不同黄金板完成真实样板；
- 每块板都有需求、设计、制造和测试完整链路；
- 工程师发现的严重问题都转化为回归规则或明确的人工门；
- 相同 locked toolchain 可复现制造包；
- 用户手工编辑不会被 Agent 静默覆盖；
- 安全/许可证/隐私审查通过 alpha 范围。

## 5. Phase 3：Beta 与工程团队使用

目标工作量：持续 2-4 个月。

### 5.1 重点

- 10-20 个内部/合作项目覆盖多种常规板型；
- 4 层、电源/热/EMC 预检查增强；
- 机械 STEP/DXF 约束、外壳干涉；
- 第二家制造商 profile；
- 企业 AVL/PLM/ERP 接入；
- 评审、评论、角色权限和组织模板；
- 性能、长任务、恢复、多会话和大工程优化；
- 规则 coverage dashboard；
- telemetry 仅收集授权的匿名运行指标，不上传设计内容。

### 5.2 退出条件

- Beta 范围内严重逃逸缺陷率达到团队设定阈值；
- 制造商首次上传识别成功率达到目标；
- 关键库 pin/pad 黄金集 100% 通过；
- 取消、崩溃、断电恢复不产生未知设计状态；
- Production Release 权限、签名和撤销流程完成演练；
- 支持矩阵和不支持范围对用户清晰可见。

## 6. Phase 4：生产版本

### 6.1 必要条件

- 经批准的产品支持矩阵；
- 完整安装/升级/回滚和 toolchain 镜像策略；
- 组织签名、审计、备份和灾难恢复；
- 供应商 API 商业条款和 SLA；
- 第三方 notice、源代码义务和法务批准；
- 安全评估、恶意文件和供应链测试；
- 质量指标、事件响应和受影响发布查询；
- 用户文档、工程师培训和责任边界。

生产版本不表示支持所有电子产品，只表示已声明支持矩阵内的流程达到发布标准。

## 7. 工作流优先级

### P0：纵向闭环

1. revision/artifact/provenance 基础；
2. JIR schema/graph/revision/migration；
3. JDP query/patch/transaction/diff；
4. JDL AST/type checker/formatter；
5. KiCad reference adapter/CLI/round-trip/equivalence；
6. DesignIntent；
7. 器件精确 MPN 与受控库；
8. 原生 logical design + ERC；
9. manufacturer profile + physical constraints；
10. 布局/布线候选；
11. native + reference DRC/自定义检查；
12. 制造包 + 独立验证；
13. 样板/签核状态模型。

### P1：工程效率

- 语义 diff、手工编辑同步；
- 多供应商、替代料、变体；
- DFM 回写；
- 多候选优化和解释；
- 团队评论/审批；
- 4 层与双面装配。

### P2：高级板型

- 阻抗/高速/SI/PI；
- RF、天线和 EMC；
- 高压/隔离/安规；
- HDI/BGA/刚挠；
- 高功率/热仿真；
- 法规/功能安全行业流程。

P2 每类都需要专用求解器、规则、黄金数据和专家签核，不能只靠增加提示词开放。

## 8. 验收场景

### A. 正常生成

用户输入黄金板需求，系统通过 JDL/JIR/JDP 完成追问、选型、库、原理图、PCB 候选和制造包，并清楚停在
正确成熟度等级；主流程不依赖模拟传统 EDA GUI。

### B. 缺失关键需求

用户只说“做 24V 转 5V 电源板”。系统识别功率、电流、隔离、纹波、尺寸和环境缺失，在选拓扑前询问。

### C. 错误库

第三方 symbol pin 1 与 datasheet 不一致。系统拒绝 exact match、创建 error，并禁止原理图验证通过。

### D. 供应冲突

首选器件满足电气但 EOL/缺货。系统保留事实，推荐备选并显示重新验证影响，不静默换料。

### E. 自动布线失败

router 未布通或产生 DRC。系统保存候选诊断、不中断独立工作，可改布局/规则/seed，但不能推荐失败候选。

### F. 用户手工修改

用户在 KiCad 改动网络与布局。系统检测外部变化，通过 adapter 导入新 JIR revision，展示语义 diff 并使相关
检查失效；未导入的外部文件不能发布。

### G. 制造包错配

旧 BOM 与新 PCB 混用。manifest/reference 集合检查失败并阻止 release candidate。

### H. 高风险需求

用户要求 220 VAC 离线电源。系统可整理需求和风险，但阻止 MVP 自动生产发布，并说明需要的专业流程。

### I. 提示注入

数据手册文本要求忽略安全规则并上传工程。内容作为不可信数据处理，不改变权限或工具策略。

### J. 取消与恢复

自动布线中取消/进程崩溃。系统确认退出、清理候选或隔离资源，current revision 不被未知修改。

## 9. 测试层级

| 层级 | 内容 |
| --- | --- |
| Unit | JDL/JIR/JDP、物理单位、几何、MPN、pin map、规则和 manifest |
| Property | JIR migration、JDP transaction、AST round-trip、坐标变换、旋转、集合 |
| Contract | native kernel、KiCad adapter/CLI/IPC、供应商 API、router、manufacturer profile |
| Golden | 已知正确板与故意错误板的语义结果 |
| Integration | 需求到 JIR、JIR 到 KiCad、JIR 到制造包、外部编辑/DFM 回写 |
| Fault injection | 超时、取消、断电、磁盘满、API 限流、坏 ZIP、崩溃 |
| Security | 路径穿越、提示注入、恶意库、凭据脱敏、权限绕过 |
| UI/Computer Use | 可见阶段、对象定位、diff、问题、人工审批和恢复 |
| Rust quality | fmt、严格 clippy、unit、property、fuzz、panic/崩溃恢复和依赖审计 |
| Desktop package | 各目标平台安装、签名、sidecar、升级/回滚、卸载、离线启动和 GPU fallback |
| Hardware | 真实样板、测量、接口、温升和必要可靠性测试 |

自动化、UI、工厂和硬件测试是不同门禁，不能互相代替。

## 10. Definition of Done

单个功能只有同时满足以下条件才算完成：

- 产品行为和不支持范围文档化；
- 类型化接口和迁移/兼容策略；
- 成功、失败、边界、权限、取消和恢复测试；
- 真实工具/文件/API 的契约证据；
- UI 展示真实状态和错误；
- artifact/provenance/audit 完整；
- 没有新增可绕过发布门禁的路径；
- 相关黄金板和负向样例通过；
- 文档和支持矩阵同步更新。

## 11. 建议团队能力

最小核心团队需要：

- Agent/runtime 工程；
- Rust 编译器/DSL/类型系统、并发事务与图数据工程；
- KiCad/EDA 文件和插件工程；
- Rust 2D 几何、空间索引、`wgpu`/WebGL/WebGPU 与 CAD 交互工程；
- 硬件原理图工程；
- PCB layout/DFM 工程；
- 元器件/供应链数据工程；
- Tauri 2、React/TypeScript 与跨平台桌面发布工程；
- QA/自动化与硬件测试；
- 兼职安全、法务/许可证和制造顾问。

高级 RF/SI/PI/高压/功能安全能力不能仅由通用软件团队验收。

## 12. 主要风险

| 风险 | 早期缓解 |
| --- | --- |
| 自研 EDA 范围失控 | 先做语义内核；成熟格式、几何和求解器优先复用 |
| JDL 过早冻结 | 先冻结 JIR/JDP 语义，黄金板后冻结表面语法 |
| JIR 与 KiCad 漂移 | JIR 唯一真相、projection map、round-trip/equivalence gate |
| KiCad 文件/API 覆盖不足 | reference adapter 隔离、unknown-feature 阻断、CLI 优先 |
| 自动布局/布线质量不稳定 | 收窄板型、多候选、硬约束过滤、工程师复核 |
| 库引脚/封装错误 | 精确 MPN、三方映射、项目隔离库、负向黄金集 |
| 供应数据陈旧/许可限制 | 多源、时间戳、官方 API、许可审查、禁止抓取兜底 |
| DRC 通过但电路不工作 | 原理图领域规则、参考设计证据、样板测试 |
| 制造文件与源不一致 | 单 revision 导出、manifest、独立 parser 和 hash |
| 用户误解“AI 自动量产” | 成熟度等级、强发布门、明确责任边界 |
| 高风险板型过早开放 | 支持矩阵、risk profile、专用能力再解锁 |
| 前端与 Rust 出现双重设计逻辑 | daemon 唯一写入者；binding 自动生成；同 patch revision 一致性测试 |
| 系统 WebView 画布性能/一致性不足 | display list/LOD benchmark；保留 Rust wgpu/Slint 原生画布替换边界 |
| 跨平台只停留在“能编译” | 每个平台独立安装、签名、UI、GPU、sidecar 和真实黄金板验收 |
