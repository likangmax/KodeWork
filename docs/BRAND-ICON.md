# Kodework 应用图标

## 设计语义

- 深石墨色圆角底：Windows 开发工作台与终端环境。
- 珊瑚橙轮廓：将终端窗口、远程连接链路和原有 R 形识别合并成一个符号。
- `>_`：SSH/PTY 命令工作流。
- 薄荷绿节点：远端节点在线、Tailscale/Herdr 状态与连接成功。

图形避免文字、细线和复杂纹理，确保在 16–32 px 的任务栏、托盘和快捷方式中
仍能保持清晰轮廓。

## 资产

- 设计母版：`assets/branding/kodework-icon-master.png`
- Windows ICO：`src-tauri/icons/icon.ico`
- Tauri PNG：`32x32.png`、`64x64.png`、`128x128.png`、`128x128@2x.png`
- Windows Store 与平台扩展尺寸：由 `npx tauri icon` 从母版统一生成。

生成提示：使用内置 image generation，以原有橙色 R + 绿色节点为参考，重新设计为
终端/远程连接组合标志；最终版采用全幅深色底，避免透明棋盘格伪影。
