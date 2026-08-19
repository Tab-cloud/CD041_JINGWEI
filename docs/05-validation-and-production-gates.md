# 验证体系与生产发布门禁

> 版本：0.3  
> 日期：2026-08-19  
> 状态：待硬件、PCB、制造和测试负责人评审

## 1. 核心原则

1. 生成成功不等于验证成功；验证成功不等于量产成功。
2. LLM 可解释和建议修复，但不能替代 ERC、DRC、解析器、测量、样板测试和具名签核。
3. 任何检查都绑定精确 design revision、toolchain lock 和规则版本。
4. error 必须修复；warning 必须修复或形成有效、具名、有范围的豁免。
5. 自动布线器、Gerber viewer、工厂上传预览都只是证据的一部分，不能单独放行。
6. 发布门禁是领域安全控制，不使用模型评分或自然语言“看起来没问题”。
7. 原生内核验证与 KiCad/reference backend 验证在首期并行；差异本身就是阻断 finding。
8. 人工编辑与 AI 编辑遵守同一门禁；旧 validation 不得覆盖修改后的新 revision。

## 2. 成熟度等级

| 等级 | 状态 | 可声称内容 | 不可声称内容 |
| --- | --- | --- | --- |
| L0 | Draft | 已形成需求/设计草稿 | 已验证、可制造 |
| L1 | ECAD Candidate | JIR revision 完整、JDL 可编译、KiCad projection 可打开 | 电气正确、PCB 合格 |
| L2 | Engineering Verified | ERC/DRC/领域检查通过并完成人工设计审查 | 工厂接受、样板可用 |
| L3 | Manufacturing Candidate | 制造包完整、可解析、一致性检查通过 | 工厂 DFM 通过、可量产 |
| L4 | Prototype Verified | 样板按测试计划通过 | 批量工艺/供应链已放行 |
| L5 | Production Released | 所有要求的 DFM、测试、供应链和具名签核有效 | 永久有效或任何变更无需复核 |

UI 必须始终显示真实等级和未完成门禁。

## 3. 阶段门禁

### G0：需求基线

通过条件：

- 风险等级已确定；
- 供电范围、接口、机械边界、环境、目标数量、制造区域已知；
- 硬约束无冲突；
- 未知项都有最晚解决阶段；
- 高风险功能已禁止自动发布或绑定专业审查；
- 用户/产品负责人确认需求基线。

阻断示例：只说“做一个电源板”但没有输入范围、输出、功率、隔离和环境。

### G1：器件与库

通过条件：

- 所有生产器件有制造商和精确 MPN；
- 所有关键参数满足硬约束并链接证据；
- 生命周期/库存/单一来源风险已展示；
- symbol/footprint/3D provenance 和许可证可接受；
- pin-to-pad mapping 完整；
- 自动生成库经独立验证和人工审查；
- 无未批准的模糊库匹配。

### G2：原理图

通过条件：

- JDL/JIR schema、类型、单位和 assertion 编译通过；
- 原生 logical ERC error 为 0；
- 锁定 KiCad projection 可打开/保存且 JIR → KiCad → JIR 无语义丢失；
- KiCad reference ERC error 为 0，且两套 ERC 差异已处置；
- 未连接 pin 已连接或显式 no-connect；
- 封装、值、MPN、关键字段完整；
- 电源树、去耦、复位、启动配置、时钟、保护和测试点检查通过；
- 关键接口/地址/电平兼容性检查通过；
- 所有 warning 已修复或具名豁免；
- 没有未发布人工 draft、未解决 typed conflict 或被绕过的对象锁；
- 人工/AI 修改来源、semantic diff 和受影响验证失效记录完整；
- 硬件工程师完成语义 diff 与原理图审查。

### G3：PCB 约束

通过条件：

- 板框、孔、连接器、禁布区和高度约束已确认；
- 制造商 profile 锁定且未过期；
- 叠层、铜厚、表面处理和网类已建立；
- 线宽/间距/via/板边/阻焊等规则不优于制造商能力；
- 电流、热、差分/时序等项目特定约束已录入或明确不适用；
- 高风险未支持约束阻止自动发布。

### G4：布局与布线

通过条件：

- 所有器件位于合法区域，无机械/高度/装配冲突；
- 连接器、LED、按键、安装孔等人机/机械位置已确认；
- 去耦、晶振、开关电源回路和保护器件位置符合规则；
- 关键网络路由和回流路径完成审查；
- 未布通网络为 0；
- Jingwei 原生 DRC error 为 0；
- KiCad reference DRC error 为 0，且两套 DRC 差异已处置；
- 所有铜皮重新填充并复测；
- 丝印、极性、Pin 1、接口标识和可读性检查通过；
- 人工移动、交互布线和局部 AI 重做均已提交 revision，选区外/锁定对象保护已验证；
- 没有未发布 PCB draft、并发冲突或过期 DRC 被显示为当前通过；
- 自动布线候选已写入隔离 JIR revision，并完成 native + KiCad 独立 DRC。

### G5：制造候选包

通过条件：

- 从同一 clean JIR revision 导出全部 source/projection 文件；
- Gerber/钻孔可由独立 parser/viewer 读取；
- 层集完整、板框唯一、单位/原点一致；
- PTH/NPTH/槽孔和 drill map 一致；
- IPC-356/导出网表与 PCB connectivity 一致；
- BOM、CPL、原理图、PCB reference designator 集合闭合；
- BOM/CPL 面、坐标、旋转、DNP 和 variant 一致；
- manifest、SHA-256、工具链、profile 和验证报告完整；
- JDL、JIR snapshot、operation journal、KiCad projection 与制造包的 revision/object mapping 完整；
- 包内无缓存、凭据、临时文件或未授权资料。

### G6：制造商 DFM

通过条件：

- 制造商成功识别层、板框、钻孔、BOM 和 CPL；
- DFM/DFT/SMT 问题已导入 issue ledger；
- 所有问题已修复或由制造工程师与工厂确认处置；
- 工厂确认的工程稿/CAM 变更可追溯并回写设计或形成批准差异；
- 最终订单参数与 profile/制造说明一致。

上传预览成功不等于 G6 通过；必须处理工厂实际反馈。

### G7：样板验证

通过条件：

- 样板对应明确 release candidate、订单和批次；
- 来料/外观/尺寸/短路开路检查完成；
- 限流上电和电源轨测量通过；
- 所有主要功能与接口按测试计划通过；
- 必要的热、负载、环境、EMC 预一致性或寿命测试完成；
- 发现的问题回写 revision 并重新走受影响门禁；
- 测试工程师签核。

### G8：生产发布

通过条件：

- G0-G7 均对当前 candidate 有效；
- AVL/替代料、采购数量和库存风险复核；
- 生产测试/烧录/校准方案可执行；
- 质量、可追溯性、序列号和返修策略明确；
- 风险 profile 要求的角色全部具名签核；
- 生成不可变、签名的 Production Release manifest。

## 4. 库验证清单

### 4.1 Symbol

- pin number/name 与制造商 pin table 一致；
- 多单元/电源单元拆分正确；
- electrical type 正确；
- hidden power pin、NC、DNU、EP 明确；
- 极性和 pin 1 标记清晰；
- Footprint、Datasheet、MPN 字段完整；
- 渲染无重叠、截断或误导性连接。

### 4.2 Footprint

- pad number、形状、尺寸和位置与封装图一致；
- pitch、body、lead、exposed pad、thermal via 正确；
- 采用制造商推荐 land pattern 或已批准标准计算；
- courtyard、fab、silkscreen、assembly、Pin 1 标记合规；
- solder mask/paste 扩展和窗口化策略明确；
- 原点、旋转和拾放中心验证；
- 3D 模型尺寸、方向和 Z 高度匹配；
- 单位、四舍五入和公差没有丢失。

### 4.3 Pin-Pad

- 每个需要装配的 symbol pin 映射到合法 pad；
- EP、多 GND、多 pad pin 的映射有显式语义；
- 机械 pad 不被当作电气 pin；
- datasheet、symbol、footprint 三方 pin 1 一致；
- 测试用故意错引脚样例必须被拦截。

## 5. 原理图领域检查

除 KiCad reference ERC 外，Jingwei 原生内核至少实现：

- 输入电压与器件绝对最大额定值/推荐范围；
- 电源 rail 的 source/sink 和预算；
- 稳压器输入/输出电容类型、值、耐压和稳定性条件；
- IC 每个电源 pin 的去耦覆盖与距离约束投影；
- reset/boot/enable strap 合法状态；
- 晶振/时钟负载与参考拓扑；
- 数字接口电压域兼容；
- I2C pull-up、电压和地址冲突；
- LED/晶体管/MOS 电阻与额定值的基本计算；
- 连接器 ESD/反接/过流保护策略；
- 调试、烧录和最低测试点覆盖；
- unused pin 的数据手册处理要求；
- 器件 MPN、封装和 BOM 字段完整性。

这些规则必须以插件形式按器件类别和项目 profile 扩展，不能全部硬编码进提示词。

## 6. PCB 领域检查

除 KiCad reference DRC 外，Jingwei 原生内核至少实现：

- 关键器件相对位置与最大距离；
- 去耦到电源/GND pin 的连接长度和过孔策略；
- 开关电源高 di/dt 回路区域与敏感网络隔离；
- 板边、安装孔、连接器和工具可达性；
- 器件 courtyard、返修和装配方向；
- 极性/Pin 1/接口丝印可见且不压焊盘；
- 走线载流能力的项目规则与余量；
- 回流路径和跨平面/分割风险；
- 测试点尺寸、间距和覆盖；
- 铜皮孤岛、意外细颈、未连接 zone；
- 热焊盘/via、热源间距和温升风险提示；
- CPL 旋转/面与封装标准方向一致性。

未实现的 SI/PI/热/EMC 能力必须作为 coverage gap 显示，不得用“DRC 通过”冒充已验证。

## 7. 制造包验证

### 7.1 文件级

- 所有清单文件存在、非空、哈希匹配；
- 文件名、扩展名和 role 映射符合 manufacturer profile；
- 压缩包可解压且无路径穿越、重复名称或隐藏临时文件；
- Gerber、drill、IPC-356、IPC-2581/ODB++ 使用独立 parser 读取；
- 坐标范围、单位、精度和 origin 合理；
- 层数量与设计叠层一致。

### 7.2 设计一致性

- Gerber 铜层 connectivity 与 PCB revision 比较；
- board outline 单一闭合，cutout/slot 可识别；
- drill hit 数和孔属性与 PCB 相符；
- mask/paste 开窗与装配器件集合相符；
- BOM designator 与 schematic/PCB/CPL 闭合；
- DNP 不进入默认装配位置或按工厂格式正确标记；
- CPL 坐标、角度、side 通过渲染叠加检查；
- fab/assembly drawing 与订单参数一致。

### 7.3 人工审查

- 用至少一个与 KiCad 独立的 Gerber viewer 查看全部层；
- 检查 top/bottom 镜像、文字、Pin 1、板框、槽孔和极性；
- 在制造商上传页面检查层识别、尺寸、钻孔、BOM/CPL 和 3D/装配预览；
- 保存截图/报告和审查人，不以“浏览过”作为无证据签核。

## 8. 供应链门禁

- 生产 BOM 所有行均有精确 MPN、manufacturer、approved supplier；
- 价格/库存快照未超过项目 TTL；
- 目标数量加损耗后库存充足或有采购计划；
- NRND/EOL/unknown 生命周期有处置；
- 单一来源、MOQ、长交期和假货风险已评估；
- 替代料经过等价性验证和签核；
- 数据源/地区/币种和日期明确；
- 工厂自动匹配后的实际料号由人确认，不能只依赖模糊匹配。

## 9. 样板测试记录

每条测试包含：

```ts
interface PrototypeTestCase {
  id: string
  requirementRefs: string[]
  releaseCandidateId: string
  unitSerials: string[]
  equipment: Array<{ name: string; assetId?: string; calibrationDue?: string }>
  setup: string
  procedure: string[]
  expected: MeasurementLimit[]
  actual: MeasurementResult[]
  result: "pass" | "fail" | "blocked"
  attachments: string[]
  executedBy: string
  executedAt: string
}
```

不能用模型生成的测试结论代替实际测量。缺少设备/样板时状态是 blocked，不是 pass。

## 10. 变更后的重验证

| 变更 | 最低重跑范围 |
| --- | --- |
| 文案/非铜丝印 | PCB DRC、制造包、人工丝印审查 |
| 被动值但同封装 | 原理图领域检查、ERC、BOM、受影响功能测试 |
| MPN 替代且同 footprint | 器件/库等价性、ERC、BOM、供应链、受影响测试 |
| footprint/封装 | 库、原理图、PCB、DRC、制造包、样板 |
| 网络连接 | ERC、PCB 更新、DRC、制造包、受影响测试 |
| 布局/布线 | DRC、PCB 领域检查、制造包、受影响 SI/EMC/功能测试 |
| 叠层/工厂 profile | PCB 规则、DRC、DFM、制造包，必要时重做样板 |
| EDA/路由器版本 | 黄金回归、全量 ERC/DRC/导出比较 |

## 11. 禁止放行的模式

- 只因为 KiCad 文件能打开；
- 只因为 Jingwei native check 通过但 reference backend 结果不一致；
- 只因为 ERC/DRC 命令退出码为 0，但报告没有读取或使用了排除项；
- 只因为自动布线器显示 100%；
- 只因为 Gerber ZIP 已创建或 viewer 能显示；
- 只因为工厂网页成功识别文件；
- 只因为 LLM/子 Agent 给出高置信度或“复核通过”；
- 用自动创建 waiver 把 error 数降为 0；
- 使用与源 revision 不一致的旧报告或旧 BOM；
- 样板/实测尚未执行却标记为通过。

## 12. 首个黄金板验收目标

建议使用“9-24 V 输入、3.3 V MCU、温湿度传感器、非隔离 RS-485、状态 LED、SWD、2 层 60 x 40 mm
单面 SMT”为首个纵向样板。最终器件与详细规格由产品评审确认。

验收：

- 从结构化需求完整生成 JDL/JIR 源工程和 KiCad reference projection；
- 至少一个器件需要自动生成并验证项目私有库；
- 至少两个供应源候选并形成首选/备选；
- ERC/DRC/自定义检查通过；
- 自动布局和布线产生可对比候选；
- JLCPCB profile 制造包被正确识别；
- 故意注入的错引脚、错封装、错 CPL 旋转、漏钻孔和 stale BOM 均被拦截；
- 真实样板完成上电、通信、传感、温升和基本功能测试后才能达到 L4。
