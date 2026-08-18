# ADR-0004：原生剪贴板暂存与内置 Tailscale sidecar

- 状态：Accepted
- 日期：2026-08-16

## 背景

终端用户需要直接按 `Ctrl+V` 粘贴文本、截图、图片或 PDF。浏览器剪贴板 API
受权限和 WebView 格式支持限制，且不能安全地把本地文件交给远端。与此同时，
“内置 Tailscale”不能只依赖系统已经安装的 `tailscale.exe`；干净 Windows 环境
必须能直接启动隔离的 userspace daemon。

## 决策

1. 剪贴板读取属于平台适配器（当前 Cargo 路径仍为
   `kodework-platform-win`，以保持旧版本兼容），只接受文本、真实文件头匹配
   的 PDF 和常见图片；限制单次 16 个文件、单文件 512 MiB、总计 1 GiB。
2. 位图先写入当前用户 LocalAppData 的临时目录；原文件只读取，不修改。
3. 文件通过 `TransferManager::enqueue_and_wait` 上传到唯一的远端 `/tmp` 路径。
   SFTP `.part` 原子重命名成功后，前端才可把远端路径送入 xterm。
4. 文本调用 xterm `paste()`，从而保留 bracketed-paste 语义；不通过逐字符 IPC。
5. Tailscale v1.102.2 固定到 commit
   `eb67e5dcbe145d63e1128b9b4b630f8a82da101f`，由发布脚本构建 CLI 与 daemon，
   并作为 Tauri `externalBin` 随 MSI 分发。
6. EmbeddedUserspace 使用应用私有 state 和 control pipe，不接管系统 Tailscale
   服务。Auth Key 仍只存在 Windows Credential Manager 和短生命周期文件中。
7. 上游服务 pipe 把 Builtin Administrators 声明为 owner，普通用户无法创建。
   版本固定补丁移除 owner 声明、保留原 DACL，让 Windows 赋予当前用户 owner。

## 安全与失败语义

- 扩展名和 magic bytes 必须同时匹配；目录、相对路径和其他类型直接拒绝。
- 临时截图无论上传成功或失败都尽力删除。
- 上传失败时不向终端插入任何虚假路径。
- sidecar 源码 commit、补丁和许可证均可追溯；构建脚本遇到源树漂移立即失败。
- Host Key、SSH 认证和 Tailscale 传输路径仍为彼此独立的安全边界。

## 后果

安装包会增加约 42 MiB 未压缩二进制体积，但新电脑不再要求预装 Tailscale。
剪贴板文件上传会显示短暂进度提示；超大或不受支持的内容给出明确错误。
