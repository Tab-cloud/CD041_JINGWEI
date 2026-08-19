# 安全、合规与审计

> 版本：0.4  
> 日期：2026-08-19  
> 状态：待安全、法务与质量评审

## 1. 安全目标

- 不让不可信资料、库或插件修改超出项目范围的文件和进程；
- 不让模型或普通工具绕过高风险设计、豁免和生产发布权限；
- 不泄露私有原理图、PCB、BOM、数据手册授权内容或凭据；
- 确保 Production Release 可追溯、可验证、不可静默替换；
- 对错误器件数据、恶意库、供应链陈旧数据和工具链漂移保持可检测。
- 确保 JDL、GUI、Agent、SDK 和 importer 都不能绕过 JDP 事务与 JIR revision 权限。
- 确保 Tauri WebView 无法直接获得工作区、shell、sidecar 或发布凭据权限；所有能力经过显式 capability 和 daemon 校验。

## 2. 信任边界

### 2.1 不可信输入

- 用户上传的 KiCad/EDA 工程、ZIP、PDF、图片和脚本；
- 用户/模型生成的 JDL、JDP patch、TypeScript/Python SDK 代码；
- 数据手册、应用笔记、网页文本中的提示注入内容；
- 第三方 symbol、footprint、3D model 和插件；
- 供应商/聚合商 API 响应；
- MCP server instructions、tool description/annotations、resources、prompts 和 tool result；
- Freerouting DSN/SES 和外部 DFM 报告；
- Gerber/ODB++/IPC-2581 等制造文件；
- 从用户已有工程读取的自定义命令或路径。

### 2.2 可信但版本受控

- 签名/哈希锁定的 Jingwei 代码与生成器；
- 版本锁定的 JDL grammar、JIR schema/migration 和 JDP protocol；
- 锁定 KiCad 二进制和官方库快照；
- 经批准、版本化的 manufacturer/risk profile；
- 组织身份、角色和发布签名服务。

可信不代表没有 bug，仍需黄金回归和输出验证。

### 2.3 进程信任边界

| 进程/区域 | 信任级别 | 允许能力 |
| --- | --- | --- |
| React/WebView | 低权限展示层 | JDP client 的最小 query/patch proposal、视图缓存、用户交互 |
| Tauri Rust shell | 受控平台桥 | 窗口、系统对话框、签名更新、受 allowlist 约束的 daemon 生命周期 |
| `jingwei-agent-runtime` | Agent 控制面 | session/run、provider、tool registry、审批、MCP、事件和审计；不得直接写 JIR |
| `jingwei-daemon` | 设计可信计算基 | JIR transaction、workspace lock、规则、任务、审计和 adapter 编排 |
| MCP server | 不可信外部能力 | 独立连接、最小 context、tool allowlist；无隐式 sampling/JIR/credential/release 权限 |
| 外部 EDA/router/parser | 隔离候选执行 | 固定二进制、固定参数模板、受控输入输出目录、无隐式发布权 |
| 高级 SDK/插件 | 不可信扩展 | 沙箱内读取最小 projection，只提交待验证 JDL/JDP |

WebView 连接成功不等于获得文件或发布权限。Tauri capability 控制前端能调用哪些平台命令，daemon 再按用户、
workspace、工具权限、revision 和领域门禁执行第二次授权。

runtime 的 allow 不能覆盖 daemon 的 deny。工具出现在模型可见列表中只表示可发现，实际执行还要重新检查
用户、session、Agent role、effect/resource claim、项目策略和 daemon capability。

## 3. 沙箱与进程

- KiCad、router、转换器和 parser 使用固定可执行路径与哈希；
- 不允许项目文件指定任意 executable、启动参数或网络端点；
- 临时目录按 execution 隔离，输出只允许写入受控 workspace；
- 解压先检查路径穿越、链接、重复文件、文件数与总大小；
- 外部进程设置 CPU/内存/时间/输出上限并支持取消；
- 取消必须等待真实退出；无法确认时隔离资源；
- 生产发布不能通过 shell 复制文件伪造状态。
- current JIR snapshot 和 operation journal 只能由 kernel transaction service 写入。
- daemon endpoint 使用用户级 OS ACL；Windows named pipe/macOS/Linux UDS 默认不暴露为 TCP 服务；
- daemon 会话密钥不得出现在命令行、URL、日志、crash dump 摘要或 lock file；
- Tauri sidecar 只允许启动签名/哈希和 target triple 匹配的 `jingwei-daemon`，不接受项目提供的可执行路径；
- 前端不直接调用 shell/file system 插件；必要平台能力以最小命令、最小 scope 和默认拒绝配置；
- remote content 不与本地高权限页面共享 capability，外部链接交给系统浏览器并验证 scheme/目标。

## 4. 提示注入与模型边界

- 数据手册、网页、Gerber 文本和库字段只作为数据，不作为系统指令；
- 提取器把内容放入带来源的证据信封，禁止把其中指令提升为工具授权；
- 模型看不到供应商 API 密钥或发布签名材料；
- 模型建议的 MPN、尺寸、引脚和规则必须经过数据/确定性检查；
- 模型不能创建自己的审批、测试记录或工程师身份；
- 对“忽略规则、直接发布、自动豁免”等内容返回明确拒绝和缺口。
- JDL 默认声明式且无文件/网络/进程能力；高级 SDK 在隔离进程运行，只接收最小 JIR projection；
- SDK 产物只作为候选 JDL/JDP，仍需 schema、类型、单位、权限和 postcondition 验证。
- MCP tool annotation 只是未经验证的 hint；即使声明 read-only/idempotent，也按最坏 effect 授权，直到可信
  签名 manifest 和本地策略明确收窄；
- MCP server 不得获得完整会话或其他 server 的上下文；server sampling/递归工具循环默认禁用并需单独审批。

### 4.1 Agent runtime 持久化安全

- 用户输入先持久化再唤醒 provider；相同 request ID 不得创建第二条输入；
- ToolExecution 在副作用前创建，approval request/decision 在展示/唤醒前持久化；失败时 fail closed；
- daemon 已提交但 runtime 未收到响应时只允许按 execution/idempotency key 对账，不允许盲目重放；
- 未知外部副作用进入 `needs-reconciliation`/`safety-quarantine`；
- durable event sequence 支持 UI 回放，但 token delta、动画和 telemetry 不拥有业务终态；
- Main Agent 是唯一会话 owner；专业 Agent 不能继承根凭据、创建子 Agent 或取得 release 工具。

## 5. 凭据与网络

- 凭据使用 Windows Credential Manager/DPAPI 或组织 secret store；
- 数据库只保存 credential reference，不保存明文；
- 日志、模型上下文、工程目录和制造包中不得出现 token/secret；
- 网络适配器使用域名 allowlist、TLS 和明确超时；
- 每个请求标记 data classification，私有设计外发需用户授权；
- 本地优先：不需要云端的 KiCad/验证/导出默认离线执行；
- 云路由/DFM/模型调用前展示将上传的文件及接收方。

## 6. 设计数据分类

| 分类 | 示例 | 默认处理 |
| --- | --- | --- |
| Public | 官方公开数据手册、开源黄金板 | 可缓存，遵循许可证 |
| Internal | 一般需求、未公开 BOM | 本地/组织服务，禁止公共分享 |
| Confidential | 私有原理图、PCB、器件策略 | 本地优先，外发逐次或项目授权 |
| Restricted | 安全关键设计、客户受控资料、出口管制资料 | 默认禁止外部模型/服务，专用流程 |

## 7. 元器件与库供应链

- 任何库资产保存来源、许可证、版本和 hash；
- 供应商签名/hash 可用时必须验证；
- 官方库升级不自动覆盖项目锁；
- 第三方库进入 quarantine，完成 parser、几何、pin map 和人工审查后才能批准；
- 不允许通过相似文件名或模糊 MPN 自动绑定生产器件；
- 组织级库发布需要双人审查或等价质量流程；
- 发现已批准库错误时标记所有引用 revision 并触发影响分析。

JIR 不能直接复用第三方 EDA/renderer/solver 的私有 schema；所有依赖通过 adapter 映射，避免恶意或失控字段
进入核心对象模型。

## 8. 许可证

### 8.1 KiCad

KiCad 官方库采用 CC-BY-SA 4.0 并带设计使用例外，设计使用与重新分发库集合的义务不同。产品打包前应由
法务确认官方库、修改后的集合和归属文件如何分发。

### 8.2 Freerouting

Freerouting 为 GPL-3.0。需要评审：

- 是否随安装包分发、作为独立程序调用或让用户自行安装；
- 修改版本的源码提供义务；
- 与 Jingwei 专有代码的进程/协议边界；
- 许可证文本和第三方 notice。

本文不是法律意见，未完成评审前不得把候选工具直接放入商业安装包。

### 8.3 第三方库与器件 API

- 每个 provider 单独记录 ToS、缓存、批量下载和再分发许可；
- 禁止通过网页抓取绕过 API 配额、登录或许可；
- 不得把只允许个人/单项目使用的库提升为组织共享库；
- 无法确认许可时仅保存用户本地引用，阻止再分发。

## 9. 电子安全边界

Jingwei 的自动化结果不能替代专业电气安全和法规认证。风险 profile 决定哪些能力可以自动生成、哪些只能分析、
哪些必须阻断。

MVP 默认阻断生产发布：

- 市电/高压/高能量电池；
- 功能安全、生命支持、汽车制动/转向等安全关键用途；
- 未实现验证能力的 RF/高速/高功率/隔离设计；
- 用户试图通过改名、豁免或移除标记绕过风险分类。

风险分类以真实电压、能量、用途、环境和法规为依据，不以项目名称或用户措辞判断。

## 10. 标准与法规

项目应在 `CompliancePlan` 中选择适用标准、市场和证据，不把所有标准硬编码为全局要求。可能涉及：

- PCB 设计、封装、制造、装配和可接受性标准；
- Gerber、IPC-2581 等数据交换规范；
- RoHS/REACH 与材料声明；
- EMC、安规、无线、电池、汽车、医疗或行业专用规范；
- 客户/工厂内部 DFM/DFT 和质量规则。

Jingwei 只记录适用性、检查覆盖和证据；没有认证机构报告时不得显示“已认证”。标准内容可能受版权保护，
只能在获得授权的环境中使用，不将完整付费标准打包到产品或模型上下文。

## 11. 角色与职责

| 角色 | 可以做 | 不可以做 |
| --- | --- | --- |
| User/Requester | 提需求、确认偏好、查看设计 | 代表未授权工程角色签核 |
| Hardware Engineer | 原理图/器件/电气审查、issue waiver | 替代制造/测试签核（除非组织授予多角色） |
| PCB Engineer | 约束、布局布线、PCB waiver | 伪造样板测量 |
| Manufacturing Engineer | DFM、装配、制造包签核 | 改电气设计而不创建 revision |
| Test Engineer | 计划与执行样板/生产测试 | 用模型预测代替实测 |
| Supply Chain | AVL、库存、生命周期、替代料采购批准 | 绕过电气等价性验证 |
| Release Manager | 检查签核并发布/撤销 | 在缺失强制门禁时强制放行 |
| Agent | 生成候选、执行工具、解释证据 | 扮演人类角色或签名 |

## 12. 审计事件

以下事件不可变记录：

- 需求基线和变更；
- 器件选择、替代和供应快照；
- 库获取、生成、验证、批准和撤销；
- ECAD revision 创建、合并和外部编辑导入；
- JDL 编译、JDP patch、JIR migration、projection 与等价性检查；
- ERC/DRC/DFM 执行和所有 waiver；
- 外部数据上传、目标和用户授权；
- 样板测试记录的创建/修订；
- release candidate 创建、Production Release、撤销；
- 工具链/profile/规则升级；
- 权限拒绝、安全隔离和恢复。
- Agent input/run/model attempt/tool execution/approval 的创建、终态、重启恢复和对账；
- MCP server 连接、capability/tool list 变化、调用目标、外发数据分类和拒绝原因；

审计包含 actor、时间、对象、前后 revision、原因和证据引用，不保存秘密。

## 13. 发布完整性

- Production Release 目录不可覆盖；
- manifest 包含所有文件 hash、工具链、验证和批准；
- 使用组织受控签名密钥签名 manifest；
- 签名私钥不进入 Agent 进程或普通用户 workspace；
- 验证工具可在离线环境检查包完整性和签名；
- 撤销不删除旧发布，而是追加 revoke record；
- 制造商 CAM 修改必须回传并绑定，否则工厂实际生产数据与 release 不一致。

## 14. 事件响应

### 14.1 错误库/器件数据

1. 撤销组织级库/器件批准；
2. 查询所有引用 revision/release；
3. 按严重性通知负责人；
4. 阻止受影响项目新生产发布；
5. 修复并重新验证；
6. 对已生产批次执行质量/召回评估。

### 14.2 凭据泄露

1. 立即撤销/轮换；
2. 停止相关适配器；
3. 检查日志、artifact 和外发记录；
4. 清理缓存和历史错误信息；
5. 恢复后验证权限最小化。

### 14.3 发布包被替换

1. 签名/hash 验证失败即禁止生产；
2. 隔离包和相关 workspace；
3. 核对审计与构建环境；
4. 从受信 revision 重新生成；
5. 需要时撤销已有发布。
