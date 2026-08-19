# Slice 1：真实本地控制协议与 UI adapter 交接

> 日期：2026-08-20
> 状态：Pass（Windows transport/UI adapter）
> Production Release：Blocked

## 1. 已关闭的边界缺口

- `jingwei-agent-runtime` 的生产 RPC 已从 stdio 分离到独立、认证、长度前缀 Windows named pipe；stdin 只接收
  一次性 bootstrap，stdout 只输出版本、target、health 和 `rpcEndpoint` handshake。
- `jingwei-daemon` named pipe server 可同时服务 Runtime 与工作台客户端；正式 Rust daemon、生成
  `bindings/typescript/**`、JDP trace-context receiver 和真实跨进程测试均已存在，不再沿用早期 ICP 的阻塞结论。
- Tauri Rust bridge 管理 Runtime/daemon token、endpoint 和 workspace/project 边界；WebView 只能调用受限方法，
  不获得 token、任意 pipe、shell 或 sidecar 路径。
- 安装版默认构造 `RuntimeTransportClient` 和 `DesignTransportClient`；fixture 只能由浏览器显式 `?fixture=1`
  启用，任何真实能力缺失都返回 typed error，不自动回退。
- UI 从真实 project、draft、capabilities、approval 和 durable event sequence 初始化；标题区显示 current、candidate、
  draft，且不把进程健康伪装成几何/规则证据。
- desktop-owned 退出会终止两个子进程；旧 store v1 仅在 launcher 明确传入 `--migrate-if-needed` 时备份并迁移。

## 2. 真实纵向证据

### 并发 Runtime/工作台路径

`packages/agent-runtime-sidecar/test/runtime-pipe-integration.test.ts` 启动正式 daemon 与 Runtime 二进制，通过
Runtime pipe 创建持久 session，同时由第二个 JDP 客户端创建/读取 project、打开 draft、查询实体、preview、
commit 属性修改，并证明 project head 没有因 draft commit 移动。

`crates/jingwei-daemon/src/transport.rs` 的 Windows 测试覆盖 Runtime 与 workbench 两客户端同时 handshake 和请求，
避免单连接 `serve_once` 把第二个 UI 客户端永久阻塞。

### 已安装应用

NSIS 安装器静默安装返回 0。Computer Use 验收观察到：

- 窗口标题为“精卫 Agent 工作台”；
- `REAL LOCAL RPC · JIR 0.2`、`Runtime RPC connected`、`Daemon JDP connected`；
- replay 为 `caught-up`，current/candidate/draft 都显示真实 revision；
- 创建“人工检查点”成功，toast 明确说明 current revision 未改变；
- 浅色切换到深色成功；
- 原生 display-list 未接入时明确显示“未使用 fixture”；
- 关闭应用后 workbench、Runtime、daemon 残留进程均为 0。

机器可读记录见：

- `testdata/sidecars/windows-sidecar-gate.json`
- `testdata/sidecars/windows-installed-app-gate.json`

### 旧工作区迁移

用已安装 daemon 对真实 schema v1 桌面工作区启动 `serve --migrate-if-needed`：生成完整 v1 backup、将 metadata
升级到 schema v2、输出 healthy handshake，随后正常终止且无 daemon 残留。没有该显式参数时仍保留 Slice 0 的
read-only/refusal 边界。

## 3. 验收命令与结果

- `cargo fmt --all -- --check`：Pass
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：Pass
- `cargo test --workspace --all-targets --all-features`：Pass
- `cargo test -p jingwei-daemon`：Pass（含并发 named pipe）
- `cargo test --manifest-path app/src-tauri/Cargo.toml`：13 Pass
- `cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`：Pass
- `pnpm check`：Pass；商业 OpenAI live test 1 项因缺少环境变量明确 skip
- `pnpm --dir app lint`：Pass
- `pnpm --dir app test`：42 Pass
- `pnpm --dir app build`：Pass
- `pwsh -NoProfile -File scripts/verify-windows-sidecars.ps1`：Pass
- `pnpm --dir app tauri build --bundles nsis`：Pass

## 4. 未越界声明

- `OPENAI_API_KEY` 和 `JINGWEI_OPENAI_MODEL` 均未配置，因此商业 OpenAI 端点 live test 未执行，不能声明通过。
- Runtime→daemon reconciliation 仍为 `false`，由 Slice 2 关闭。
- 原生 schematic/PCB display-list、可点击对象编辑、黄金板同 patch 三入口等价由 Slice 3 关闭。
- 安装器仍未签名；升级/回滚、SBOM、来源证明和干净机矩阵属于 Slice 7。
- 工厂 DFM、真实样板与组织签核没有外部证据，Production Release 继续阻断。
