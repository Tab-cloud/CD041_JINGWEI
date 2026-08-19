# Jingwei-Agent 领域模型与制品规范

> 版本：0.4  
> 日期：2026-08-19  
> 状态：待技术评审

> 实现约定：本章的 `interface`/YAML/JSON 片段用于说明 wire shape。规范领域类型由 Rust crate 定义，
> JSON Schema 与 TypeScript client binding 从 Rust 类型生成，禁止手工维护第二套模型。

## 1. 设计目标

领域模型是 Jingwei Intermediate Representation（JIR）的业务语义层。它要把自然语言、JDL、GUI 操作和
外部 EDA 导入转换为可验证对象，并保持以下链路：

```text
Requirement
  → Decision
  → FunctionalBlock
  → Part / LibraryAsset
  → SchematicObject / Net
  → PCBObject / Constraint
  → ValidationFinding
  → ManufacturingArtifact
  → ReleaseApproval
```

任一生产文件都应能反查到源 revision、设计对象、规则、生成器和签核。反之，任一硬需求也应能查询它由
哪些电路、器件、PCB 约束和测试覆盖。

JIR 是唯一规范化设计真相；JDL、KiCad、Gerber、BOM 和渲染图都是 JIR 的输入或 projection。

## 2. 标识与版本

### 2.1 标识规则

- 所有领域对象使用不可变 UUID；
- 名称、参考位号、网络名和坐标都是可变属性，不是身份；
- 外部器件使用 `manufacturer + normalized MPN` 作为业务唯一键，同时保留供应商料号；
- 库资产由 `type + logicalName + contentHash` 唯一标识；
- 制品由内容寻址哈希和 artifact ID 双重标识。

### 2.2 Revision

```ts
interface DesignRevision {
  id: string
  projectId: string
  parents: string[]
  createdAt: string
  createdBy: ActorRef
  changeSetId: string
  intentRevision: string
  toolchainLockId: string
  artifactManifestId: string
  jirSchemaVersion: string
  operationJournalHash: string
  status: "draft" | "candidate" | "verified" | "released" | "superseded"
}
```

已提交 revision 不可原地修改。JDL 编译、GUI 修改、Agent patch 和外部 KiCad 编辑导入都通过 JDP transaction
形成新 revision。

### 2.3 JIR Entity 与 Edge

```ts
interface JirEntity<TKind extends string, TProps> {
  id: string
  kind: TKind
  parentId?: string
  tags: string[]
  props: TProps
  sourceRefs: SourceRef[]
  revisionCreated: string
  revisionUpdated: string
}

interface JirEdge {
  id: string
  kind: "contains" | "connects" | "implements" | "constrained-by" |
        "derived-from" | "verified-by" | "projected-to"
  from: string
  to: string
  props?: Record<string, unknown>
}
```

对象与关系都可独立查询、引用和 diff。外部格式对象 ID 通过 projection map 关联，不进入核心身份空间。

### 2.4 Operation Journal

每个 revision 保存规范化操作序列：

```ts
interface JdpOperationRecord {
  id: string
  baseRevision: string
  actor: ActorRef
  toolExecutionId?: string
  operations: TypedDesignOperation[]
  preconditions: DesignAssertion[]
  postconditions: DesignAssertion[]
  impactedEntityIds: string[]
  resultRevision: string
  createdAt: string
}
```

operation journal 用于审计、undo/redo、branch/merge 和重放；JIR snapshot 用于快速读取。二者必须 hash 对应。

### 2.5 人工编辑会话与对象保护

```ts
interface DesignEditSession {
  id: string
  projectId: string
  actor: ActorRef
  baseProjectRevision: string
  draftHeadRevision: string
  state: "active" | "disconnected" | "conflicted" | "published" | "abandoned"
  leaseExpiresAt: string
  lastAutosaveAt: string
}

interface EditCommandGroup {
  id: string
  sessionId: string
  baseDraftRevision: string
  label: string
  commands: SemanticEditCommand[]
  inverseCommands?: SemanticEditCommand[]
  affectedEntityIds: string[]
  previewHash: string
  resultDraftRevision: string
}

interface ObjectEditControl {
  entityId: string
  createdBy: ActorRef
  lastModifiedBy: ActorRef
  lastOperationId: string
  aiEditPolicy: "ai-editable" | "ask-before-change" | "locked"
  lockedBy?: ActorRef
  lockReason?: string
}
```

- pointer/drag 中间帧属于 client transient state，不写入 `EditCommandGroup`；
- 每次逻辑编辑 commit 产生 draft revision，draft publish 后才移动项目候选 pointer；
- 已提交撤销使用 `inverseCommands` 创建新 revision，不能删除原 operation；
- `previewHash` 防止 client 在 preview 后替换命令内容；commit 时 daemon 仍需重新检查 base、权限和锁；
- AI、solver、GUI 和 importer 都尊重 `aiEditPolicy`，任何绕过都属于权限/一致性缺陷；
- session 断线可恢复，租约过期不允许继续写旧 draft，必须重新建立或语义 rebase。

## 3. DesignIntent

`DesignIntent` 是需求基线，不是模型摘要。所有可以影响安全、拓扑、器件、机械或制造的条件都应结构化。
JDL 中的 requirement/assertion 编译为这些对象，不能只保留在源文本注释中。

### 3.1 示例

```yaml
schemaVersion: 1
project:
  name: environmental-sensor-node
  targetQuantity:
    prototype: 10
    annual: 5000
  regions: [CN]
product:
  purpose: 温湿度采集并通过 RS-485 上传
  riskClass: low-voltage-general
power:
  inputs:
    - name: VIN
      nominal: 12 V
      min: 9 V
      max: 24 V
      reversePolarityProtection: required
  rails:
    - { name: V3V3, voltage: 3.3 V, maxCurrent: 500 mA }
interfaces:
  - { type: RS-485, count: 1, isolation: none, connector: screw-terminal }
  - { type: SWD, count: 1, population: debug-only }
functions:
  - id: sense-temperature
    requirement: -20 °C 至 70 °C，系统误差目标 ±0.5 °C
mechanical:
  outline: { width: 60 mm, height: 40 mm, tolerance: 0.2 mm }
  mountingHoles: 4
  maxComponentHeight: 12 mm
manufacturing:
  preferredManufacturer: jlcpcb
  layers: 2
  material: FR-4
  assemblySides: top
  minPassivePackage: 0603
sourcing:
  preferredDistributors: [LCSC, DigiKey]
  currency: CNY
  targetBomCostAtAnnualQty: 80 CNY
constraints:
  hard:
    - 现场接线端子必须有极性/信号丝印
    - 生产器件必须有精确 MPN
  soft:
    - 优先使用常备基础料
assumptions:
  - id: ambient-indoor
    value: 室内非凝露
    source: agent-default
    needsConfirmation: true
unknowns:
  - id: enclosure-drawing
    severity: blocking-before-layout
```

### 3.2 字段分级

- `hard`：不满足即淘汰方案或阻止阶段推进；
- `soft`：用于候选评分，可被用户显式权衡；
- `assumption`：透明默认值，带来源和待确认状态；
- `unknown`：缺失信息，包含最晚解决阶段；
- `conflict`：互相矛盾的输入，未解决前不能建立需求基线。

## 4. 功能架构

```ts
interface FunctionalBlock {
  id: string
  name: string
  purpose: string
  requirementRefs: string[]
  inputs: PortSpec[]
  outputs: PortSpec[]
  powerBudget: PowerBudget
  implementationCandidates: ImplementationCandidate[]
  selectedCandidateId?: string
  evidenceRefs: string[]
}
```

功能块与原理图层次页不要求一一对应，但必须保持映射。电源树、时钟树和接口拓扑是独立的一等对象。

## 5. 器件模型

### 5.1 参数需求

```ts
interface PartRequirement {
  id: string
  category: string
  blockId: string
  hardFilters: ParameterConstraint[]
  preferences: WeightedPreference[]
  packageConstraints: PackageConstraint[]
  sourcingPolicy: SourcingPolicy
  qualificationPolicy?: QualificationPolicy
}
```

### 5.2 器件候选

```ts
interface PartCandidate {
  id: string
  manufacturer: string
  mpn: string
  lifecycle?: "active" | "nrnd" | "eol" | "unknown"
  parameters: Record<string, NormalizedValue>
  packages: PackageOption[]
  offers: SupplierOfferSnapshot[]
  datasheets: EvidenceRef[]
  libraryMatches: LibraryMatch[]
  hardFilterResults: CheckResult[]
  scoreBreakdown: ScoreTerm[]
  freshness: string
  sourceRefs: string[]
}
```

价格和库存不是器件固有属性，而是按供应商、区域、币种、数量和时间记录的快照。

### 5.3 选型决定

`PartDecision` 必须包含：

- 首选精确 MPN 与封装；
- 被选中的物理/电气参数快照；
- 每个硬约束的通过证据；
- 评分明细和权重；
- 至少一个备选，或明确说明无备选的供应风险；
- 用户/策略审批；
- 何种变化会使决定失效。

### 5.4 替代料

替代关系不是简单的 MPN 列表：

```ts
interface ApprovedSubstitution {
  primaryPartId: string
  substitutePartId: string
  equivalence: {
    pinout: "exact" | "adapter-required" | "not-equivalent"
    footprint: "exact" | "compatible" | "different"
    electrical: CheckResult[]
    firmwareImpact: "none" | "config" | "driver" | "unknown"
  }
  approvedForVariants: string[]
  approvalRefs: string[]
}
```

只有 `pinout=exact`、封装允许且所有硬电气检查通过的替代料，才可在不改 PCB 的装配变体中使用。

## 6. 证据模型

```ts
interface EvidenceRef {
  id: string
  kind: "datasheet" | "reference-design" | "manufacturer-rule" | "api-snapshot" |
        "test-report" | "human-decision" | "render" | "eda-report"
  sourceUri?: string
  localArtifactId?: string
  contentHash: string
  capturedAt: string
  locator?: {
    page?: number
    section?: string
    table?: string
    figure?: string
    objectId?: string
  }
  extractionMethod?: string
  confidence?: number
  license?: string
}
```

模型推断不能伪装成数据手册证据。`confidence` 只描述提取可靠性，不代表电路正确性。

## 7. 库资产

### 7.1 LibraryAsset

```ts
interface LibraryAsset {
  id: string
  kind: "symbol" | "footprint" | "model3d"
  logicalName: string
  format: string
  contentHash: string
  source: LibraryProvenance
  partBindings: PartBinding[]
  verificationRunIds: string[]
  approvalState: "candidate" | "project-approved" | "organization-approved" | "rejected"
}
```

### 7.2 Provenance

```ts
interface LibraryProvenance {
  provider: "project" | "kicad-official" | "manufacturer" | "licensed-third-party" | "generated"
  upstreamId?: string
  upstreamVersion?: string
  sourceUrl?: string
  licenseId?: string
  retrievedAt?: string
  generator?: { name: string; version: string; inputEvidenceRefs: string[] }
}
```

### 7.3 Pin/Pad 映射

```ts
interface PinPadMapEntry {
  logicalPinId: string
  symbolUnit: string
  symbolPinNumber: string
  symbolPinName: string
  electricalType: string
  footprintPadNumbers: string[]
  packagePinName?: string
  evidenceRefs: string[]
}
```

裸露焊盘、多重 GND、内部短接焊盘和机械焊盘需要显式建模，不能因“数量不一致”简单忽略。

## 8. 原理图模型

### 8.1 SchematicGraph

JIR 保存与任何外部 EDA 文件解耦的 logical graph：

- `Sheet`、`SymbolInstance`、`PinInstance`；
- `Net`、`Bus`、`Junction`、`NoConnect`；
- `PowerDomain`、`PowerPort`、`SignalPort`；
- `Annotation`、`Calculation`、`EvidenceLink`；
- `TestPoint`、`ProtectionElement`、`ConfigurationOption`。

### 8.2 网络不变量

- 网络拥有稳定 ID，网络名可变；
- 每个 pin 的连接状态为 connected/no-connect/unknown 三态；
- 电源网络记录电压范围、电流预算和 source/sink；
- 同名网络合并必须符合层次作用域；
- 连接图来自解析后的 ECAD 源，不来自渲染图片位置推断。

## 9. PCB 模型

### 9.1 机械与叠层

```ts
interface BoardDefinition {
  outline: GeometryRef
  cutouts: GeometryRef[]
  mountingFeatures: MechanicalFeature[]
  stackup: StackupDefinition
  regions: BoardRegion[]
  origin: CoordinateOrigin
  panelization?: PanelizationRequirement
}
```

### 9.2 约束

```ts
interface PCBConstraint {
  id: string
  kind: "clearance" | "width" | "via" | "impedance" | "length" | "delay" |
        "diff-pair" | "placement" | "keepout" | "height" | "thermal" | "current" |
        "creepage" | "testability"
  scope: ObjectSelector
  severity: "hard" | "advisory"
  value: unknown
  source: "manufacturer" | "requirement" | "datasheet" | "calculation" | "human"
  evidenceRefs: string[]
}
```

高风险约束（例如阻抗、爬电距离）在对应求解/验证能力未实现时不能降级为普通 advisory。

### 9.3 布局/布线候选

每个候选记录：

- base revision、算法、版本、配置、seed、耗时；
- 硬约束结果；
- 飞线长度、过孔数、拥塞、回路面积、器件密度、热和可装配评分；
- 未布通网络、违规和需要人工复核的关键区域；
- 原生检查结果，以及投影到 KiCad 后的等价性和 DRC 报告。

评分只用于硬约束全部满足的候选之间排序。

## 10. Issue 与豁免

```ts
interface DesignIssue {
  id: string
  code: string
  severity: "error" | "warning" | "info"
  stage: string
  title: string
  objectRefs: string[]
  requirementRefs: string[]
  evidenceRefs: string[]
  fingerprint: string
  status: "open" | "fixed" | "waived" | "not-reproducible"
  waiverId?: string
}

interface Waiver {
  id: string
  issueFingerprint: string
  rationale: string
  approvedBy: ActorRef
  approvedAt: string
  scopeRevision: string
  expiresOnChange: string[]
  expiresAt?: string
}
```

豁免绑定问题指纹和 revision。对象或相关规则变化后，系统必须重新打开问题或要求重新批准。

## 11. Artifact Manifest

```yaml
schemaVersion: 1
projectId: ...
designRevision: ...
intentRevision: ...
toolchain:
  jingweiKernel: 0.1.0
  jirSchema: 0.1.0
  jdlLanguage: 0.1.0
  jdpProtocol: 0.1.0
  kicad: 10.0.5
  jingweiGenerators: 0.1.0
  router: freerouting-2.2.4
  manufacturerProfile: jlcpcb-standard-2l@2026-08-19
inputs:
  - { path: source/main.kicad_sch, sha256: ... }
  - { path: source/main.kicad_pcb, sha256: ... }
libraries:
  - { assetId: ..., sha256: ..., license: CC-BY-SA-4.0-with-KiCad-exception }
validationRuns:
  - { kind: erc, report: reports/erc.json, status: passed }
  - { kind: drc, report: reports/drc.json, status: passed }
outputs:
  - { role: gerber-copper-top, path: fabrication/main-F_Cu.gbr, sha256: ... }
  - { role: drill, path: fabrication/main-PTH.drl, sha256: ... }
approvals:
  - { role: hardware-engineer, actorId: ..., signatureRef: ... }
```

Manifest 本身也要哈希；Production Release 还应使用组织签名机制防止替换。

## 12. 建议项目制品布局

```text
project/
  jingwei.yaml                 # 项目元数据与当前指针
  design/
    main.jwd                   # JDL 规范化文本 projection
    jir.snapshot.json          # 当前 JIR 文本快照/调试投影
    operations.jsonl           # JDP operation journal
    schema.lock                # JDL/JIR/JDP 版本
  intent/
    design-intent.yaml
    decisions/
  projections/
    kicad/                     # KiCad reference backend projection
      board.kicad_pro
      board.kicad_sch
      board.kicad_pcb
  libraries/
    symbols/
    footprints/
    models3d/
    provenance/
  constraints/
    manufacturer-profile.lock
    design-rules.yaml
  evidence/
    datasheets/
    reference-designs/
    supplier-snapshots/
  reports/
    erc/
    drc/
    dfm/
    reviews/
  releases/
    candidates/<revision>/
    production/<release-id>/
  .jingwei/
    revisions/
    indexes/
    cache/
    locks/
```

`design/` 是权威源；`projections/` 可重新生成。缓存不进入发布包。供应商原始响应和数据手册是否可随项目
分发由许可证决定；不可分发时仅保存哈希、定位和受限本地引用。

## 13. 语义 Diff

Diff 不应仅展示文本行变化，至少包括：

- 需求新增/删除/修改；
- 器件 MPN、参数、封装、供应风险变化；
- symbol pin 和 footprint pad 映射变化；
- 原理图器件、网络和引脚连接变化；
- PCB 器件位置/旋转/面、走线、过孔、铜皮和规则变化；
- ERC/DRC/DFM 问题新增/解决；
- 制造文件层集、钻孔、BOM 和 CPL 变化；
- 已失效的批准与需要重新验证的测试。

JDL 文本 diff 只作为辅助；发布和审查使用基于 JIR entity/edge/constraint 的语义 diff。

## 14. 数据保留

- 草稿 revision 可按策略压缩，但保留具名决策和已引用证据；
- 所有 candidate/released revision、验证报告、manifest 和批准不可丢弃；
- 外部价格/库存快照按项目策略设 TTL，并明确显示过期；
- 删除项目时区分本地源、缓存、凭据、外部云副本和发布归档。
