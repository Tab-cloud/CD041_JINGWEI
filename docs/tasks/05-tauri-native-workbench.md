# 会话 05 任务书：Tauri 原生工作台与人工编辑

> 建议分支：`feat/jw-05-workbench`  
> 主要语言：TypeScript/React + Tauri 2/Rust shell  
> 任务性质：实现可交互、可编辑、可恢复的原理图/PCB 工作台，不复制 EDA 业务逻辑。  
> 统一合并顺序：最后合并。

## 1. 可直接交给会话的任务描述

你负责 Jingwei-Agent 的桌面产品工作台：使用 Tauri 2 + React/TypeScript，实现 Agent 对话/run/tool/approval
界面、项目与阶段状态、原理图/PCB 语义画布、对象/网络/finding cross-probe、AI patch preview、人工语义编辑、
undo/redo/draft recovery 和双 sidecar 生命周期。前端只保存视图/transient state，通过 RuntimeClient 和 JDP
client 操作；不得在前端实现连通性、snap/DRC、JIR store 或制造发布逻辑。先用 versioned mock clients 并行
开发，统一合并时替换真实 transport。

## 2. 目标与完成结果

本会话完成后应有一个 Windows 首发的最小工作台：

- 可创建/打开项目，显示真实 revision 和阶段；
- 可提交用户需求并观看 durable Agent run/tool/approval 状态；
- 可回放断线前事件；
- 可查看层次原理图和 2D PCB，不以 PNG/PDF 代替对象视图；
- 可选择对象、追踪网络、定位 ERC/DRC finding；
- 可执行最小人工编辑：移动 symbol/footprint、修改属性、连接 wire、绘制基础 track/via；
- pointer 中间帧只形成 preview，commit 才产生语义 command；
- 可查看 semantic diff、按 hunk 接受/拒绝 AI proposal；
- 支持 object edit control、undo/redo/checkpoint/draft recovery；
- Tauri 管理 `jingwei-agent-runtime` 与 `jingwei-daemon` 两个独立 sidecar；
- mock/真实状态有显著区分，不展示虚假完成或“可量产”。

## 3. 开始前必读

- `docs/tasks/00-five-session-execution-plan.md`
- `docs/11-native-workbench-preview-and-editing.md`
- `docs/10-rust-tauri-platform-architecture.md`
- `docs/01-system-architecture.md`
- `docs/00-product-requirements.md`：UX/SCH/PCB/AGT
- `docs/04-tool-contracts.md`
- `docs/06-security-compliance-and-audit.md`
- `docs/12-agent-runtime-reference-architecture.md`
- `../embedded-agent/AGENTS.md`：图标、TypeScript、文件拆分与 Tauri 规范

## 4. 文件所有权

本会话可以修改：

```text
app/**
testdata/ui/**
docs/handoffs/task-05/**
docs/interface-change-proposals/task-05-*.md
```

禁止修改 runtime packages、Rust core/domain crates 和生成 binding。需要协议变化时提交 change proposal。

`pnpm-lock.yaml` 可包含仅由 `app/**` manifest 产生的本分支生成差异，统一合并按总任务书 4.1 重生成；不得据此
修改根 package/workspace 定义。会话 01/02 未合并前使用 versioned mock clients，不复制正式协议类型到 UI store。

## 5. 技术边界

### Tauri Rust shell 负责

- 窗口、菜单、文件选择、系统通知、外部链接；
- runtime/daemon 两个 sidecar 的启动/附着、版本握手、health 和崩溃提示；
- 安装/更新准备、最小 capabilities/permissions/CSP；
- 受控本地 transport bootstrap；
- 操作系统 credential reference 的最小桥接。

### React/TypeScript 负责

- UI layout、viewport、selection、panel、tab 和 transient ghost；
- RuntimeClient/JDP client 调用；
- durable event projection 的 UI 展示；
- 用户输入编排、快捷键、属性编辑、patch review；
- 性能监控和错误呈现。

### React/TypeScript 禁止

- 直接读取/写入 JIR SQLite/store/workspace；
- 自行计算规范 snap、connectivity、ERC/DRC 或最终坐标；
- 调用任意 shell、KiCad、router、供应商 API；
- 保存发布签名/供应商密钥；
- 直接把 pointer event 序列化成生产设计；
- 用本地 optimistic state 冒充 daemon 已提交 revision。

## 6. Client 接口

### 6.1 RuntimeClient

消费总任务书和会话 01 的接口：

```ts
interface RuntimeClient {
  createSession(input: CreateSessionInput): Promise<SessionView>
  admitInput(input: AdmitInput): Promise<AdmittedInput>
  resumeRun(sessionId: string): Promise<RunView>
  cancelRun(runId: string, reason?: string): Promise<void>
  events(sessionId: string, afterSequence?: number): AsyncIterable<DurableEvent<unknown>>
  listApprovals(sessionId?: string): Promise<ApprovalView[]>
  replyApproval(input: ApprovalReply): Promise<void>
  getArtifact(artifactId: string): Promise<ArtifactView>
  capabilities(): Promise<RuntimeCapabilities>
}
```

### 6.2 DesignClient

只消费生成 JDP binding，通过 facade 使用：

```ts
interface DesignClient {
  getProject(projectId: string): Promise<ProjectView>
  query<T>(request: DesignQuery): Promise<T>
  getView(request: ViewRequest): Promise<DisplayList>
  hitTest(request: HitTestRequest): Promise<HitTestResult>
  crossProbe(request: CrossProbeRequest): Promise<CrossProbeResult>
  openEditSession(request: OpenEditSession): Promise<EditSessionView>
  previewEdit(request: EditCommandGroup): Promise<EditPreview>
  commitEdit(request: EditCommandGroup): Promise<EditCommitResult>
  undo(editSessionId: string): Promise<EditCommitResult>
  redo(editSessionId: string): Promise<EditCommitResult>
  checkpoint(request: CheckpointRequest): Promise<CheckpointView>
  publishDraft(request: PublishDraftRequest): Promise<RevisionView>
  proposeAiPatch(request: AiPatchRequest): Promise<PatchProposalView>
  acceptPatchHunks(request: PatchDecision): Promise<RevisionView>
  events(aggregateId: string, afterSequence?: number): AsyncIterable<DurableEvent<unknown>>
}
```

UI 只能依赖 facade/interface，不能 import runtime/daemon 内部模块。

### 6.3 Mock 模式

必须提供 versioned mock clients，fixture 覆盖：

- normal run/tool/approval；
- provider/tool failure；
- schematic/PCB display list；
- finding/cross-probe；
- edit preview/commit/conflict；
- event disconnect/replay；
- sidecar unavailable/version mismatch；
- candidate/verified/blocked 等真实阶段。

mock UI 必须显示开发标识，不得在交付截图中冒充真实 daemon 结果。

## 7. 信息架构

建议主布局：

| 区域 | 内容 |
| --- | --- |
| 顶部 | 项目、current/draft revision、成熟度、保存/检查点、运行/取消 |
| 左侧 | 原理图层次、PCB 层、对象树、搜索、未放置/未连接队列 |
| 中央 | Schematic、PCB、3D、BOM、Manufacturing 多标签编辑区 |
| 右侧 | 属性、约束、证据、AI edit control、finding 详情 |
| 底部 | Agent 对话、run/tool timeline、ERC/DRC、semantic diff、历史/审批 |

阶段文案严格区分 Draft、Candidate、Engineering Verified、Manufacturing Candidate、Prototype Verified、
Production Released。没有证据时不得使用“可量产”“已验证”。

## 8. Agent 控制面 UI

必须提供：

- session 列表/当前 session；
- 用户输入与附件引用；
- run/step/model attempt/tool timeline；
- tool input/effect/target/状态/结构化错误；
- approval drawer/modal：目标、脱敏输入、原因、effect、有效范围、过期；
- cancel、retry-with-change、request-user；
- durable replay 和“正在重连”；
- artifact/evidence link；
- Main Agent 与 specialist run 层次，但不暴露 private child reasoning。

权限拒绝、失败和等待必须显著，不用无限 spinner 或 ambient animation代替真实状态。

## 9. 原理图画布

### P0 查看

- sheet hierarchy、pan/zoom/fit/minimap 可选；
- symbol/pin/wire/bus/label/power/net；
- selection/multi-selection；
- net highlight；
- ERC/finding overlay；
- evidence/property panel；
- stable entity ID cross-probe。

### P0 编辑

- place/remove/move symbol；
- change value/property/footprint assignment；
- connect/disconnect pin、wire/label；
- edit control lock；
- preview/commit/conflict；
- undo/redo/checkpoint。

规范 symbol geometry、snap 和 connectivity 以 daemon preview/commit 结果为准。

## 10. PCB 画布

### P0 查看

- board outline/layers/footprint/pad/net/ratsnest/track/via/zone/text；
- layer visibility/color；
- selection/net highlight；
- DRC/finding overlay；
- schematic/BOM/3D placeholder cross-probe；
- viewport/LOD/culling。

### P0 编辑

- move/rotate/flip/lock footprint；
- edit board/region 的基础交互；
- draw/move/delete track 和 via；
- constraint/property panel；
- preview 中显示 snap/局部规则结果；
- commit 后以新 draft revision 重载 affected objects；
- undo/redo/checkpoint。

不要求本轮实现商业级 push-and-shove、diff-pair/length-tuning 或完整 3D 编辑。

## 11. 人机协作

必须支持：

- `ai-editable` / `ask-before-change` / `locked`；
- 选中对象/网络/区域后发起 AI 指令；
- AI proposal 显示 base revision、affected objects/nets/rules/findings；
- 按 object/net/module/region/hunk 接受/拒绝；
- stale proposal 禁止直接应用，提示 rebase/regenerate；
- 人工修改 provenance；
- AI/solver 默认保留锁定/ask-before-change 内容。

## 12. 编辑状态模型

前端状态明确分开：

```text
viewport state
selection state
transient interaction/ghost
server-backed edit session
draft revision head
project current/candidate revision
```

pointer event → local intent → debounced/streamed preview → daemon ghost/snap/findings → command group commit → new draft
revision。只有 commit response 可以更新 draft head。

断线/重启：

- 保存 last event sequence 和 edit session ID；
- 重连先 replay；
- 查询 server-backed draft head；
- 本地 ghost 丢弃；
- stale local command 不自动重放；
- current revision 不因 UI 重启改变。

## 13. 性能与可用性目标

以黄金板：

- pan/zoom/selection 目标 60 FPS；
- hit-test P95 < 50 ms；
- 普通 command preview P95 < 100 ms；
- 普通 commit P95 < 300 ms（不含标记的长验证）；
- 10k/100k primitives 建 benchmark；
- 使用 viewport culling、LOD、incremental display list；
- WebView 不达标时记录证据，不把几何逻辑移进 React；后续可替换 renderer。

键盘、焦点、错误提示、缩放和高对比度需要基础可访问性。UI 图标必须用 SVG，不使用 emoji。

## 14. Tauri 与安全

- capabilities/permissions 默认拒绝；
- WebView 不直接 shell/filesystem；
- external links 验证 scheme 并交给系统浏览器；
- remote content 与本地 capability 隔离；
- sidecar target/version/hash/handshake 校验；
- runtime/daemon 独立 crash/health；
- endpoint/token 不进 URL/log；
- CSP 和本地资源协议最小化；
- 不把 secret 写入 localStorage/Redux/日志。

## 15. 不在本会话范围

- Agent loop/provider/tool policy 的实现；
- JIR/JDP/store/daemon；
- 器件/库/ERC/DRC/布局布线/制造逻辑；
- 通过 UI 自动操作 KiCad；
- Production Release/waiver 代签；
- macOS/Linux 正式支持声明；
- 只读 PNG/PDF viewer 作为完成替代。

## 16. 实施步骤

1. 建 Tauri 2 + React/TS app scaffold、design tokens/icons/test harness；
2. 定义 RuntimeClient/DesignClient facade 和 versioned mock clients；
3. 建项目 shell、阶段、Agent timeline、approval/diff/findings；
4. 实现 schematic display/selection/cross-probe；
5. 实现 PCB display/layer/selection/cross-probe；
6. 实现 edit session/preview/commit/undo/redo/checkpoint；
7. 实现 AI patch review/edit control；
8. 实现 event replay/reconnect；
9. 实现 Tauri 双 sidecar lifecycle/security；
10. 运行自动化、性能和真实窗口验收，生成 task-05 交接包。

## 17. 测试与验收

### 自动化

- reducer/store：revision、selection、event replay、stale update；
- client contract：mock/真实 adapter shape；
- component：approval、timeline、finding、diff；
- canvas：display list、hit-test response、layer、cross-probe；
- edit：preview 不提交、commit 更新 draft、conflict、undo/redo；
- security：no arbitrary path/shell、external link；
- sidecar：unavailable/version mismatch/crash；
- accessibility：keyboard/focus/labels；
- performance benchmark。

### 真实 Tauri/Computer Use

必须单独验收：

1. 启动 app，明确显示 mock 或真实连接；
2. 创建/打开项目；
3. 发送需求、查看 run/tool/approval；
4. 打开 schematic/PCB，选择并 cross-probe；
5. 移动 symbol/footprint、连接 wire、绘制 track/via，检查 preview/commit；
6. undo/redo、锁定对象、审查 AI patch；
7. 断开/restart client 后恢复 draft/event；
8. 截图/录像和终端日志绑定同一 revision/run。

自动化测试、浏览器页面或进程列表不能替代真实 Tauri 窗口验收。

### 建议命令

```text
pnpm install --frozen-lockfile
pnpm lint
pnpm typecheck
pnpm test
pnpm build
cargo fmt --manifest-path app/src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path app/src-tauri/Cargo.toml
pnpm tauri build --bundles nsis
git diff --check
```

NSIS 构建是本任务的打包 smoke，不等于签名正式发布；如果共享 sidecar 尚未合并，安装包需明确为 mock/dev。

## 18. 必交制品

- Tauri/React app；
- RuntimeClient/DesignClient facade；
- versioned mock servers/fixtures；
- Agent/approval/diff/finding UI；
- schematic/PCB semantic canvas；
- edit/draft/recovery/AI patch flow；
- 双 sidecar lifecycle/security 配置；
- UI unit/component/e2e/performance 结果；
- 真实 Tauri/Computer Use 证据；
- Windows NSIS smoke 或明确 blocker；
- `docs/handoffs/task-05/` 完整交接包；
- 接口 change proposals。

## 19. 会话完成时必须明确报告

- UI 当前连接 mock 还是真实 runtime/daemon；
- 哪些画布操作是真 JDP commit，哪些只是 UI prototype；
- 实测图元数量和性能；
- 真实 Tauri 窗口完成了哪些交互；
- sidecar/安装包是否包含真实二进制；
- 任何未通过的安全、安装、辅助功能或图形门禁；
- 与会话 01 RuntimeClient、会话 02 binding、会话 03/04 view/edit provider 的替换步骤。
