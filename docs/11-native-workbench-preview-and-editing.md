# 原生工作台预览与人工编辑需求

> 版本：1.0  
> 日期：2026-08-19  
> 状态：Accepted as product and interaction baseline  
> 范围：AI 生成原理图/PCB 后的原生预览、人工编辑、人机协作、事务和验收。

## 1. 产品结论

AI 生成的原理图和 PCB 必须在 Jingwei 原生工作台中成为**可交互、可编辑的候选设计**，不能只显示 PNG、PDF、
SVG 或 Gerber 预览。

用户应能：

- 查看完整原理图层次、器件、引脚、网络、约束、证据和 ERC finding；
- 查看 PCB 板框、层、封装、焊盘、网络、走线、过孔、铜皮、禁布区、规则和 DRC finding；
- 选择对象并修改属性、连接、位置、方向、约束和几何；
- 撤销、重做、保存草稿、创建检查点、比较 revision；
- 锁定人工确认的对象，防止后续 AI 或 solver 静默覆盖；
- 对 AI 建议按对象或变更块接受、拒绝或继续修改；
- 在人工编辑后继续让 AI 完成局部修复、重新布局、重新布线或验证；
- 从原理图、PCB、3D、BOM 和 finding 之间双向定位同一设计对象。

工作台是 JIR 的交互视图。人工操作与 AI 操作具有相同的 JDP 事务、revision、provenance 和验证语义。

## 2. 端到端工作流

```text
用户需求
  → AI 生成候选 revision
  → 工作台打开“候选设计”
  → 用户预览原理图 / PCB / 3D / BOM / findings
  → 用户直接编辑，或要求 AI 局部修改
  → 工作台显示 patch preview 与影响范围
  → 提交为新 revision
  → 增量 ERC/DRC/约束验证
  → 用户继续编辑、接受或回退
  → 工程审查与制造发布门禁
```

AI 生成结束只表示产生了候选 revision，不表示设计已验证或可以量产。工作台必须始终显示当前成熟度、未解决
finding、失效验证和生产发布缺口。

## 3. 工作台信息架构

建议采用多区域工程工作台：

| 区域 | 主要内容 |
| --- | --- |
| 顶部 | 项目、branch/revision、成熟度、保存/检查点、验证状态、发布状态 |
| 左侧 | 原理图层次、PCB 层、对象树、搜索、未放置/未连接队列 |
| 中央 | Schematic、PCB、3D、BOM、Manufacturing 多标签编辑区 |
| 右侧 | 对象属性、约束、证据、provenance、AI 修改策略、锁定状态 |
| 底部 | ERC/DRC/issues、AI 对话、semantic diff、任务、操作历史 |

交互要求：

- 一个对象在所有视图使用同一稳定 JIR ID；
- 选择器件时同时高亮相关 symbol、footprint、BOM 行、网络和 finding；
- finding 点击后缩放并定位到对象/引脚/焊盘/走线区域；
- AI 回答中引用的对象可在画布定位，画布对象也可作为 AI 请求上下文；
- 视图状态可本地恢复，但规范设计状态必须从 daemon/revision 重建。

## 4. 原理图预览与编辑

### 4.1 预览能力

- 层次页、模块和跨页连接导航；
- 平移、缩放、框选、多选、搜索和适合窗口；
- 器件、引脚、电源符号、网络、总线、标签、连接点、测试点和注释显示；
- pin number/name/type、value、MPN、footprint、单位、替代料和数据手册证据检查；
- 网络高亮、从 pin 追踪完整网络、跨页/跨模块跳转；
- ERC、未连接、地址冲突、电源方向和自定义 finding 覆盖层；
- 当前 revision 与上一个/指定 revision 的增删改语义差异；
- AI 生成、人工修改、导入和库更新的 provenance 标记。

### 4.2 P0 人工编辑能力

- 放置、删除、移动、旋转、镜像 symbol；
- 绘制/删除 wire、junction、bus 和 bus entry；
- 创建、重命名和删除 net label、global label、hierarchical label；
- 修改器件 value、容差、额定值、DNP、注释和允许修改的字段；
- 为器件选择已验证的 symbol、footprint、3D model 和批准替代料；
- 创建/删除页和模块实例，调整层次连接；
- 增加测试点、连接器 pin 分配和接口映射；
- 对对象/模块设置 `locked`、`ask-before-change` 或 `ai-editable`；
- 对 ERC finding 修复、标记待处理或发起具名豁免流程。

涉及 MPN、pin map、footprint 或电源拓扑的修改不能退化为字符串属性编辑；必须调用对应的器件/库/连接变更
服务，执行影响分析并使受影响的 PCB、BOM 和验证结果失效。

### 4.3 P1 扩展

- 可复用模块模板、参数化模块和多单元器件高级编辑；
- 网络/页面重构、总线批量编辑和批量字段表；
- 规则驱动的对齐、分布、自动整理和页面排版；
- 多人评论、对象级审查状态和组织库发布。

## 5. PCB 预览与编辑

### 5.1 预览能力

- 任意铜层、阻焊、丝印、Edge.Cuts、Courtyard、Fab、User layer 的显隐和透明度；
- 板框、安装孔、封装、焊盘、飞线、走线、过孔、铜皮、keepout、尺寸和文本；
- net、net class、差分/长度约束、线宽、间距、过孔和制造规则查看；
- 2D/3D 切换以及 symbol ↔ footprint ↔ BOM 的 cross-probe；
- DRC、未布通、间距、板边、回流路径、热、装配和测试点 finding 覆盖层；
- 单层、单网络、单模块隔离查看和剖面/测量工具；
- AI 布局/布线候选之间的几何和指标比较；
- 当前 revision 与指定 revision 的 placement、routing、constraint 和 copper diff。

### 5.2 P0 人工编辑能力

- 修改板框、安装孔、尺寸和允许的机械约束；
- 移动、旋转、翻面、对齐、分布、锁定 footprint；
- 创建/修改 keepout、placement region、height region 和 routing corridor；
- 交互式绘制、拖动、删除 track 和 via，并遵循当前 net class/DRC；
- 修改局部线宽、过孔类型、层切换和受支持的走线属性；
- 创建/修改 copper zone 参数并请求 Rust 内核重新填充；
- 编辑丝印、参考位号、极性标记、装配/Fab 文本；
- 调整 net class、普通网络约束和制造 profile 允许的规则；
- 锁定关键器件、关键走线、区域、板框和人工确认的铜皮；
- 选定局部区域或网络，要求 AI 重新布局/重新布线，未选区域默认保持不变。

所有交互式布线和移动先显示实时合法性反馈；最终几何、snap、连通性和 DRC 判断由 Rust 内核完成，不能只依赖
前端像素碰撞。

### 5.3 P1 扩展

- push-and-shove、长度调节、差分对和等长高级交互布线；
- 大规模铜皮/泪滴/过孔缝合编辑；
- 刚体模块移动、布局约束求解和多候选局部优化；
- 机械 STEP/MCAD 联动和高级 3D clearance；
- 多人审查、区域 ownership 和冲突可视化。

## 6. 编辑会话与事务模型

### 6.1 指针事件不是设计事务

鼠标移动、拖动中的每一帧只改变本地 transient preview。释放鼠标、确认属性或完成一次布线后，前端生成一个
语义命令并交给 daemon 校验，不能把数百个像素事件写入 JIR。

### 6.2 编辑会话

1. `edit_session_open(baseRevision)` 创建受租约保护的 draft branch；
2. client 查询 viewport/display list 并进行临时交互；
3. 每个逻辑动作调用 preview，返回 snap、约束、影响和即时 finding；
4. commit 将一个或一组语义动作原子写入 draft revision；
5. cheap checks 实时执行，受影响 ERC/DRC 在 commit 后增量执行；
6. 用户创建 checkpoint 或接受候选后，项目 current pointer 才移动到批准 revision；
7. session 断开时保留可恢复草稿；超时、丢锁和冲突必须显式显示。

### 6.3 撤销与重做

- 未提交的 transient preview 可直接取消；
- draft 内每个 command group 具有可逆 operation；
- 已提交 revision 的撤销创建新的 inverse revision，不删除或改写历史；
- redo 同样创建可审计 revision；
- 外部工具结果不可简单求逆时，回到父 revision 建立新 branch，并保留原结果和原因。

### 6.4 并发冲突

- 每次 commit 带 base revision 和对象级 precondition；
- base 已过期时执行语义 rebase，不能 last-write-wins；
- 同一对象/网络/区域的不可合并修改返回 typed conflict；
- 用户可选择保留人工版本、采用 AI 版本、逐字段合并或创建新候选；
- project release revision 不允许原地编辑，只能从其创建变更 branch。

## 7. AI 与人工协作规则

### 7.1 来源和保护策略

每个可编辑对象记录：

- `createdBy`: AI、human、importer、solver 或 library；
- `lastModifiedBy` 和对应 operation/revision；
- `editPolicy`: `ai-editable`、`ask-before-change`、`locked`；
- requirement、constraint、evidence 和 decision 关联；
- 用户评论、审批和未解决 finding。

AI 再次生成或优化时：

- 默认保留人工修改和所有 locked object；
- 对 `ask-before-change` 对象只生成 proposal，不自动应用；
- 必须列出将删除、重连、换料、移动或重布的人工对象；
- 用户可按 object、net、module、region 或 patch hunk 接受/拒绝；
- 不能为了让 ERC/DRC 通过而静默删除人工约束或豁免 finding；
- AI 修改后必须重新显示 semantic diff 和失效验证。

### 7.2 从画布请求 AI

用户可以选中对象后发出局部指令，例如：

- “把这一页的去耦电容整理到对应电源 pin 附近”；
- “保持连接器和安装孔不动，重新布局这个电源模块”；
- “只重布这三条网络，不修改已锁定走线”；
- “解释这个 DRC，并给出两个修复候选”；
- “将 U3 换成批准替代料，但保留接口和板框”。

工作台把稳定 object ID、selection、base revision 和允许修改范围传给 Agent，不能只发送截图坐标。

## 8. 验证反馈

| 时机 | 检查 |
| --- | --- |
| 拖动/绘制中 | snap、层/对象类型、明显碰撞、当前规则和连接预览 |
| 单个命令 preview | 类型、权限、锁、precondition、局部几何/电气合法性、影响范围 |
| draft commit | 原子 postcondition、连通性、增量 ERC/DRC、失效验证列表 |
| checkpoint | 受影响模块完整验证、JIR/JDL projection 更新、semantic diff |
| release candidate | 全量 native + reference backend ERC/DRC/DFM 与制造包门禁 |

finding 必须包含严重级别、规则、对象、位置、证据、首次出现 revision 和建议操作。用户修复后显示 finding delta，
不能简单清空旧结果。

## 9. JDP 工作台接口

建议增加以下语义接口；它们由 Rust daemon 实现，Tauri/React 只消费生成 binding：

| 接口 | 用途 | 是否写设计 |
| --- | --- | --- |
| `design_view_get` | 按 viewport/layer/LOD 返回增量 display list | 否 |
| `design_hit_test` | 坐标到稳定对象、候选选择与 snap | 否 |
| `design_cross_probe` | 在 schematic/PCB/3D/BOM/finding 间映射对象 | 否 |
| `edit_session_open` | 基于 revision 创建/恢复 draft session | 创建草稿状态 |
| `edit_command_preview` | 校验语义命令并返回影响、ghost geometry 和 finding | 否 |
| `edit_command_commit` | 原子提交 command group，创建 draft revision | 是 |
| `edit_undo` / `edit_redo` | 生成可审计 inverse/reapply revision | 是 |
| `edit_checkpoint` | 为 draft 创建命名检查点并运行指定验证 | 是 |
| `edit_session_publish` | 将批准 draft 提升为项目候选 revision | 是 |
| `design_lock_set` | 设置对象/网络/区域的 AI 修改策略 | 是 |
| `design_selection_context` | 为 Agent 生成最小语义上下文 | 否 |

语义命令至少覆盖：

- schematic：place/remove/move symbol、connect/disconnect pin、wire/bus/label、property、module/interface；
- PCB：move/rotate/flip footprint、board/region、track/via、zone、text、constraint、lock；
- 通用：comment、issue、selection set、AI edit policy、waiver request。

## 10. 性能与可用性门槛

- 普通交互目标 60 FPS；低配目标机不得长期低于批准的最低帧率；
- pan/zoom/selection 的 P95 反馈目标小于 50 ms；
- hit-test/snap 的 P95 目标小于 50 ms；
- 普通单对象 command preview 的 P95 目标小于 100 ms；
- 普通 command commit 的 P95 目标小于 300 ms，不含明确标识的长 solver/全量验证；
- 大任务不阻塞画布，显示阶段、进度、取消和最近有效 revision；
- 支持键盘操作、可配置快捷键、缩放、焦点可见性和不只依赖颜色的 finding 标识；
- 达不到性能时先采用 viewport culling、LOD、增量 display list 和 Rust 计算；仍不达标时按架构决策切换原生画布。

具体阈值必须用 10k/100k/1M 图元基准和目标硬件实测确认，以上是首轮工程目标而非未经验证的承诺。

## 11. MVP 支持矩阵

| 能力 | Phase 0 | Phase 1 MVP | Phase 2 |
| --- | --- | --- | --- |
| 原理图预览/cross-probe | 最小黄金板 | 完整 P0 | 扩展层次与协作 |
| 原理图人工编辑 | 移动、属性、连接 spike | 本文 P0 | P1 批量/高级编辑 |
| PCB 2D/层/finding 预览 | 最小黄金板 | 完整 P0 | 大板优化 |
| PCB 人工编辑 | footprint 移动、基础 track spike | 本文 P0 | 高级 push-and-shove/等长 |
| 3D 预览 | 基础模型 | cross-probe | 高级 MCAD/clearance |
| undo/redo/draft recovery | 事务原型 | 必须完成 | 多人协作增强 |
| AI patch 分块接受/拒绝 | 结构化 diff | 对象/网络/区域 | 多人审查 |
| KiCad 人工编辑 | 兼容验证 | fallback/import | 双向协作增强 |

Phase 1 不能只交付只读 viewer。若本文 P0 编辑能力未完成，产品必须标记为“生成与审查原型”，不能宣称为
可由用户修订的 AI EDA 工作台。

## 12. 验收场景

### A. AI 原理图后人工修订

AI 生成黄金板原理图。用户移动 U1、增加测试点、重命名网络并修改一个电阻值；系统生成新 revision、显示
semantic diff、只重跑受影响检查，并能在重启后恢复相同设计。

### B. 人工锁定后 AI 继续设计

用户锁定连接器、接口 pin map 和电源模块。要求 AI 整理剩余页面；AI proposal 不包含锁定对象，若必须修改则
返回冲突和原因，不静默覆盖。

### C. PCB 局部人工布线

用户在 PCB 中移动一个 footprint、手工布两条网络并增加过孔。实时预览遵循规则，commit 后 connectivity 正确，
DRC delta 能定位到具体 track/via，原理图/BOM 映射不丢失。

### D. 选区交给 AI 重做

用户框选电源区域，要求保持板框、安装孔和其他区域不动并重新布局布线。系统只生成选区内候选，显示被移动和
保留对象，用户可以逐块接受。

### E. 撤销、冲突和恢复

用户撤销已提交移动时产生 inverse revision；另一个 client 基于旧 revision 修改同一对象时收到 typed conflict；
daemon 或 UI 异常退出后可恢复未发布 draft，current revision 不被未知修改。

### F. 视图联动

用户点击 ERC/DRC finding，可定位 schematic pin 和 PCB pad/track；点击 BOM 中 U3，可同时高亮 symbol、footprint、
3D model、证据和所有相关问题。
