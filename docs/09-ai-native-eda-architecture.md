# AI 原生 EDA 架构评估与决策

> 决策版本：1.3  
> 日期：2026-08-19  
> 状态：Accepted as architecture baseline  
> 决策：自研 Rust Jingwei 原生设计内核；Tauri 2 为首版薄客户端；KiCad 保留为兼容投影和独立验证后端。

## 1. 结论

Jingwei 不应以“让 AI 操作传统 EDA GUI”为产品核心，也不应把 `.kicad_sch`/`.kicad_pcb` 等外部格式直接
当作 Agent 的母语。推荐架构是：

1. 自研 **Jingwei Design Kernel**，headless-first，UI、Agent、CLI 和 CI 使用同一个内核；
2. 自研声明式 **JDL**、规范化 **JIR** 和事务化 **JDP**；
3. 所有 Agent 修改通过结构化 query/patch/validate 接口完成，不直接拼接 EDA 文件或模拟鼠标；
4. 原理图、PCB、器件、库、约束、证据和制造信息在同一语义图中关联；
5. KiCad 10.0.5 在首期承担导入/导出、工程师兼容和独立 ERC/DRC/制造输出交叉验证；
6. Jingwei 原生验证器达到黄金覆盖前，不单独承担 Production Release 放行；
7. 随能力成熟，制造输出逐步由原生内核直接生成，同时继续保留异构工具交叉验证。
8. 核心语言统一为 Rust，`jingwei-daemon` 是每个工作区唯一状态和写事务所有者；
9. Tauri 2 + React/TypeScript 只承担工作台展示与交互，Agent、GUI、CLI 都通过相同 JDP 调用 daemon。

这是“自研核心、复用成熟生态”的路线，不是从零重写所有几何算法、文件 parser、路由器和制造标准。

## 2. 为什么需要 AI 原生内核

传统 EDA 的基本交互单位通常是页面、图元、菜单和坐标；AI 真正需要的单位是需求、模块、接口、网络、约束、
器件决策、证据、变更影响和验证结果。

如果只封装传统 EDA，会长期遇到：

- 模型必须理解庞大且易变的文件语法；
- 一次局部修改需要重写整份文件，难以保证不破坏人工内容；
- “为什么这样设计”无法与对象和约束绑定；
- 查询常常退化为文本搜索或截图理解；
- GUI 自动化不稳定、不可并发、难以 headless 验证；
- 设计、BOM、库、规则和制造包分散在不同格式；
- 事务、分支、语义 diff、回滚和多候选求解很难统一；
- AI 容易输出语法正确但工程语义错误的文件。

AI 原生内核应让自然语言只负责提出意图，让确定性语言、类型、约束和编译器把意图转成设计。

## 3. 可选路线评估

| 路线 | 优点 | 主要问题 | 结论 |
| --- | --- | --- | --- |
| KiCad 外挂/GUI 自动化 | 上手快、成熟编辑器 | 语义阻抗大、API 覆盖受限、难做事务与 AI 查询 | 只作兼容层 |
| 直接生成 KiCad S-expression | 无需 GUI、可交付源 | 外部格式成为内部模型，增量/跨 EDA/语义演进受限 | 只作 exporter/importer |
| 直接采用 tscircuit | TypeScript、Circuit JSON、AI/路由/渲染生态 | 产品语义与成熟度由上游决定；React 可执行源码不适合作为安全生产真相 | 参考并评估复用组件 |
| 直接采用 atopile | 声明式语言、单位/约束/选型、KiCad 协同 | 当前仍快速演进；语言和布局工作流不完全匹配 Jingwei 的 Agent/发布模型 | 参考语言与编译器设计 |
| 采购/集成 JITX | 代码化设计、约束和高级求解能力成熟 | 商业依赖、核心可控性和产品集成边界 | 可作能力/设计参考或商业适配 |
| 自研全部 EDA | 最大控制力 | 成本、周期和正确性风险极高 | 不采用全栈重写 |
| **自研语义内核 + 复用成熟后端** | AI 接口可控、可演进、兼顾生产验证 | 需要明确 IR/编译器和双重验证投入 | **采用** |

## 4. 现有项目给出的有效启示

### 4.1 tscircuit

tscircuit 使用 React/TypeScript 描述电子设计，并提供 Circuit JSON、渲染、自动布局/布线和制造输出生态；其
核心库可把 TypeScript 转成结构化 Circuit JSON。这证明“结构化中间表示 + 通用软件语言 + 可组合模块”对 AI
生成有明显优势。

Jingwei 借鉴：

- TypeScript 友好的类型和工具生态；
- 结构化、关系化 JSON 作为求解器/渲染器交换格式；
- viewer、router、dataset/benchmark 分离；
- AI 不依赖像素即可生成和查询电路。

不直接照搬：

- React component tree 不作为生产设计的唯一真相；
- 任意 TypeScript 执行不进入默认安全边界；
- 低层扁平 JSON 不能替代需求、决策、证据和发布语义。

参考：[tscircuit 官方文档](https://docs.tscircuit.com/)、
[核心仓库](https://github.com/tscircuit/tscircuit)、
[autorouting 数据与接口](https://github.com/tscircuit/autorouting)。

### 4.2 atopile

atopile 使用声明式 `.ato` 语言表达模块、接口、单位、公差和 assertions，编译器执行约束、选型和检查，并与
KiCad 布局协作。它证明电子设计语言应表达“必须满足什么”，而不只表达“图元放在哪里”。

Jingwei 借鉴：

- 声明式模块/接口；
- 带单位、范围、公差和断言的类型系统；
- 编译期器件选择和深度验证；
- 代码与外部 PCB 布局协作。

不直接照搬：

- Jingwei 必须把 Agent 事务、证据、审批、制造 revision 纳入语言与协议；
- 需要稳定的 GUI/Agent 双向编辑，而不仅是代码驱动；
- 当前 atopile 官方文档仍提示 pre-release 和快速变化，不能把其版本稳定性直接继承为生产承诺。

参考：[atopile 官方仓库](https://github.com/atopile/atopile)、
[官方文档](https://docs.atopile.io/)。

### 4.3 JITX

JITX 以 Python 类表达 Components、Circuits 和层次化 circuit graph，并支持 introspection、pin assignment 和
代码化物理约束。其 tag/condition/effect 约束模型说明复杂 PCB 规则应该可以查询、组合和确定性选择，而不是
散落在 GUI 对话框中。

Jingwei 借鉴：

- 层次化 circuit graph；
- 设计 introspection；
- tag + condition + effect 约束；
- 约束参与生成和路由，不只是事后 DRC。

参考：[JITX Circuit Design](https://docs.jitx.com/en/latest/essentials/design/index.html)、
[JITX Design Constraints](https://docs.jitx.com/en/latest/essentials/physical_design/design-constraints.html)。

## 5. 三层语言与协议

### 5.1 JDL：Jingwei Design Language

面向工程师、AI 和版本控制的声明式语言，建议扩展名 `.jwd`。

负责表达：

- requirements、assumptions、interfaces、modules；
- parts、参数范围、选型策略和 variant；
- connections、power domains、signal semantics；
- units、tolerances、equations、assertions；
- board、stackup、placement/routing/manufacturing constraints；
- reusable design blocks 和组织模板；
- 可读的人工 override 与 approval reference。

JDL 默认不支持任意文件、网络、进程和反射执行。高级用户可通过受控 TypeScript/Python SDK 生成 JDL/JDP，
但 SDK 在沙箱中运行，其输出仍须通过 schema、类型和约束编译。

### 5.2 JIR：Jingwei Intermediate Representation

规范化、带类型、关系化的设计图，是唯一设计语义真相。JDL、GUI、导入器和 Agent 操作最终都编译为 JIR
revision。

JIR 同时包含：

- intent graph：需求、决策、假设、风险、证据；
- logical graph：模块、器件、接口、pin、net、电源/时钟域；
- library graph：symbol、footprint、pad、3D、provenance；
- physical graph：board、layer、region、placement、trace、via、zone；
- constraint graph：selector、condition、effect、priority、source；
- verification graph：finding、coverage、waiver、test；
- manufacturing graph：variant、BOM、CPL、process、release manifest。

JIR 在内存中使用强类型对象/索引，在持久化和跨进程接口中使用版本化 JSON/Protobuf；大几何数据可使用
CBOR/FlatBuffers 等二进制快照，但必须有确定性文本摘要和 schema。

### 5.3 JDP：Jingwei Design Protocol

Agent、GUI、CLI、插件和求解器使用的唯一写入口。

核心操作：

- `design.query`：按类型、tag、关系、空间、issue 或 requirement 查询；
- `design.explain`：返回对象为何存在、由哪些约束/证据决定；
- `design.proposePatch`：创建带 precondition 的候选变更；
- `design.applyPatch`：事务应用并返回新 revision；
- `design.compile`：JDL/导入内容编译为 JIR；
- `design.validate`：运行指定规则集并生成 findings；
- `design.solve`：调用选型、pin assignment、布局、布线等求解器；
- `design.diff`：语义 diff 和影响分析；
- `design.export/import`：外部格式桥；
- `design.branch/merge`：多候选和人工/AI 并行修改。

协议使用 JSON Schema/Protobuf 定义，不把自然语言命令当作可执行写操作。

## 6. 唯一真相与可编辑性

### 6.1 权威模型

```text
Natural language / JDL / GUI / imported EDA
                    │
                    ▼
            validated JDP operations
                    │
                    ▼
        JIR revision + operation journal
                    │
      ┌─────────────┼─────────────┐
      ▼             ▼             ▼
 Native views   KiCad projection  Manufacturing projection
```

- JIR revision 是唯一规范化设计真相；
- operation journal 记录如何从父 revision 得到当前 revision；
- JDL 是确定性、可读的源投影，可由编译器与 formatter 稳定生成；
- KiCad/Altium/EasyEDA 文件是外部格式投影或导入源；
- Gerber/BOM/CPL 是发布候选 projection；
- 编译缓存、渲染缓存和索引不拥有设计真相。

### 6.2 GUI 与语言双向编辑

GUI 操作不直接改数据库字段，而是生成相同 JDP patch；Agent 与 GUI 因而共享：

- precondition、权限、事务和回滚；
- revision、语义 diff 和审计；
- validation 和影响分析；
- undo/redo 与 branch/merge。

JDL formatter 只改写受当前 patch 影响的声明块或生成规范化文件。无法无损回写的高级 JDL 构造必须提前
拒绝 GUI 破坏性编辑，或降级为受保护的 generated block。

## 7. JDL 语言草案

```text
design EnvSensorNode {
  requirement input_voltage: Voltage in 9V..24V
  requirement temperature_error: TemperatureError <= 0.5degC
  target prototype=10 annual=5000 region="CN"

  interface vin: PowerInput { voltage satisfies input_voltage }
  interface fieldbus: RS485 { isolated=false }

  module power: BuckRegulator {
    input  connects vin
    output voltage = 3.3V +/- 2%
    output current >= 500mA
    select prefer stock(region="CN") then bom_cost
  }

  module controller: MCU {
    rail connects power.output
    require interfaces { i2c >= 1; uart >= 1; swd >= 1 }
  }

  connect controller.uart to fieldbus.logic
  assert every(controller.power_pin) has decoupling within 5mm

  board {
    outline rectangle(60mm, 40mm)
    layers = 2
    assembly_side = top
  }

  constrain net where tag(power) {
    width >= current_width(current=500mA, rise=10degC)
  }
}
```

语法只是草案；Phase 0 先冻结 AST 和语义，再冻结表面语法。模型调用主要生成 AST/JDP，不要求每次直接输出
完整文本，从而减少标点/缩进导致的失败。

## 8. JIR 核心设计

### 8.1 对象模型

每个对象至少包含：

```ts
interface JirEntity<TKind extends string, TProps> {
  id: string
  kind: TKind
  revisionCreated: string
  revisionUpdated: string
  parentId?: string
  tags: string[]
  props: TProps
  sourceRefs: SourceRef[]
}
```

关系使用有类型 edge，避免把所有连接压成字符串：

```ts
interface JirEdge {
  id: string
  kind: "contains" | "connects" | "implements" | "constrained-by" |
        "selected-from" | "verified-by" | "derived-from" | "projected-to"
  from: string
  to: string
  props?: Record<string, unknown>
}
```

### 8.2 单位和数值

- 内部值为 `quantity + dimension + unit + tolerance/range + conditions`；
- 禁止裸浮点代表具有物理意义的数；
- 编译器在连接、计算和比较时做量纲检查；
- 数据手册典型值、最小/最大值和测试条件分开保存；
- 坐标采用固定内部单位，序列化时明确单位和精度。

### 8.3 约束

```ts
interface JirConstraint {
  id: string
  selector: SelectorExpression
  relationSelector?: SelectorExpression
  condition?: BooleanExpression
  effect: ConstraintEffect
  strength: "hard" | "soft"
  priority: number
  sourceRefs: string[]
  appliesBeforeStage: string
}
```

selector 支持 object type、tag、hierarchy、net class、layer、region、spatial relation 和 requirement reference。
约束选择必须确定性，冲突时产生 error，不允许按插入顺序静默覆盖。

## 9. 面向 AI 的接口原则

### 9.1 小上下文查询

模型不加载整板 JSON，而是调用：

- `query module power with dependencies depth=2`；
- `query open issues severity>=warning relatedTo=U3`；
- `query nets crossing region analog`；
- `query requirements without verification coverage`；
- `explain placement U1`；
- `diff revision A B scope=power`。

返回稳定 ID、有限字段、分页和 evidence refs。

### 9.2 高层语义修改

优先允许：

- 替换某功能块实现；
- 给电源 pin 添加去耦策略；
- 把连接器固定在板边；
- 给一组网络应用约束；
- 求解满足接口/成本/库存的器件；
- 重新布局受影响模块。

只有求解器或人工精调才使用坐标级 patch。Agent 不应为每条线生成大量无约束坐标 token。

### 9.3 Patch 与 precondition

```json
{
  "baseRevision": "rev-104",
  "operations": [
    {
      "op": "replace-part",
      "target": "part-instance:U3",
      "withDecision": "part-decision:PD-88",
      "preconditions": [
        { "kind": "current-mpn", "equals": "STM32G030F6P6" },
        { "kind": "object-unlocked" }
      ]
    }
  ],
  "postconditions": [
    { "kind": "pin-map-valid" },
    { "kind": "erc", "severity": "error", "count": 0 }
  ]
}
```

失败返回具体 precondition、对象和修复选项，不半应用 patch。

### 9.4 Explainability

每个求解器决定可回答：

- 为什么选择这个器件；
- 哪条需求/约束导致该网络线宽；
- 为什么器件放在此区域；
- 哪个工具和 seed 生成该走线；
- 修改后哪些检查失效；
- 哪些结论来自模型建议，哪些来自确定性证明/实测。

## 10. 原生编译流水线

```text
parse JDL / validate JDP / import external
  → type & unit checking
  → elaborate modules and interfaces
  → solve parts and pin assignments
  → build logical connectivity graph
  → ERC + domain assertions
  → bind symbol/footprint/model
  → elaborate board and constraints
  → placement solver candidates
  → routing solver candidates
  → native DRC/DFM
  → external cross-validation
  → manufacturing projections
```

各阶段增量编译并缓存依赖图。修改一个电阻值不应重跑不相关布局；修改 footprint 则必须使对应 PCB、制造和样板
验证失效。

## 11. 原生 UI

Jingwei 应提供自己的设计工作台，而不是长期嵌入/遥控 KiCad：

- requirement/decision/issue 面板；
- 原理图语义视图；
- PCB 2D 视图和约束覆盖层；
- 3D/机械视图；
- BOM/器件/库/供应链；
- validation 和 release；
- Agent 对话与 patch preview。

该工作台必须包含可人工编辑的 schematic editor 和 PCB editor。AI 生成的候选应打开为语义对象画布，用户可
直接修改连接、属性、器件位置、板框、区域、走线、过孔和其他受支持对象；静态渲染只用于导出和审查。

技术建议：

- 采用 Tauri 2 + React/TypeScript 桌面壳，复用现有 Eval 技术经验；
- JDL/JIR/JDP、store、几何、规则、solver adapter 和 exporter 全部位于 Rust Cargo workspace；
- `jingwei-daemon` 持有 domain state、workspace lock 和 transaction，UI 只持有视图状态；
- 小型原理图可使用 SVG，PCB/大型画布使用 Canvas/WebGL/WebGPU，但 display list、hit-test、snap 和最终
  坐标事务由 Rust 计算；
- 所有 UI 编辑调用 JDP，不形成第二套编辑逻辑；
- transient drag/draw 不写设计，完成逻辑动作后提交 semantic command；undo/redo 生成可审计 revision；
- 人工修改记录 provenance 和 `ai-editable`/`ask-before-change`/`locked` 策略，后续 AI 默认保留；
- viewer/render package 可以评估复用 tscircuit 等 MIT 组件，但不得让其数据模型反向绑架 JIR；
- 若系统 WebView 无法达到批准的性能/正确性门槛，只替换为 Rust `wgpu`/Slint 原生画布，不改变 JIR/JDP；
- Windows 首发；macOS/Linux 必须分别完成构建、安装、签名、sidecar、图形和黄金板验收后再声明支持。

实现目录、IPC、安全和打包门禁见 `10-rust-tauri-platform-architecture.md`；完整预览与人工编辑需求见
`11-native-workbench-preview-and-editing.md`；通用 Agent 控制面的复用边界见
`12-agent-runtime-reference-architecture.md`。

## 12. 自研与复用边界

### 12.1 必须自研

- JDL AST、类型、单位、公差和 assertions；
- JIR schema、revision、关系、provenance 和迁移；
- JDP query/patch/transaction/diff/merge；
- 需求到设计的 traceability；
- 统一约束模型与 coverage；
- Agent-facing EDA semantic tools 及其 Rust/JDP schema；
- 原生 editor 的对象模型；
- EDA release/signoff policy 与 manufacturing manifest（通用审批等待/恢复复用 Agent runtime）；
- Jingwei 特有的领域 ERC/DFM 规则。

### 12.2 优先复用或适配

- KiCad 官方库和格式 parser/exporter；
- KiCad CLI/ERC/DRC 作为交叉验证；
- Gerber、Excellon、IPC-2581、ODB++ parser/exporter；
- STEP/几何/布尔运算库；
- Freerouting 或其他路由器；
- tscircuit 的 viewer/router/dataset/footprint 组件（逐项许可和质量评估）；
- 数值优化、图算法、SAT/SMT/约束求解器；
- 供应商、PLM、制造商 API。
- Eval 的 session/run/provider/tool/approval/event/MCP runtime；
- Codex App Server/SDK 和 OpenCode V2 的协议/可靠性设计；
- OpenTelemetry 的跨进程观测协议。

### 12.3 暂不自研

- 通用 3D CAD 内核；
- 任意板型的完整 SI/PI/热/EMC 求解器；
- 所有商业 EDA 的无损双向转换；
- 通用高质量全自动布线器；
- 标准/法规认证判定。
- 通用 Agent session、provider streaming、MCP/OAuth、审批等待和 durable workflow 框架。

## 13. KiCad 的新定位

KiCad 10.0.5 不再是“Jingwei 首发唯一 ECAD”，而是首个 **Reference Backend**：

- `JIR → KiCad` 确定性 projection；
- `KiCad → JIR` 受控导入和语义 diff；
- 外部工程师使用 KiCad 精调时形成新 revision；
- KiCad ERC/DRC 与 Jingwei native checks 对照；
- 首期制造输出优先从经过 round-trip 的 KiCad projection 生成；
- projection 与 JIR connectivity/geometry/BOM 必须一致；
- 后期可加入 Altium、EasyEDA、IPC-2581 reference backend。

不得维护“JIR 真相”和“KiCad 真相”两套可独立编辑却不合并的源。任何外部编辑都必须导入成为新 JIR
revision 后才能进入发布流程。

## 14. 分阶段演进

### Stage A：AI-native semantic kernel

- JIR v0、JDP query/patch、revision、units、modules/interfaces/nets；
- JDL 最小 AST 和 formatter；
- native logical ERC；
- KiCad 双向桥和交叉验证；
- 原生只读 schematic/PCB viewer。

### Stage B：原生设计工作台

- GUI 编辑全部通过 JDP；
- library/footprint editor；
- constraint inspector 和 patch preview；
- native board/placement model；
- 多候选 branch/diff/merge。

### Stage C：原生物理求解

- placement solver；
- routing adapter 与 benchmark；
- native geometry DRC/DFM；
- KiCad 继续交叉验证；
- 原生制造 exporter 先 shadow、再参与放行。

### Stage D：原生生产后端

- 原生 Gerber/Excellon/BOM/CPL/IPC-2581 经大规模黄金/工厂验证；
- 与至少一个异构 parser/backend 做持续交叉验证；
- KiCad 从必选后端降为支持的兼容后端；
- 仅在逃逸缺陷、等价性和真实生产数据达到门槛后切换。

## 15. 风险与控制

| 风险 | 控制 |
| --- | --- |
| 自研范围失控 | 严格区分语义内核与成熟算法/格式复用 |
| DSL 过早冻结 | 先冻结 JIR/JDP 语义，表面语法在黄金板后冻结 |
| JIR 变成巨大 JSON | 强类型索引、分页查询、二进制几何快照、文本摘要 |
| GUI/JDL 无法双向 | 所有写操作统一为 JDP；generated block/保护策略 |
| 两套真相漂移 | JIR 唯一真相；外部编辑必须导入新 revision |
| 原生 ERC/DRC 不成熟 | KiCad + 独立 parser + 工厂 DFM 交叉验证 |
| 复用上游绑架架构 | adapter/anti-corruption layer，JIR 不采用上游私有类型 |
| AI 输出任意代码 | 默认声明式语言；高级 SDK 沙箱化，只消费验证后的输出 |
| 性能不足 | 增量依赖图；Rust 原生几何/规则；display list/LOD；WebGPU 与原生 wgpu/Slint 备选；benchmark 驱动 |
| UI 与内核形成双重真相 | daemon 唯一写入者；Rust schema 生成 binding；Agent/CLI/GUI revision 一致性测试 |
| 生产责任过早 | 原生 exporter 先 shadow，样板和签核门禁不变 |

## 16. Phase 0 必须回答的技术问题

1. JIR v0 能否无损表达黄金板需求、logical graph、library 和 2 层 physical graph？
2. JDL/JDP/GUI 三种入口能否产生相同 JIR revision 和语义 diff？
3. JIR → KiCad → JIR 是否保持器件、pin/net、约束、placement 和 route 等价？
4. native ERC 与 KiCad ERC 的差异能否定位到同一对象？
5. 10k/100k 对象的 query、patch、diff 和增量 validation 性能是否可接受？
6. 未识别的外部 EDA 特性如何隔离而不丢失？
7. JDL 能否在模型上下文中局部生成、局部修复，而无需重发整板？
8. renderer/solver 使用 Circuit JSON 等外部格式时，anti-corruption mapping 是否完整？
9. 外部 KiCad 人工修改如何安全导入并使相关验证失效？
10. 哪些原生输出可以 shadow，哪些仍必须由 reference backend 生成？
11. daemon 单写锁、journal 恢复、client 断线、取消和协议升级能否在故障注入下保持原子性？
12. Tauri 系统 WebView 在目标低配机器上的 10k/100k/1M 图元性能和图形正确性是否达到批准门槛？

## 17. Phase 0 验收

以下是完整 Phase 0 product exit 条件，不因五会话 foundation 已合并而自动通过。当前逐项状态见
`14-design-convergence-audit-and-plan.md`；其中真实 UI adapter、reconciliation、完整 KiCad round-trip、签名
安装和黄金板闭环仍为阻塞项。

- 一套由 Rust 类型生成的版本化 JIR/JDP JSON Schema、client binding 和迁移策略；
- 一套最小 JDL grammar/AST/formatter；
- 一套 JDP query/patch/validate 协议；
- Rust Cargo workspace、锁定工具链、daemon、CLI 和自动生成的 TypeScript binding；
- CLI、Agent 和 GUI 对同一 patch 产生相同 revision；
- 一个工作区只能有一个写入 daemon，崩溃/断线/冲突事务不污染 current revision；
- 黄金板 logical design 由 JDL 编译并显示在原生可编辑工作台；
- 用户可在原生工作台完成最小原理图/PCB 编辑，生成语义 diff、inverse undo revision 和增量 finding；
- 导出 KiCad 后 open/save/ERC/DRC 通过；
- KiCad 人工修改导回后产生正确语义 diff；
- 故意冲突的 patch 原子失败，不污染 current revision；
- 原生 ERC 与外部 ERC 差异报告可读；
- 干净 Windows 机器使用 Tauri 安装包启动签名 sidecar、离线完成黄金 patch、升级/重启后保持数据一致；
- 设计全过程无需 Computer Use 操作 KiCad GUI；GUI 只用于人工兼容验收。

## 18. 最终判断

自研 AI 原生 EDA 是正确的长期产品方向，但正确的切入点是 **语义内核、语言和协议**，不是先重写一套传统
原理图/PCB 画布。先让 AI 能可靠地“理解、查询、修改、验证、解释”设计，再逐步替换物理求解和制造后端。

因此当前技术路线确定为：

```text
Jingwei-native semantics first
  + Rust daemon as the single state owner
  + Tauri thin client with replaceable rendering
  + mature EDA interoperability
  + heterogeneous cross-validation
  + staged replacement of backends
```
