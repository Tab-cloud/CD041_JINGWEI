# 会话 03 任务书：器件、库与原理图逻辑设计

> 建议分支：`feat/jw-03-logical-design`  
> 主要语言：Rust  
> 任务性质：实现 DesignIntent → 器件/库 → JIR 原理图 → native ERC 的逻辑设计域。  
> 统一合并顺序：第 3 个合并。

## 1. 可直接交给会话的任务描述

你负责 Jingwei-Agent 的逻辑设计域：实现结构化 DesignIntent、精确 MPN 候选与选型、证据/供应快照接口、
symbol/footprint/pin-pad 资产模型与验证、JIR schematic graph、原理图语义编辑、native ERC、自定义电气规则、
schematic display/hit-test 和最小 KiCad schematic projection。所有写入通过会话 02 定义的 domain module/JDP
transaction；不得直接修改 daemon store，不实现 PCB/制造，不以 OCR/LLM 猜测替代确定性验证。

## 2. 目标与完成结果

本会话完成后，使用公开黄金 fixture 应能：

1. 接收结构化黄金板 DesignIntent；
2. 对关键器件做硬约束过滤、软评分并输出选择理由；
3. 绑定有来源/许可证/hash 的 symbol 和 footprint；
4. 校验 pin-pad 映射、封装尺寸和极性；
5. 生成层次化 JIR schematic graph；
6. 执行 native ERC 和至少 10 类逻辑/电气检查；
7. 返回可定位到稳定对象 ID 的 findings/evidence；
8. 支持移动 symbol、属性和 connect/disconnect 等语义命令；
9. 提供 schematic display list、hit-test、cross-probe；
10. 输出最小 KiCad schematic projection 或明确、可测试的 projection model。

## 3. 开始前必读

- `docs/tasks/00-five-session-execution-plan.md`
- `docs/00-product-requirements.md`：REQ/CMP/LIB/SCH
- `docs/02-domain-model-and-artifacts.md`：DesignIntent/Part/Library/Schematic/Evidence
- `docs/03-eda-toolchain-and-integrations.md`：器件源、库、KiCad
- `docs/04-tool-contracts.md`：需求/器件/库/原理图工具
- `docs/05-validation-and-production-gates.md`：G0/G1/G2、库和原理图检查
- `docs/11-native-workbench-preview-and-editing.md`：原理图查看/编辑
- `docs/06-security-compliance-and-audit.md`

## 4. 文件所有权

本会话可以修改：

```text
crates/jingwei-components/**
crates/jingwei-library/**
crates/jingwei-schematic/**
crates/jingwei-erc/**
crates/jingwei-kicad-schematic/**
testdata/logical/**
docs/handoffs/task-03/**
docs/interface-change-proposals/task-03-*.md
```

禁止直接修改 `jingwei-jir/jdp/kernel/store/daemon`。需要 core 接口变化时提交 change proposal，并在本分支用
test adapter/feature shim 保持领域实现可测试。

正式接入应基于会话 02 的 `contract-pack-v0`。脚手架未到达前可以准备 fixture/算法/测试 adapter，但不得创建
第二套正式 core 类型。因本 crate manifest 导致的 `Cargo.lock` 分支差异按总任务书 4.1 处理；不得手工编辑。

## 5. 输入/输出边界

### 5.1 输入

- `DesignIntent`：已由用户/Agent 确认或标记未知的结构化需求；
- `PartSourceSnapshot`：本地公开 fixture 或经授权 supplier adapter 返回值；
- `LibraryAssetCandidate`：项目/组织/官方/第三方/自动生成候选；
- JIR base revision；
- project risk profile 和 library policy。

本模块不直接接收自然语言作为生产事实。Agent 的自然语言提取结果必须先进入 typed DesignIntent candidate。

### 5.2 输出

- `DesignIntentRevision` 和未决问题；
- `PartCandidate[]`、`PartSelectionDecision`、`ApprovedSubstitution`；
- `LibraryAsset`、`PinPadMap`、验证 findings；
- `SchematicGraph`、semantic operations、native ERC run；
- schematic display list/hit-test/cross-probe；
- KiCad schematic projection + object map + coverage；
- artifact/evidence/provenance references。

## 6. 必须实现的领域模型

### 6.1 DesignIntent

至少包含：

- product/use/environment/risk；
- input power、rails、interfaces；
- mechanical envelope；
- target manufacturer/assembly；
- component preferences/forbidden parts；
- hard constraints、soft preferences、assumptions、unknowns、conflicts；
- baseline actor/time/source/evidence。

缺少会改变安全、拓扑、关键器件或机械/制造的字段时返回定向问题，不静默填默认。

### 6.2 Part requirement/candidate/decision

```rust
struct PartRequirement {
    function: String,
    hard_constraints: Vec<Constraint>,
    soft_preferences: Vec<WeightedPreference>,
    quantity_tiers: Vec<u32>,
}

struct PartCandidate {
    mpn: ExactMpn,
    manufacturer: String,
    parameters: CanonicalValue,
    package: PackageRef,
    source_snapshot: EvidenceRef,
    hard_failures: Vec<Finding>,
    score_breakdown: Vec<ScoreComponent>,
}
```

规则：

- 只用精确 MPN 绑定设计；
- 硬失败先淘汰，软评分不能覆盖硬失败；
- 库匹配按 exact MPN/package/pin count/pad map；
- 价格/库存包含时间、数量、币种、来源和 TTL；
- NRND/EOL/单一来源/长交期形成显式 risk；
- 替代料必须重新做 pinout/package/electrical equivalence。

### 6.3 Evidence/provenance

每个外部事实保存：source URL/文档、publisher、version、retrievedAt、content hash、page/table/figure locator、
license/cache policy。网页摘要不能冒充数据手册原文。

### 6.4 Library asset

必须建模：

- symbol、footprint、3D model 类型；
- source/provenance/license/hash；
- exact MPN/package compatibility；
- pin、pad、unit、polarity、origin、courtyard、silkscreen、solder mask；
- verification/approval state；
- project-private quarantine 与 organization promotion 分离。

自动生成资产默认为 project-private candidate，不进入全局库。

### 6.5 Schematic graph

至少支持：

- sheet/module/block/interface；
- symbol/unit/pin；
- net/bus/label/power/test point/no-connect；
- component instance 和 reference designator；
- property/parameter/evidence link；
- stable ID 和层次路径；
- logical connectivity invariants。

## 7. Domain tools

通过 `register_logical_module()` 暴露以下工具 descriptor/handler：

```text
requirement_compile
requirement_baseline
requirement_change_analyze
risk_classify
part_search
part_compare
part_select
part_substitute_validate
datasheet_extract              # 只消费已有受控 artifact；网络获取可为 adapter
library_resolve
symbol_generate
footprint_generate
library_verify
library_diff
schematic_create
schematic_apply_change
schematic_connect
schematic_erc
schematic_verify_custom
schematic_render/project
```

`library_approve`、waiver 和 Production Release 只提供 policy request/缺口，不在本任务代签。

每个写工具返回 candidate operation，由 kernel transaction service 创建 revision。

## 8. 器件源和库策略

### 本轮必须

- 可重复的本地公开 catalog fixture；
- `PartSourceAdapter` trait；
- project library → approved organization library → locked official library → approved third-party 的解析顺序；
- source freshness/license 字段；
- 无网络、无凭据也能跑黄金测试。

### 可选 live adapter

- LCSC/JLC 用户提供快照导入；
- DigiKey/Mouser/Octopart 等官方 API adapter，只在账户/条款明确时实现；
- live test 无凭据时明确 skip，不抓网页绕过 API。

## 9. Symbol/Footprint 生成与验证

最小生成路径必须基于结构化 pin/package 表，不以低置信度 OCR 直接生成可发布资产。

验证至少覆盖：

- pin 数、名称、编号、类型、方向和单位；
- pad 数/编号与允许多对一映射；
- 机械尺寸、pitch、body、courtyard；
- pin 1、极性、orientation；
- 原点、旋转和 3D alignment metadata；
- solder mask/paste 和 silkscreen 基础规则；
- duplicate/missing pin-pad；
- rendered review artifact。

错误 pin-pad 必须是 blocking finding，不能用模型解释或 warning waiver 自动隐藏。

## 10. Native ERC 与自定义检查

至少实现并测试 10 类：

1. 未连接或悬空必须连接引脚；
2. output-output/驱动冲突；
3. power input 无合法 source；
4. 电压范围/绝对额定值不兼容；
5. 缺失/错误 footprint；
6. duplicate I2C address 或接口资源冲突；
7. 去耦覆盖与关键位置/值要求；
8. connector pin 与接口/保护/方向不一致；
9. pin mux/供电域/电平转换冲突；
10. test point/DFT 最低覆盖；
11. no-connect 与实际连接冲突（可作为额外）；
12. 关键计算/assertion 余量不足（可作为额外）。

每个 finding 包含 rule ID/version/severity/object refs/evidence refs/message/suggested actions/coverage。ERC error 为 0
只说明已实现规则通过，未实现能力必须作为 coverage gap 展示。

## 11. 原理图编辑与视图

### 语义命令

- place/remove/move symbol；
- connect/disconnect pin；
- wire/bus/label；
- set property/value/footprint；
- module/interface；
- no-connect/test point；
- edit control。

preview 返回 ghost geometry、snap candidate、影响对象和 cheap findings；commit handler 返回规范 operation 和
validation scope，不直接写 store。

### ViewProvider

- 层次页/viewport/LOD display list；
- symbol/pin/net/label/entity ID；
- net highlight、finding overlay；
- hit-test 和 snap candidate；
- cross-probe symbol ↔ part ↔ footprint placeholder ↔ BOM ↔ finding。

## 12. KiCad schematic reference projection

本轮最低要求：

- 生成受版本控制的 projection model 或 `.kicad_sch`；
- 保存 JIR ID ↔ KiCad UUID/object map；
- 未覆盖特性返回 coverage gap，不静默丢弃；
- parse/serialize 或 open/save round-trip 可验证；
- 如果本机存在锁定 KiCad 10.0.5，执行真实 CLI/GUI-compatible smoke；不存在时明确未通过 reference gate。

不得用正则/字符串模板在现有 KiCad 文件中替换坐标/网络。

## 13. 不在本会话范围

- Agent provider/session/tool runtime；
- JIR store、transaction、daemon/IPC；
- PCB board/placement/routing/DRC/manufacturing；
- Tauri/React UI；
- 自动批准组织库、waiver、Production Release；
- 无授权商业库抓取/再分发；
- 用 OCR/视觉低置信结果直接创建可发布 footprint。

## 14. 实施步骤

1. 建 logical testdata：黄金板 intent、catalog、datasheet structured fixture、坏库；
2. 实现 domain types/schema registration；
3. 实现 part source adapter、filter/rank/decision；
4. 实现 library resolve、pin-pad、generation/validation；
5. 实现 schematic graph 和 creation tools；
6. 实现 ERC/check coverage；
7. 实现 edit/view/hit-test/cross-probe；
8. 实现 KiCad schematic adapter/projection model；
9. 注册 domain module 并跑 contract tests；
10. 生成 task-03 交接包。

## 15. 测试与验收

### 黄金正向

- 9–24 V 输入、MCU、温湿度、非隔离 RS-485、保护、LED/SWD 的 DesignIntent；
- 候选选择有硬过滤/软评分/来源；
- 每个 component exact MPN、symbol、footprint、pin-pad 完整；
- schematic connectivity 稳定；
- native ERC/error 为 0，coverage 报告完整；
- display/cross-probe 可找到稳定 ID。

### 负向 fixture

- 错 MPN/封装；
- pin 1/polarity 错；
- pin-pad 缺失/重复；
- 越额定电压；
- power input 无 source；
- output conflict；
- 未连接；
- duplicate address；
- stale supplier snapshot；
- malicious prompt text in datasheet/library fields；
- unlicensed library promotion attempt。

### 建议命令

```text
cargo fmt --all -- --check
cargo clippy -p jingwei-components -p jingwei-library -p jingwei-schematic -p jingwei-erc \
  -p jingwei-kicad-schematic --all-targets --all-features -- -D warnings
cargo test -p jingwei-components -p jingwei-library -p jingwei-schematic -p jingwei-erc \
  -p jingwei-kicad-schematic
git diff --check
```

## 16. 必交制品

- 五个 logical domain crates；
- versioned public golden/invalid fixtures 与来源说明；
- domain module registration；
- 10+ ERC 规则、rule coverage manifest；
- schematic display/edit provider；
- KiCad schematic projection/object map 或明确的未满足 gate；
- 正负向报告和渲染 artifact；
- `docs/handoffs/task-03/` 完整交接包；
- 跨接口 change proposals。

## 17. 会话完成时必须明确报告

- 哪些器件/库数据来自真实公开来源，哪些是合成 fixture；
- 是否使用任何受限 API/库及其许可结论；
- native ERC 实际 rule/coverage；
- KiCad 10.0.5 是否真实执行；
- footprint generation 的输入证据和人工审查缺口；
- 与会话 02 registry、会话 04 logical netlist、会话 05 view/edit 的接入说明。
