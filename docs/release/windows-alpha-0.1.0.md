# Jingwei Agent Windows Alpha 0.1.0

> 产品名：精卫 Agent  
> 发布等级：Windows Alpha，非 Production Release

## 输出

干净、已提交的独立 Git 工作树运行 `scripts/package-windows-alpha.ps1` 后，只生成一个用户交付文件：

- `dist/v0.1.0/Jingwei-Agent_0.1.0_x64-setup.exe`。

自动化验证证据写入忽略的 `target/package-evidence/v0.1.0/`，不与用户交付包混放。

打包前必须验证 manifest 固定的 Runtime/daemon 二进制哈希、协议握手和真实 Runtime→daemon operation 目录。
脚本拒绝 dirty worktree；Tauri 构建不在打包时重建或替换已固定 sidecar，避免来源证明与提交内容漂移。

## 安装与卸载占用处理

- NSIS 在复制或删除文件前探测 Workbench、Runtime 和 Daemon 三个已安装 EXE 是否被占用。
- 发现占用时，先提示用户保存编辑并请求 Workbench 正常退出，让 Tauri 生命周期处理器清理两个 sidecar。
- 对旧版本遗留的孤儿 sidecar 先尝试非强制退出，等待后才清理仍残留的精卫专属进程。
- 清理完成后再次验证三个文件均已释放；仍被占用时只能重试或取消，禁止忽略文件继续安装。
- 静默安装执行相同的清理与释放校验，无法完成时以失败退出。

## 当前发布边界

- 安装包未配置组织 Authenticode 证书；gate 必须记录实际签名状态，不能标记为 Production Release。
- 尚未配置签名更新源和可验证回滚通道。
- 商业 OpenAI live test 缺少 `OPENAI_API_KEY` 与 `JINGWEI_OPENAI_MODEL`，因此明确跳过。
- 制造商 DFM、样板与组织 Production Release 签名不属于软件 Alpha 打包能够替代的证据。

只有签名、升级/回滚、干净机矩阵和外部硬件门禁分别通过后，才允许提升发布等级。
