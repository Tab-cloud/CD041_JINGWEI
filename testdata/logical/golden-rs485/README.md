# 温湿度 / RS-485 逻辑黄金板

该目录保存由正式 `jingwei-daemon`、生产 registry 和内容寻址 artifact store 生成的逻辑域证据，
不是前端 fixture。

- `kicad/design.kicad_sch`：四个逻辑器件、十二个引脚和六个网络的原生 KiCad 投影；
- `kicad/design.projection.json`：可解析的类型化投影模型；
- `kicad/design.object-map.json`：JIR 到 KiCad UUID 的稳定映射；
- `kicad/design.erc.json`：固定 KiCad 10.0.5 的真实 ERC 输出；
- `kicad/rendered/design.svg`：同一原理图的真实 KiCad SVG 渲染；
- `reports/kicad-10.0.5-schematic-gate.json`：版本、二进制哈希、输入哈希、ERC 和渲染门禁。

重新生成并验证：

```powershell
$env:JINGWEI_LOGICAL_GOLDEN_OUT = "$PWD\testdata\logical\golden-rs485\kicad"
cargo test -p jingwei-daemon --test logical_golden_slice
Remove-Item Env:JINGWEI_LOGICAL_GOLDEN_OUT
pwsh -NoProfile -File scripts/verify-kicad-schematic.ps1
```

器件料号和库验证状态均为受控合成评测数据，不代表真实供应商库存、生命周期或 datasheet 审批；
真实器件证据仍由供应链门禁单独约束。
