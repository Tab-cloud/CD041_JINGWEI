# Rust 与 Tauri 跨平台实现架构

> 决策版本：1.3
> 日期：2026-08-19  
> 状态：Accepted for implementation  
> 决策：Jingwei 的 EDA 设计内核、领域工具和设计状态服务统一使用 Rust；Agent 控制面复用 Eval 的
> TypeScript/Bun runtime；首版桌面工作台采用 Tauri 2 + React/TypeScript。

## 1. 结论

Jingwei 采用 **Rust-first、daemon-owned-state、Tauri-thin-client** 架构：

- JDL 编译器、JIR、JDP、器件库、几何、ERC/DRC、布局布线、格式转换和制造发布均实现为 Rust crate；
- `jingwei-daemon` 是工作区 JIR、operation journal、资源锁和事务的唯一写入者；
- Tauri 2 负责桌面窗口、安装包、更新、sidecar 生命周期、系统集成与最小权限桥接；
- React/TypeScript 只负责视图状态、交互和可视化，不复制设计语义、校验规则或事务逻辑；
- Jingwei Agent Runtime 从 Eval 的 TypeScript/Bun runtime 抽取，是独立控制面 sidecar 和 JDP client；桌面
  UI、CLI、插件与它都不能绕过 daemon，所有设计修改走同一条协议路径；
- Windows 是首发和 Phase 0/1 强制门禁，macOS、Linux 在核心接口稳定后进入签名、安装和图形兼容矩阵。

这比“把所有逻辑直接写进 Tauri command”多一个本地服务边界，但能保证只有一个状态所有者，也让 GUI、Agent、
CLI、CI 和未来远程执行复用同一实现。

## 2. 方案评估

| 方案 | 优点 | 对 Jingwei 的主要代价 | 结论 |
| --- | --- | --- | --- |
| **Tauri 2 + React/TypeScript** | Rust 后端；复用现有 Eval/Tauri/React 经验；前端生态成熟；可打包 sidecar | 使用系统 WebView，跨平台渲染差异需测试；大型 CAD 画布需专门性能验证 | **首版采用** |
| Slint + Rust | UI 与业务均可保持 Rust；原生渲染后端；适合嵌入式或专用画布 | 通用桌面组件、团队经验和现有 React 复用较弱 | 作为原生画布或纯 Rust 客户端备选 spike |
| Flutter | Windows/macOS/Linux 桌面支持成熟；UI 一致性较好 | 引入 Dart 和 Flutter engine；Rust 内核需要额外 FFI/IPC 层 | 不作为首版主方案 |
| Electron | Web 生态和调试工具成熟；跨平台经验广泛 | 随应用分发 Chromium/Node；运行体积、更新与安全维护面更大 | 仅作 Tauri 遇到阻断时的应急备选 |
| Qt Quick | 成熟桌面/CAD 生态和高性能图形能力 | 引入 C++/QML、商业许可评估和额外绑定层 | 高端原生客户端的长期备选 |

选择 Tauri 不是把 Web 技术当作设计内核，而是把它作为可替换的展示壳。只要 JDP 稳定，未来可以增加 Slint、
Qt、Web 或专用原生客户端，而无需迁移 JIR 或重写设计工具。

## 3. 进程与所有权模型

```text
┌───────────────────────────────────────────────────────────────┐
│ Clients                                                       │
│ Tauri Workbench │ Jingwei Agent Runtime │ CLI │ CI │ Plugins  │
└──────────────────────────────┬────────────────────────────────┘
                               │ authenticated local JDP
                               │ commands / events / cancel
┌──────────────────────────────▼────────────────────────────────┐
│ jingwei-daemon (Rust, one writer per workspace)               │
│ session / auth / workspace lock / transaction / task runtime  │
├───────────────────────────────────────────────────────────────┤
│ JDL │ JIR │ JDP │ constraints │ library │ ERC/DRC │ solvers  │
│ KiCad adapters │ manufacturing │ revision/artifact services   │
└──────────────────────────────┬────────────────────────────────┘
                               │ atomic files + metadata
┌──────────────────────────────▼────────────────────────────────┐
│ Project workspace / JIR revisions / operation journal / CAS   │
└───────────────────────────────────────────────────────────────┘
```

### 3.1 唯一写入者

- 一个工作区同一时刻只允许一个有效 daemon 持有写锁；
- 所有 client 都提交带 `baseRevision`、前置条件和幂等键的 JDP transaction；
- daemon 原子提交 revision，client 不直接写 SQLite、JIR snapshot 或 projection；
- daemon 崩溃后通过 journal 和临时事务目录恢复，未提交事务不移动 current pointer；
- 只读 client 可并发；长任务通过 task ID、进度事件、取消和结构化终态管理。

### 3.2 生命周期

- 桌面模式：Tauri 启动或附着到版本匹配的 `jingwei-agent-runtime` 与 `jingwei-daemon` 两个 sidecar；
  runtime API 和 JDP 分别完成握手后才开放对应功能；
- Agent 模式：Jingwei Agent Runtime 连接同一 daemon，不通过前端转发大对象；
- CLI/CI 模式：优先连接已有 daemon；不存在时启动受工作区锁保护的 headless 实例；
- 版本不兼容时拒绝连接并给出升级/迁移操作，不尝试静默降级协议；
- 当前 Windows Phase 0 使用 `desktop-owned`：Tauri 只关闭自己启动的 runtime/daemon 子进程，并在退出时
  确认无残留；该模式不承诺 UI 关闭后长任务继续运行；
- 后续 `attached`/`headless` 模式由外部 supervisor 持有 owner lease，Tauri 只断开，不杀外部进程；支持前
  必须实现 client refcount、attach authorization、任务终态与 idle timeout；
- 两个 sidecar 独立健康、独立 epoch、独立崩溃恢复。runtime 不拥有 JIR，daemon 不拥有会话/provider 状态。

三种 owner mode 的权威定义见 `13-authoritative-boundaries-and-lifecycle.md`。不能把 future persistent mode 的
规则套到当前 desktop-owned 实现，也不能为保留任务而留下无 owner 的孤儿进程。

## 4. Rust workspace

建议从单一 Cargo workspace 开始，按稳定领域边界拆 crate，而不是过早拆成网络微服务：

```text
jingwei-agent/
  Cargo.toml
  rust-toolchain.toml
  crates/
    jingwei-jdl/               # grammar、AST、type checker、formatter
    jingwei-jir/               # entity/edge、units、constraints、migration
    jingwei-jdp/               # protocol types、query、patch、events
    jingwei-kernel/            # transaction、revision、dependency graph
    jingwei-store/             # metadata、journal、CAS、workspace lock
    jingwei-library/           # symbol、footprint、3D model
    jingwei-components/        # MPN、候选、供应数据归一化
    jingwei-geometry/          # shape、transform、spatial index、boolean ops
    jingwei-erc/               # 原理图与领域电气规则
    jingwei-drc/               # PCB、间距、连通性和制造规则
    jingwei-placement/         # 布局约束与候选
    jingwei-routing/           # 路由接口、候选与评分
    jingwei-kicad/             # AST、import/export、CLI/IPC adapter
    jingwei-manufacturing/     # Gerber、drill、BOM、CPL、manifest
    jingwei-agent-tools/       # 面向 Agent runtime 的稳定 JDP 工具合同
    jingwei-daemon/            # JDP server、auth、task runtime
    jingwei-cli/               # headless/CI client
  app/
    src/                       # React/TypeScript presentation layer
    src-tauri/                 # Tauri 2 shell、sidecar、capabilities、updater
  bindings/
    typescript/                # 从 Rust schema 生成，禁止手工维护镜像类型
  profiles/
  testdata/
  docs/
  packages/
    agent-runtime/            # 从 Eval 抽取的 session/run/provider/recovery
    agent-tools/              # 单一 registry/settlement/output bounding
    agent-policy/             # permission/approval/effects/resource claims
    agent-events/             # durable event/replay/projection
    agent-mcp/                # 受控 MCP host/client
    agent-observability/      # OpenTelemetry 与脱敏
    agent-protocol/           # runtime API schema/client
```

边界规则：

- 核心 crate 不依赖 Tauri、React、Agent runtime 或供应商 SDK；
- adapter 依赖 JIR/JDP 公共类型，JIR 不依赖 adapter 私有类型；
- `jingwei-agent-tools` 只做权限、参数和结果映射，不复制领域实现；
- `app/src-tauri` 不直接实现 ERC/DRC、库解析或设计写入；
- 可选 TypeScript/Python SDK 只生成候选 JDL/JDP，并在沙箱和 schema 校验后提交。

## 5. JDP 本地传输

### 5.1 Phase 0 选择

- Windows 使用命名管道，macOS/Linux 使用 Unix domain socket；默认不监听 localhost TCP；
- 消息使用版本化、长度前缀的 JSON 信封，规范类型由 Rust `serde` 模型生成 JSON Schema 和 TypeScript binding；
- 大型几何、渲染列表和制品通过内容寻址 artifact 引用传输，不内嵌到普通消息；
- 协议支持 request ID、idempotency key、deadline、cancel、分页、事件流和结构化错误；
- 握手协商 JDP 版本、daemon build、workspace ID、feature flags 与只读/读写能力。

Phase 0 优先可审查、可抓取和易做黄金测试的 JSON。只有 benchmark 证明序列化是瓶颈后，才为 display list 或
大批量几何增加 MessagePack/Protobuf 等二进制通道；不能让两种编码产生两套语义模型。

Runtime control API 与 JDP 是两个协议面：daemon 继续使用 JDP named pipe/UDS；Runtime 必须使用由
`agent-protocol` 定义的独立本地 endpoint。stdin 只传一次 bootstrap secret/config，当前 JSON-lines stdio RPC
仅属于 Phase 0 harness，不能作为生产多 client transport，也不能与 stdout 日志/handshake 混流。

### 5.2 本地认证

- endpoint 使用当前操作系统用户可访问的 ACL；
- 每次启动生成高熵会话密钥，通过受保护的引导通道交给合法 client，不放入命令行或日志；
- capability token 绑定 workspace、client、权限、过期时间和 daemon instance；
- 每个危险操作仍执行工具权限和领域发布门禁，连接成功不等于拥有发布权限；
- lock/endpoint 文件可记录 PID、版本和地址，但不得包含可复用明文密钥。

## 6. Tauri 工作台边界

### 6.1 Tauri 负责

- 窗口、菜单、文件选择、系统通知和安全的外部链接；
- Agent runtime/daemon 两个 sidecar 的启动、独立健康检查、更新协调和崩溃提示；
- 安装包、代码签名、自动更新和平台集成；
- capabilities/permissions、CSP、允许的本地资源协议；
- 将小型 JDP 命令和事件桥接给前端。

### 6.2 前端负责

- 需求、决策、issue、BOM、验证和发布面板；
- 可交互原理图/PCB/3D 视图、选择、测量、cross-probe、约束/finding 覆盖和 patch preview；
- 原理图与 PCB 人工编辑的输入编排、transient ghost、快捷键、属性面板和 undo/redo 呈现；
- 乐观 UI 只能用于临时交互，必须以 daemon 返回的新 revision 为准；
- UI store 只保存视图状态和缓存，不保存规范设计状态；
- 不直接调用文件系统、shell、供应商 API、KiCad 或制造发布命令。

### 6.3 不允许的捷径

- 不从 React state 直接序列化生产 JIR；
- 不在 Tauri command 中形成一套绕过 daemon 的写接口；
- 不让 WebView 直接执行任意 sidecar、shell 或工作区路径；
- 不手工复制 Rust/TypeScript 两套协议类型；
- 不把 remote web content 与本地高权限命令放在同一信任域。

## 7. CAD 渲染演进

首版保留 Web 前端，但渲染数据由 Rust 计算：

1. Rust 根据 viewport、layer、LOD 和 selection 生成增量 display list；
2. React 只管理面板和交互编排，画布使用 Canvas/WebGL/WebGPU renderer；
3. hit-test、snap、规则查询和最终坐标变更回到 Rust 内核；
4. 10k/100k/1M 图元建立帧率、显存、选择延迟和 patch 延迟基准；
5. 如果系统 WebView 在目标硬件上达不到门槛，优先把画布替换为 Rust `wgpu`/Slint 原生窗口或专用渲染面，
   不迁移 JIR/JDP 和周边工作台。

Phase 0 交付最小可验证编辑闭环，Phase 1 必须交付原理图/PCB 的 P0 人工编辑；不能把静态图片、PDF 或只读
viewer 当作工作台完成。详细范围见 `11-native-workbench-preview-and-editing.md`。性能门槛必须由黄金板和低配
目标机实测决定，不能只凭框架宣传作选择。

## 8. 跨平台打包策略

### 8.1 平台顺序

| 阶段 | 平台 | 门禁 |
| --- | --- | --- |
| Phase 0/1 | Windows 11 x64；必要时 Windows 10 x64 | 安装/卸载、WebView2、sidecar、命名管道、签名、KiCad 兼容、GPU fallback |
| Phase 2 | macOS Apple Silicon | notarization、entitlements、UDS、文件权限、Metal/WebView、KiCad 兼容 |
| Phase 2/3 | Ubuntu LTS x64 | WebKitGTK、Wayland/X11、AppImage/deb、UDS、OpenGL/Vulkan fallback |
| 后续 | Windows ARM64、macOS x64、Linux ARM64 | 依据客户与 CI 硬件需求立项 |

每个平台都构建对应 target 的 runtime/daemon/adapter sidecar，不把一个平台二进制误用于另一平台。发布清单记录 UI、
daemon、schema、KiCad、外部工具、库和 profile 版本及哈希。

### 8.2 发布门禁

- `cargo fmt --check`、严格 `clippy`、unit/property/contract/golden/fuzz 测试；
- 前端 lint/typecheck/unit/e2e 和真实 Tauri 窗口验收；
- 干净目标机安装、首次启动、升级、回滚、卸载和离线启动；
- sidecar 被替换、版本不匹配、端口/管道抢占、runtime/daemon 分别崩溃和磁盘满的负向测试；
- 安装包、sidecar 和 updater artifact 均验证签名与 manifest；
- Windows、macOS、Linux 的 UI 验收分别完成，不能用单个平台构建成功代替。

## 9. 实施顺序

### Slice A：Rust 骨架与协议

- 建立 Cargo workspace、工具链锁和 CI；
- 完成 JIR 最小实体、revision store、JDP query/patch 和 journal；
- daemon + CLI 在一个工作区完成原子 patch、冲突和恢复测试；
- 生成 JSON Schema 与 TypeScript binding。

### Slice B：Tauri 交互工作台骨架

- Tauri 安全启动/附着 sidecar；
- 展示项目、revision、JIR graph、原理图/PCB、issue 和日志；
- 完成 viewport、hit-test、cross-probe、移动 symbol/footprint 和基础连接/走线 spike；
- 验证 capabilities、CSP、外部链接和无直接文件访问。

### Slice C：统一写路径

- Agent、CLI、GUI 对同一 patch 生成完全相同的 revision hash 和语义 diff；
- 加入 draft session、semantic command preview/commit、undo/redo、对象锁、patch 分块接受、取消和崩溃恢复；
- 禁止任何 client 绕过 JDP 写工作区。

### Slice D：EDA 纵向闭环

- Rust JDL → JIR → KiCad projection → ERC/DRC → 制造包；
- Tauri 支持原理图/PCB P0 人工编辑，并展示验证差异和发布缺口；
- 在真实 Windows 安装包中完成黄金板端到端验收。

## 10. 重新评审触发条件

出现下列任一情况时重新评估 Tauri，但不重新评估 Rust 核心和 JDP 边界：

- 目标最低硬件上 PCB 画布经专项优化后仍无法达到批准的交互指标；
- 系统 WebView 的跨平台差异导致无法接受的图形正确性问题；
- 必需的原生输入、辅助功能或多窗口能力无法通过 Tauri 稳定实现；
- 代码签名、企业部署或目标平台支持形成无法规避的发布阻断；
- 纯 Rust/Slint spike 在总成本、性能和可访问性上出现数量级优势。

## 11. 官方依据

- [Tauri：嵌入外部二进制/sidecar](https://v2.tauri.app/develop/sidecar/)
- [Tauri：Security](https://v2.tauri.app/security/)
- [Tauri：Permissions](https://v2.tauri.app/security/permissions/)
- [Tauri：Updater 与签名](https://v2.tauri.app/plugin/updater/)
- [Tauri：平台 prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Slint：Desktop platforms](https://docs.slint.dev/latest/docs/slint/guide/platforms/desktop/)
- [Slint：Backends and renderers](https://docs.slint.dev/latest/docs/slint/guide/backends-and-renderers/backends_and_renderers/)
- [Flutter：Desktop support](https://docs.flutter.dev/platform-integration/desktop)
- [Electron：Process model](https://www.electronjs.org/docs/latest/tutorial/process-model)
- [Electron：Security](https://www.electronjs.org/docs/latest/tutorial/security)
- [Qt：Supported platforms](https://doc.qt.io/qt-6/supported-platforms.html)
