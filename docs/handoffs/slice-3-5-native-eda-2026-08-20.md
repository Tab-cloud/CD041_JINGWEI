# Slice 3/5 原生 EDA 纵向闭环验收

> 日期：2026-08-20
> 结论：受控逻辑黄金板与受限双层物理黄金板的正式 daemon/JDP 闭环通过；Production Release 仍阻断。

## 闭合的权威链路

- `SchematicGraph ↔ JIR` 覆盖 sheet/module/interface/symbol/pin/net/wire/bus/label/no-connect/test-point，
  embedded pin 与 standalone pin 不一致会拒绝；
- `design.view`、`design.hit_test` 和 `design.project` 从 immutable revision 读取真实 JIR，不再由前端构造图；
- 原理图 move、PCB footprint move、preview、commit、undo/redo 均通过 semantic command 和注册校验器；
- `Board ↔ JIR` 使用一个完整 root 和可寻址 child mirror，缺失、未知或过期 mirror 会 fail closed；
- PCB 创建、确定性布局、可取消 reference routing、native DRC 和 KiCad PCB equivalence 均为正式 domain tool；
- KiCad 原理图/PCB 和制造包通过 Kernel 写入 CAS；artifact write 不要求虚构 base revision，但仍进入 execution reconciliation；
- 制造包绑定同一 board hash/revision，包含 Gerber、drill、BOM、CPL、IPC-356、STEP、object map、native DRC、
  manifest/hash 与独立 package-verification attestation。

## 真实工具门禁

固定 `kicad-cli 10.0.5`，二进制 SHA-256 为
`fc142e3b4c13af868501fcbc9312dd94ad62d3c05882f97a23b6fd9f8118d0c3`。

- 逻辑黄金板：4 symbols、12 pins、6 nets；真实 ERC 0 violation，SVG 渲染成功；
- daemon 物理黄金板：真实 PCB DRC 0 violation / 0 unconnected，7 层 Gerber 和 PTH/NPTH drill 导出成功；
- 证据：`testdata/logical/golden-rs485/` 与 `testdata/physical/daemon-golden/`。

## 自动化结果

- `cargo test --workspace --all-targets`：全部通过；
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：通过；
- Runtime/package：44 pass，1 个商业 OpenAI live test 因缺少配置明确 skip；
- Workbench：16 files / 44 tests，通过 TypeScript、ESLint 和 production build；
- KiCad schematic 与 PCB 两套固定工具门禁：通过。

## 保留边界

- 逻辑黄金板的 MPN、库 verification 和 evidence 是 CC0/受控合成 fixture，不是实时供应商事实；
- generated KiCad supported subset 可 round-trip/equivalence；任意外部 KiCad unknown feature 尚不能无损导入，必须只读或拒绝；
- Freerouting 未安装，黄金板使用内置确定性 reference router；
- 当前制造包是 `L3-manufacturing-candidate-at-most`，不包含工厂 DFM、实物样板、测量或具名签核；
- 商业 OpenAI live test 缺少 `OPENAI_API_KEY`、`JINGWEI_OPENAI_MODEL`，未执行且未伪造通过。
