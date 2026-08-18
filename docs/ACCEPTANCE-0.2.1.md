# Kodework Windows 0.2.1 验收记录

验收日期：2026-08-17

## 修复内容

- 终端选区变化停止 120 ms 后，将非空选区写入 Windows 原生剪贴板；相同选区不会重复写入，单次最大 4 MiB。
- 每个终端 pane 的标题栏新增“粘贴图片/PDF”按钮。Ctrl+V 与按钮共用同一条原生剪贴板读取、格式校验、SFTP 上传和远端路径插入链路。
- 图片、PDF 上传期间显示明确状态；上传全部完成后才向终端插入带 shell 引号的远端路径。
- Host 新增 `default_remote_path`；主机编辑页可以直接配置绝对远程目录，文件页也可以将当前目录一键固定。
- SQLite schema 从 v7 升至 v8，旧 Host 自动获得 `/` 默认目录；切换 Host 时立即恢复各自固定目录。

## 验证结果

以下门禁全部通过：

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --quiet
npm run lint
npm run build
git diff --check
```

本地 UI 走查确认“固定文件目录”字段可见、占位说明正确且控制台无错误。剪贴板内容可能包含用户隐私，因此自动化验收没有读取或覆盖用户当前剪贴板；原生写入接口通过编译、Clippy 与命令注册检查。

## 安装包

- 文件：[Kodework Windows_0.2.1_x64_en-US.msi](../target/release/bundle/msi/Kodework%20Windows_0.2.1_x64_en-US.msi)
- SHA-256：`0E8FD8D445D700281632C7ED1CF8BA3A34F9F68BBCEB5E0568E89369E485FF6B`
- 大小：24,768,512 bytes
- updater `.sig`：428 bytes
- Authenticode：`NotSigned`（仍需商业代码签名证书）

