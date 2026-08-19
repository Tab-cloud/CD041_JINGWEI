# Jingwei Agent Windows Alpha 0.1.0

> 产品名：精卫 Agent  
> 发布等级：Windows Alpha，非 Production Release

## 输出

干净、已提交的独立 Git 工作树运行 `scripts/package-windows-alpha.ps1` 后，在 `dist/` 生成：

- `Jingwei-Agent_0.1.0_x64-setup.exe`；
- `jingwei-agent.cdx.json`（CycloneDX 1.5 SBOM）；
- `THIRD_PARTY_NOTICES.md`；
- `build-provenance.json`（commit、工具版本、材料哈希和安装包哈希）；
- `windows-sidecar-gate.json`；
- `windows-alpha-package-gate.json`。

打包前必须验证 manifest 固定的 Runtime/daemon 二进制哈希、协议握手和真实 Runtime→daemon operation 目录。
脚本拒绝 dirty worktree；Tauri 构建不在打包时重建或替换已固定 sidecar，避免来源证明与提交内容漂移。

## 当前发布边界

- 安装包未配置组织 Authenticode 证书；gate 必须记录实际签名状态，不能标记为 Production Release。
- 尚未配置签名更新源和可验证回滚通道。
- 商业 OpenAI live test 缺少 `OPENAI_API_KEY` 与 `JINGWEI_OPENAI_MODEL`，因此明确跳过。
- 制造商 DFM、样板与组织 Production Release 签名不属于软件 Alpha 打包能够替代的证据。

只有签名、升级/回滚、干净机矩阵和外部硬件门禁分别通过后，才允许提升发布等级。
