# 会话 04 任务书：PCB、验证与制造候选

> 建议分支：`feat/jw-04-physical-manufacturing`  
> 主要语言：Rust  
> 任务性质：实现 logical netlist → PCB 候选 → DRC → KiCad reference → 制造候选包。  
> 统一合并顺序：第 4 个合并。

## 1. 可直接交给会话的任务描述

你负责 Jingwei-Agent 的物理设计和制造域：实现 PCB/几何/约束、从规范 logical netlist 创建 board、基础布局
候选、关键网络与普通网络路由 adapter、native DRC/DFM、PCB view/edit、KiCad PCB projection/reference checks、
JLCPCB v0 profile，以及 Gerber/drill/BOM/CPL/IPC-356/STEP/manifest 制造候选。所有结果是 JIR candidate，
必须经过 Rust kernel transaction；不得把路由成功或 ZIP 生成宣称为可量产，不得创建 Production Release。

## 2. 目标与完成结果

本会话完成后，使用会话 03 定义的 logical netlist fixture 应能：

1. 创建 2 层黄金板 board 和规范 netlist；
2. 应用机械、叠层、net class、线宽/间距/过孔和制造 profile；
3. 生成确定性基础布局候选，记录算法版本/seed/metrics；
4. 生成关键网络/普通网络路由候选或通过可替换 router adapter 导入；
5. 执行 native DRC 与至少 10 类 PCB/制造检查；
6. 提供 PCB display/hit-test/cross-probe 和基础语义编辑；
7. 投影 KiCad PCB 并保存 object map/coverage；
8. 在可用环境执行 KiCad reference DRC/制造导出；
9. 生成可解析、同 revision、manifest/hash 完整的制造候选包；
10. 对故意损坏的层、drill、BOM/CPL、net/geometry 返回 blocking findings。

## 3. 开始前必读

- `docs/tasks/00-five-session-execution-plan.md`
- `docs/00-product-requirements.md`：PCB/VAL/MFG
- `docs/02-domain-model-and-artifacts.md`：Board/Constraint/Artifact/Issue
- `docs/03-eda-toolchain-and-integrations.md`：KiCad/router/manufacturer/output
- `docs/04-tool-contracts.md`：PCB/制造工具
- `docs/05-validation-and-production-gates.md`：G3–G8、PCB/制造检查
- `docs/11-native-workbench-preview-and-editing.md`：PCB 查看/编辑
- `docs/06-security-compliance-and-audit.md`

## 4. 文件所有权

本会话可以修改：

```text
crates/jingwei-geometry/**
crates/jingwei-pcb/**
crates/jingwei-placement/**
crates/jingwei-routing/**
crates/jingwei-drc/**
crates/jingwei-kicad-pcb/**
crates/jingwei-manufacturing/**
profiles/manufacturers/**
testdata/physical/**
docs/handoffs/task-04/**
docs/interface-change-proposals/task-04-*.md
```

禁止直接修改 core/daemon、logical crates、Agent runtime 和 app。需要接口变化时提交 change proposal。

正式接入应基于会话 02 的 `contract-pack-v0`；会话 03 尚未合并时只消费本文冻结的 versioned logical netlist
fixture。因本 crate manifest 导致的 `Cargo.lock` 分支差异按总任务书 4.1 处理；不得手工编辑。

## 5. 输入/输出边界

### 5.1 消费的 logical contract

只消费规范化 fixture/API，不读取会话 03 内部数据库：

```rust
struct LogicalNetlist {
    source_revision: RevisionId,
    components: Vec<ComponentInstanceRef>,
    pins: Vec<LogicalPinRef>,
    nets: Vec<LogicalNet>,
    constraints: Vec<LogicalConstraint>,
    evidence_refs: Vec<EvidenceRef>,
}
```

component 必须已有 exact footprint/pad map；缺失时阻止 PCB 创建。

### 5.2 输出

- `BoardDefinition`、stackup、net class、constraint；
- footprint/pad/net/track/via/zone/region JIR entities；
- placement/routing candidate revisions 和 metrics；
- native DRC/DFM validation runs；
- PCB display/edit/cross-probe；
- KiCad PCB projection/object map/reference report；
- manufacturing artifacts/manifest/verification；
- release readiness 缺口，但不创建 Production Release。

## 6. 几何与 PCB 模型

### 6.1 `jingwei-geometry`

至少支持：

- typed point/vector/angle/transform；
- line/arc/polyline/polygon；
- bounding box/spatial index；
- distance/intersection/containment；
- deterministic tolerance/quantization；
- board outline/holes/keepout/height regions；
- basic polygon validity；
- viewport culling 与 entity ID 映射。

不得使用裸浮点无单位；容差和坐标基准必须版本化。

### 6.2 PCB JIR

至少支持：

- board outline、cutout、mounting hole；
- layer/stackup/copper thickness；
- footprint/pad；
- net/net class；
- track/arc/via；
- zone/keepout/region；
- text/silkscreen/courtyard；
- placement/route provenance、lock/edit control；
- constraint 与 finding 引用。

## 7. 制造商 Profile

建立 versioned `jlcpcb-standard-v0`，至少包含：

- 支持层数、板厚、铜厚、表面处理基础枚举；
- 最小线宽/间距、孔径、annular ring、阻焊桥；
- 板框、槽、孔、edge clearance；
- 贴装面、最小封装/庭院、CPL 坐标/旋转映射；
- 所需输出文件名/层映射；
- profile 来源、更新时间、适用范围和 coverage gap。

profile 必须标注“用于首期黄金测试，真实下单前需工厂确认”，不能伪装为所有工艺的完整规则。

## 8. Domain tools

通过 `register_physical_module()` 暴露：

```text
pcb_create_from_schematic
pcb_profile_apply
pcb_mechanical_import
pcb_place
pcb_route_critical
pcb_autoroute
pcb_zone_fill
pcb_drc
pcb_verify_custom
pcb_render/project
design_equivalence_check       # physical/projection scope
manufacturing_profile_validate
manufacturing_export
manufacturing_package_verify
release_readiness
```

`production_release` 和 `release_revoke` 不由本模块实现；调用应返回缺少 named signoff/prototype/DFM 的明确缺口。

## 9. 布局

Phase 0 基础布局候选必须：

- 先固定 mounting hole/connector/locked footprint；
- respect outline/keepout/courtyard/side/height；
- 电源/关键回路/接口优先；
- 记录 seed、算法版本、输入 hash、cost breakdown；
- 硬冲突候选不得仅因 soft score 高而被选中；
- 输出多个 candidate 的接口可扩展，但至少有一个确定性 baseline；
- 支持局部 selection/region 重新布局且保持锁定对象。

不要求本轮实现商业级全局优化器。

## 10. 路由

### 10.1 Router adapter

```rust
trait RouterBackend {
    fn capabilities(&self) -> RouterCapabilities;
    async fn route(&self, request: RouteRequest, cancel: CancellationToken) -> RouteCandidate;
}
```

必须提供 deterministic fake/reference router fixture，支持成功、未布通、违规、取消、崩溃、未知终态。

### 10.2 Freerouting 可选 adapter

- DSN/SES 通过隔离目录导出/导入；
- 固定 binary/version/hash/args；
- CPU/memory/time/output 限制和真实取消；
- GPL-3.0 分发边界只记录，不在未完成法务决策时放入商业安装包；
- 成功退出只是候选，必须导回 JIR、native DRC、KiCad reference DRC；
- 无工具环境明确 skip，不伪造 route gate 通过。

### 10.3 关键网络

最小支持 net priority、width/via class、keepout、差分/长度字段的模型。Phase 0 可以只实现普通/电源/RS-485
基础策略，未实现差分、等长、阻抗、SI 能力要显示 coverage gap。

## 11. Native DRC/DFM

至少实现并测试 10 类：

1. copper clearance；
2. track width；
3. via/drill/annular ring；
4. unconnected/unrouted net；
5. copper/track/pad 到 board edge；
6. footprint/courtyard overlap；
7. hole/slot/NPTH 基础约束；
8. solder mask bridge/sliver 基础约束；
9. silkscreen over pad/outside board；
10. keepout/region violation；
11. connector/critical footprint position lock（可额外）；
12. return-path/test-point/assembly coverage heuristic（可额外，必须标 coverage）。

每个 finding 绑定规则版本、对象、几何位置、severity、profile/evidence 和修复建议。未实现 SI/PI/热/EMC 不能
因 DRC 为 0 被显示为“已验证”。

## 12. PCB 编辑与视图

### 语义命令

- move/rotate/flip footprint；
- edit board outline/region；
- add/move/delete track/via；
- zone/text；
- constraint/net class；
- lock/edit control。

preview 应返回 snap、ghost geometry、局部 DRC 和 affected nets；commit 返回 operation/validation scope。

### ViewProvider

- layer/viewport/LOD display list；
- footprint/pad/net/track/via/zone/entity ID；
- ratsnest/net highlight/DRC overlay；
- hit-test/snap；
- cross-probe schematic symbol/pin ↔ pad/track ↔ BOM/finding。

## 13. KiCad PCB reference backend

最低要求：

- JIR → `.kicad_pcb` projection；
- JIR ID ↔ KiCad UUID/object map；
- unknown feature/coverage report；
- parse/serialize/round-trip；
- 可用 KiCad 10.0.5 时运行真实 `pcb drc` 和制造 export；
- reference report 与 native findings 对象映射；
- 不用 GUI automation 作为 CI 必需路径；
- 不用正则拼接现有 PCB 文件。

## 14. 制造候选包

从同一 clean candidate revision 生成：

- JDL/JIR/revision manifest reference；
- KiCad compatible projection；
- Gerber layer set + Gerber Job；
- Excellon drill；
- IPC-356/netlist；
- BOM；
- CPL/PnP；
- assembly drawing/preview；
- STEP（若 backend 可用）；
- ERC/DRC/DFM reports；
- SHA-256 manifest、tool/profile/rule versions。

验证：

- 所有输出可由独立 parser 或 reference tool 重读；
- board outline 唯一且闭合；
- layer set/profile 一致；
- drill 与 JIR hole 对应；
- BOM/CPL/schematic/PCB designator 集合闭合；
- coordinates/origin/rotation/unit 明确；
- net/connectivity 与 source revision 一致；
- package 是 immutable candidate directory，不覆盖旧包。

本任务不得上传工厂、真实下单或创建 Production Release，除非用户另行明确授权。

## 15. 不在本会话范围

- Agent runtime/provider/MCP；
- DesignIntent/part/library/schematic/ERC；
- JIR transaction/store/daemon；
- Tauri/React UI；
- 商业级 push-and-shove、完整高速/RF/SI/PI/热/EMC；
- 真实工厂 DFM、样板测试、waiver/signoff/Production Release；
- 未经法务批准分发 GPL/商业工具或付费标准。

## 16. 实施步骤

1. 建 physical golden/invalid fixtures 和 logical netlist contract；
2. 实现 geometry/board/constraint/profile；
3. 实现 PCB create 与基础 placement；
4. 实现 router interface/fake backend/可选 Freerouting；
5. 实现 DRC/coverage；
6. 实现 edit/view/cross-probe；
7. 实现 KiCad PCB projection/reference；
8. 实现 manufacturing export/verify/manifest；
9. 注册 physical module，运行 fault/negative tests；
10. 生成 task-04 交接包。

## 17. 测试与验收

### 黄金正向

- 60 × 40 mm、2 层、单面 SMT、0603+、JLC standard v0；
- logical netlist 完整映射 PCB；
- locked connector/hole 保持；
- placement/routing candidate 可复现；
- native DRC error 为 0，coverage 明确；
- KiCad reference gate 如可用真实通过；
- 制造候选文件可解析，BOM/CPL/designator 闭合，manifest hash 正确。

### 负向 fixture

- outline 未闭合/多 outline；
- footprint overlap/edge violation；
- track clearance/width 违规；
- via/drill 违规；
- unrouted net；
- wrong layer/file mapping；
- stale BOM/CPL；
- rotation/origin/unit 错；
- router 取消/崩溃/unknown；
- KiCad projection 丢失对象；
- path traversal/oversized parser input。

### 建议命令

```text
cargo fmt --all -- --check
cargo clippy -p jingwei-geometry -p jingwei-pcb -p jingwei-placement -p jingwei-routing \
  -p jingwei-drc -p jingwei-kicad-pcb -p jingwei-manufacturing \
  --all-targets --all-features -- -D warnings
cargo test -p jingwei-geometry -p jingwei-pcb -p jingwei-placement -p jingwei-routing \
  -p jingwei-drc -p jingwei-kicad-pcb -p jingwei-manufacturing
git diff --check
```

真实 KiCad/router/manufacturing parser 命令、版本、退出码和报告路径写入交接，不用单元测试替代。

## 18. 必交制品

- 七个 physical/manufacturing crates；
- `jlcpcb-standard-v0` profile 与来源/coverage；
- physical module registration；
- deterministic placement/router fixture；
- 10+ DRC/DFM 规则和 coverage；
- PCB view/edit provider；
- KiCad PCB projection/object map/reference report；
- 制造候选包、manifest/hash、独立解析报告；
- `docs/handoffs/task-04/` 完整交接包；
- 接口 change proposals。

## 19. 会话完成时必须明确报告

- 哪些布局/路由是真算法，哪些是 fixture/baseline；
- KiCad/Freerouting 是否真实执行、精确版本/hash；
- native DRC 的 rule/coverage 与未覆盖高风险能力；
- 制造输出哪些由本模块生成、哪些由 KiCad reference backend 生成；
- 是否真实解析所有输出；
- 为什么结果只能称 Manufacturing Candidate；
- 会话 03 netlist、会话 02 registry、会话 05 PCB view/edit 的接入步骤。
