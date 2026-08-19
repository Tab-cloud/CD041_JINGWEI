# AI 原生 EDA 工具链与外部集成

> 版本：0.3  
> 日期：2026-08-19  
> 状态：候选技术方案，需用 spike 验证

## 1. 基线决策

Jingwei 的首要 EDA 基线是自研 **Design Kernel + JDL + JIR + JDP**。KiCad 10.0.5 是首个锁定的
**Reference Backend**，而不是 Jingwei 的核心语言或唯一设计源。

此决策让 Agent 直接使用需求、模块、接口、网络、约束、证据和 revision 等语义对象，同时用成熟 KiCad
生态降低首期生产风险。详细决策见 [09-ai-native-eda-architecture.md](09-ai-native-eda-architecture.md)。

KiCad 10.0.5 是 2026-07-22 发布的稳定修复版；每个项目通过 toolchain lock 明确记录完整版本。

选择 KiCad 作为首个 reference backend 的理由：

- 原生工程文本格式公开，符号、封装、原理图和 PCB 使用可解析的 S-expression；
- CLI 可执行 ERC、DRC、BOM 和多种制造输出；
- 可输出 Gerber/钻孔，也支持 IPC-2581C、ODB++ 等智能制造格式；
- KiCad 10 官方库持续维护，并以 STEP 作为官方 3D 模型格式；
- Windows、本地/离线和开源生态适合交叉验证与工程师兼容；
- IPC API 可在 GUI 运行时提供定位和交互，但 Jingwei 原生内核不依赖 UI 自动化。

参考：

- [KiCad 10.0.5 Release](https://www.kicad.org/blog/2026/07/KiCad-10.0.5-Release/)
- [KiCad 10 CLI](https://docs.kicad.org/10.0/en/cli/cli.html)
- [KiCad S-expression 格式](https://dev-docs.kicad.org/en/file-formats/sexpr-intro/index.html)
- [原理图格式](https://dev-docs.kicad.org/en/file-formats/sexpr-schematic/)
- [PCB 格式](https://dev-docs.kicad.org/en/file-formats/sexpr-pcb/)

## 2. 版本锁定

每个可验证项目包含：

```yaml
toolchain:
  jingwei:
    kernel: 0.1.0
    jdl: 0.1.0
    jirSchema: 0.1.0
    jdp: 0.1.0
    geometry: 0.1.0
  kicad:
    version: 10.0.5
    platform: windows-x86_64
    executableSha256: ...
  libraries:
    symbols: { source: kicad-official, tag: 10.0.5, commit: ... }
    footprints: { source: kicad-official, tag: 10.0.5, commit: ... }
    models3d: { source: kicad-official, tag: 10.0.5, commit: ... }
  router:
    provider: freerouting
    version: 2.2.4
    sha256: ...
  manufacturerProfile:
    id: jlcpcb-standard
    version: 2026-08-19
```

具体 commit/tag 必须由安装和镜像流程实测填充，不能从文档示例直接复制。

## 3. Jingwei 原生内核

### 3.1 技术栈建议

- 规范实现语言：Rust；JDL/JIR/JDP、domain、store、library、geometry、ERC/DRC、solver adapter、exporter
  和制造工具均位于同一 Cargo workspace；
- 进程：`jingwei-daemon` 是每个工作区唯一 JIR/journal 写入者，Tauri、Eval Agent、CLI 和 CI 都是 JDP client；
- schema：Rust 强类型模型生成 JSON Schema 和 TypeScript binding，禁止手工维护第二套协议类型；
- graph/index：Rust 内存强类型索引 + SQLite metadata，几何/大快照使用独立内容寻址文件；
- 本地协议：Windows named pipe、macOS/Linux Unix domain socket；Phase 0 使用可审查的长度前缀 JSON，
  大制品通过 artifact reference，二进制编码只在 benchmark 证明必要时增加；
- 原生 UI：Tauri 2 + React/TypeScript 薄客户端；PCB 大画布使用 Canvas/WebGL/WebGPU，几何、hit-test、snap
  和最终坐标事务由 Rust 完成；
- solver：稳定 JDP/JIR adapter；优先 Rust，实现或包装外部 solver 时必须保持进程隔离和规范结果映射；
- JDL parser：Rust 独立 grammar/AST/type checker/formatter，不依赖模型解析自然语言；
- 高级 SDK：TypeScript/Python 只生成候选 JDL/JDP，在沙箱中运行，不直接访问 workspace 或 JIR store。

Tauri 是首版展示壳而非核心依赖。若大型画布在目标硬件上经优化仍未达到门槛，可将画布替换为 Rust `wgpu`
或 Slint 原生渲染面；JIR/JDP、daemon 和工具合同保持不变。完整评估见 `10-rust-tauri-platform-architecture.md`。

### 3.2 语言与协议

- JDL 只表达声明式电子设计，不默认执行任意代码；
- JIR 使用稳定 ID、有类型 entity/edge、物理单位、provenance 和 constraint；
- JDP 是所有写操作的唯一入口，支持 query/patch/validate/solve/diff/branch/merge；
- JDP 信封支持版本握手、request ID、幂等键、deadline、cancel、分页、任务事件和结构化错误；
- TypeScript/Python SDK 是可选高级入口，在沙箱中生成 JDL/JDP，不直接访问 workspace 或 JIR store；
- 表面语法在黄金板验证后冻结，JIR/JDP schema 从 Phase 0 开始版本化。

### 3.3 可评估复用的开源组件

- tscircuit 的 Circuit JSON、viewer、footprinter、autorouting dataset/solver 接口；
- atopile 的声明式模块、接口、单位、公差、assertion 和选型思想；
- 通用 SAT/SMT、约束优化、图算法和空间索引库；
- 成熟 Gerber/Excellon/STEP parser 与 geometry library。

复用组件通过 anti-corruption adapter 映射 JIR，不把其上游 schema 直接设为 Jingwei 核心 schema。

## 4. KiCad Reference Backend

### 4.1 文件级 AST

用于 JIR projection、外部工程导入和等价性检查：

- 解析 `.kicad_pro`、`.kicad_sch`、`.kicad_pcb`、`.kicad_sym`、`.kicad_mod`；
- 未识别字段 round-trip 保留；
- 维护 JIR ID 与 KiCad UUID 的 projection map；
- writer 写入 `generator=jingwei-agent`；
- 每次写入后重新解析，并由锁定 KiCad 执行 open/save 兼容性验证。

不使用正则或模板直接替换坐标/网络。遇到 JIR 尚未覆盖的外部特性时应隔离并拒绝破坏性导入，而不是丢弃
字段或让 KiCad 成为第二套真相。

### 4.2 CLI

建议能力映射：

| 能力 | KiCad CLI | 用途 |
| --- | --- | --- |
| 原理图检查 | `sch erc` | 生成 JSON/报告并通过退出码识别违规 |
| BOM | `sch export bom` | 按变体和字段导出规范化 BOM |
| PCB 检查 | `pcb drc` | 全量 DRC、原理图一致性和报告 |
| Gerber | `pcb export gerbers` | 每层制造输出 |
| 钻孔 | `pcb export drill` | Excellon 与 drill map |
| 位置文件 | `pcb export pos` | CPL/PnP 原始数据 |
| IPC-2581 | `pcb export ipc2581` | B/C，首选 C，视工厂支持 |
| ODB++ | `pcb export odb` | 可选智能制造输出 |
| IPC-356 | `pcb export ipc356` | 电气测试/网表交叉检查 |
| STEP/3D | `pcb export step` | 机械检查和交付 |
| PDF/SVG | schematic/pcb export | 人工审查制品 |

实际命令和参数由版本探测测试生成，不把文档命令字符串当作唯一真相。

### 4.3 IPC API

官方说明 KiCad 9/10 IPC API 连接运行中的 GUI；`kicad-python` 是官方 Python bindings。仅建议用于兼容工作流：

- UI 中定位当前 issue 对象；
- 选择/高亮器件、焊盘、网络和区域；
- 同步用户手工编辑和 Agent 变更预览；
- 触发受控的打开、缩放和交互操作。

不建议用途：

- 作为 CI/ERC/DRC/制造输出的唯一执行路径；
- 假设所有原理图编辑能力都已稳定覆盖；
- 在多个 KiCad 实例存在时自动猜测目标实例。

参考：[KiCad IPC API for add-on developers](https://dev-docs.kicad.org/en/apis-and-binding/ipc-api/for-addon-developers/)

## 5. 自动布局

Jingwei 原生内核需要实现 JIR 约束驱动的候选生成器：

1. 固定机械对象：板框、孔、连接器、天线/禁布区、用户锁定器件；
2. 建立功能簇与连接图；
3. 先放电源和关键回路，再放接口、时钟/晶振、普通器件；
4. 对齐装配方向、边缘距离、返修空间和测试点；
5. 使用启发式/模拟退火/局部搜索生成多个候选；
6. 硬约束过滤后，以飞线、拥塞、回路、热和装配指标排序；
7. 返回局部可解释的布局原因和仍需人工复核的区域。

MVP 不追求任意板型的最优布局。先为一个受控黄金板族建立可测指标和人工基线。

## 6. 自动布线

### 6.1 候选路由器

Freerouting 2.2.4 可通过 Specctra DSN 读取设计，并输出 SES 供 KiCad 导回；项目提供 CLI/headless 路径，
许可证为 GPL-3.0。建议作为首个外部候选路由器，但必须完成许可证分发评审和版本隔离。

参考：

- [freerouting 官方仓库](https://github.com/freerouting/freerouting)
- [命令行参数](https://github.com/freerouting/freerouting/blob/master/docs/command_line_arguments.md)

### 6.2 路由流程

```text
JIR constrained board
  → native route problem / export DSN
  → run router with pinned version/config/seed/budget
  → import solution into isolated JIR revision
  → native normalize / zone fill / DRC
  → project to KiCad and run full reference DRC
  → score candidate
  → accept as candidate or discard
```

自动路由器成功退出不代表 PCB 合格。任何未布通网络、DRC 违规、关键回路不满足、回流路径不明或转换丢失都
淘汰候选。

### 6.3 路由策略

- 关键电源、晶振、模拟敏感、差分和时序网络优先使用专用规则/受控路由；
- 可以把关键网络排除在通用自动布线之外；
- 布线预算、线程、层方向、网类和 via 成本写入配置；
- 为同一设计生成多个 seed 候选，只有全量检查通过者参与评分；
- MVP 允许“关键网络策略生成 + 普通网络自动布线 + 工程师复核”的混合流程。

## 7. 器件数据源

### 7.1 数据源顺序

1. 项目/组织批准器件库与冻结快照；
2. 制造商官方产品页和数据手册；
3. 授权分销商 API；
4. 经批准的聚合数据服务；
5. 普通 Web 搜索仅用于发现来源，不直接成为生产选型事实。

### 7.2 首批适配器建议

- DigiKey Product Information API：参数、推荐料和供应数据；
- Mouser API：作为第二个全球分销数据源；
- LCSC/JLCPCB：首期至少支持用户提供/导入器件快照与 JLC/LCSC 料号，公开 API 能力和许可确认后再实现在线适配；
- 制造商官网：数据手册和封装图按受控下载器保存证据；
- 企业内部 AVL/PLM/ERP：后续通过只读适配器接入。

DigiKey 官方开发门户给 Product Information 配额和 OAuth 访问；适配器必须处理额度、授权和数据 freshness，
不得回退到未授权网页抓取。[DigiKey Product Information](https://developer.digikey.com/product-details/product-information)

### 7.3 规范化

跨源合并前统一：

- 制造商名称和精确 MPN；
- 数值、单位、范围、典型值与测试条件；
- 封装的制造商名称、JEDEC/IEC 名称和 KiCad footprint；
- 生命周期枚举与来源可信度；
- 币种、数量阶梯、税费/运费是否包含；
- 时间戳和区域。

不同数据源冲突时保留双方事实并优先要求制造商证据，不做静默覆盖。

## 8. 库来源与许可证

### 8.1 官方 KiCad 库

首选锁定版本的官方 symbols、footprints 和 STEP 模型。KiCad 官方库为 CC-BY-SA 4.0 并带设计使用例外，
允许在商业/闭源设计中使用，但重新分发库集合仍有同许可证与归属要求。

参考：

- [KiCad libraries license](https://www.kicad.org/libraries/license/)
- [KiCad library downloads](https://www.kicad.org/libraries/download/)

### 8.2 第三方库

KiCad 官方列出了 DigiKey、Octopart、SnapEDA 等第三方来源，但明确这些库不由 KiCad 维护或背书。
每个适配器必须单独审核服务条款、下载/缓存/再分发限制和数据质量。

参考：[KiCad third-party libraries](https://www.kicad.org/libraries/third_party/)

### 8.3 自动生成库

生成优先依据：

1. 制造商官方推荐 land pattern 和 pin table；
2. 封装标准和公司已验证生成器；
3. 数据手册明确尺寸及公差；
4. 缺少关键尺寸时停止并请求资料，不从产品照片估算。

自动生成输出包含：source evidence、单位、尺寸表、生成器版本、渲染图、pin-pad map 和验证报告。

## 9. 制造商 Profile

Profile 不只是最小线宽表，应包含：

- 支持板材、层数、板厚、铜厚、表面处理；
- 最小线宽/间距、环宽、孔径、阻焊桥、板边间距；
- 钻孔格式、单位、原点、槽孔和 NPTH/PTH 规则；
- 层命名、板框和特殊层说明；
- BOM/CPL 字段、坐标、旋转和面定义；
- 装配能力、器件尺寸、面数、工艺边和拼板要求；
- 特殊工艺和人工备注；
- profile 来源 URL、抓取日期、人工审核和版本。

JLCPCB 官方资料要求制造 Gerber、钻孔，并在装配时使用 BOM 与 CPL；BOM/CPL 的 reference designator 和坐标
必须保持一致。首个 profile 以这些官方页面和一次真实下单/DFM 反馈校准：

- [Gerber 文件准备](https://jlcpcb.com/help/article/gerber-files-preparation)
- [BOM/CPL 建议](https://jlcpcb.com/help/article/advice-for-bom-and-cpl-files-preparation)
- [KiCad 10 BOM/CPL](https://jlcpcb.com/help/article/how-to-generate-the-bom-and-centroid-file-from-kicad)

## 10. 制造输出

### 10.1 必选源与审查文件

- JDL、JIR schema/snapshot、operation journal 和 revision manifest；
- KiCad `.kicad_pro`、`.kicad_sch`、`.kicad_pcb` reference projection；
- 项目私有 symbol/footprint/STEP；
- 需求基线、选型记录和设计规则；
- 原理图 PDF、PCB 层/3D 预览、装配图；
- ERC/DRC/DFM 与人工审查报告。

### 10.2 标准制造包

- Gerber X2/X3 能力按工厂兼容性配置；
- Excellon drill、drill map；
- Gerber Job；
- IPC-356 netlist；
- BOM、CPL/PnP、装配图、坐标与旋转说明；
- fab drawing/notes、stackup、工艺说明；
- paste layer/stencil 信息；
- STEP 和必要机械资料；
- manifest、SHA-256 和签核记录。

Gerber 的格式依据来自 Ucamco 官方规范与测试文件：
[Ucamco Gerber downloads](https://www.ucamco.com/en/gerber/downloads)。

### 10.3 智能制造格式

如工厂接受，并行输出 IPC-2581C。KiCad CLI 支持 IPC-2581 B/C，默认可使用 C；工厂明确要求 Gerber/BOM/CPL
时仍必须提供。IPC 将 IPC-2581C 定义为设计到制造的数据交换标准：
[IPC-2581C announcement](https://www.ipc.org/news-release/ipc-releases-ipc-2581-revision-c-generic-requirements-printed-board-assembly-products)。

## 11. 技术 Spike 清单

在确定最终代码结构前完成：

1. 冻结 JIR v0、JDP transaction/query 和 JDL 最小 AST/formatter；
2. JDL、Agent patch、GUI 操作对同一设计产生相同 JIR revision；
3. 在干净 Windows 环境锁定 KiCad 10.0.5 并记录二进制/库哈希；
4. 将 JIR 最小层次原理图投影到 KiCad，GUI 打开/保存，CLI ERC 通过；
5. 将 JIR 2 层 PCB 投影到 KiCad，GUI 打开/保存，CLI DRC 通过；
6. 执行 JIR → KiCad → JIR 等价性和外部人工编辑导入测试；
7. 从同一 revision 导出 Gerber、drill、BOM、CPL、IPC-356、STEP、IPC-2581C；
8. 对所有输出做 parser/read-back 和 manifest 校验；
9. 生成一个无官方库的封装，完成 pin-pad/几何/渲染人工核对；
10. 用 Freerouting/可复用 solver adapter 跑通最小板并执行 native + KiCad DRC；
11. 用 JLCPCB profile 打包并通过一次真实上传的自动识别/DFM（不直接下单）；
12. 建立故意错引脚、错封装、错旋转、漏层、stale BOM 和冲突 patch 的负向样例集；
13. 评估 tscircuit/atopile/Freerouting 与第三方库/数据源条款，形成复用和分发边界。
