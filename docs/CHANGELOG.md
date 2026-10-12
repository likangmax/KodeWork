# Changelog

## 0.2.4 — Unreleased — Reliability, security, accessibility, and maintenance

- Chinese timeout warnings now receive the same error highlighting and 30-second display duration as English timeout messages.
- Hardened Run lifecycle/reconciliation so transport or launcher success is never misreported as remote command success; unresolved outcomes remain explicit and reconcilable.
- Strengthened SSH host-key trust ownership across direct, Tailscale, public-fallback, and jump-host paths, with fail-closed trust-store reads and native reconnect state propagation.
- Hardened SFTP resume and transfer identity with prefix verification, destination leases, source revalidation, safe `~` expansion, and more reliable pause/resume/retry/cancel behavior.
- Improved Windows ConPTY/native recovery and expanded locked full-workspace plus portable Linux/macOS core verification without claiming native macOS/Linux desktop releases.
- Remediated `RUSTSEC-2026-0285` by updating rustls, and tightened dependency-audit, tracked-secret, release-lineage, updater-signature, and Authenticode publication gates.
- Updated maintained tooling/dependencies including uuid 1.26.1, @vitejs/plugin-react 6.1.1, Vite 8.3.0, and oxlint 1.82.0; Node 26 type definitions and TypeScript 7 remain deferred to dedicated major migrations.
- Added accessible names to every current modal dialog plus a regression test that prevents unnamed dialogs from returning.
- Stable `v0.2.4` publication remains gated by the configured Tauri updater signing key and trusted Authenticode certificate; public updater hosting and native macOS/Linux bundles are still not claimed as released.

## 0.2.3 — 2026-08-18 — Guided setup, bilingual UI, and reproducible release

- Added bilingual first-run/settings/editor/terminal/local-terminal/workspace UI labels and a language switch without changing saved connection secrets.
- Added English/Chinese user and maintainer documentation covering connection fields, Tailscale modes, Herdr/tmux, files, clipboard assets, WSL, acceptance evidence, and safe release handling.
- Fixed terminal host sizing so the final row and toolbar remain visible across split panes and window resizing.
- Rebuilt the pinned Tailscale v1.102.2 sidecars from the expected upstream source revision and verified the Windows MSI plus Tauri updater signature artifact.
- This release publishes a Windows 10/11 x64 MSI only; native macOS/Linux desktop bundles, public updater hosting, and commercial Authenticode signing remain unconfigured.

## 0.2.2 — 2026-08-18 — Local terminals, release hygiene, and startup performance

- 增加独立的 Windows ConPTY 本机终端工作区：PowerShell、CMD、WSL 发行版、多标签、关闭、事件背压和 20 会话上限。
- 增加 PowerShell/CMD 真机往返、关闭后 I/O 拒绝、快速 resize 与会话上限回归测试；WSL 会严格拒绝失败的发行版探测结果。
- 本机终端渲染器改为按需加载，生产首屏主 JS 从约 554 KB 降至约 270 KB，xterm.js 不再阻塞首屏。
- 修复远程最后一个终端不能关闭、关闭后空状态误导，以及本机终端标签的嵌套交互元素问题。
- 清理公开仓库中的生成目录、内部验收交接材料和真实测试主机信息；测试 fixture 改用文档地址与通用账号。
- 当前版本引入了本机终端和公开仓库清理；具体安装包与签名状态以 0.2.3 发布说明和项目状态页为准。

## 0.2.1 — 2026-08-17 — Terminal clipboard and pinned files

- 终端选中文字后自动复制到 Windows 原生剪贴板。
- 每个终端窗格增加可见的“粘贴图片/PDF”入口，上传完成后插入安全引用的远端路径。
- 每台工作站支持固定默认远程文件目录，文件页也可一键固定当前目录。
- SQLite schema 升级到 v8，旧工作站自动迁移并默认打开 `/`。

## 0.2.0 — 2026-08-16 — Modular workbench and production authentication

- 拆分主界面为 Workspace、Terminal、Runtime、Files、Settings 独立模块；终端历史不因切换标签丢失。
- 远程文件列表改为固定行高 + overscan 虚拟滚动，目录返回上千项时 DOM 只保留可视窗口。
- Host 增加 Password、Custom Private Key、Windows SSH Agent/Pageant、Keyboard-interactive 四种明确认证模式；私钥只保存路径，口令和 MFA 响应保持一次性/凭据管理器边界。
- 增加 Windows 睡眠/网络恢复后的立即状态探测、20 PTY 并发回归测试和可配置多小时 soak 矩阵。
- 增加 updater 静态频道生成/验证、Caddy 部署配置和 Authenticode 签名接入脚本；未提供真实域名、证书或云端凭据时，发布脚本会明确失败而不伪造签名。

## 0.1.4 — 2026-08-16 — 审计收口与终端首帧修复

- 修复终端 pane 在 SSH shell 首屏输出早于 React/Tauri 订阅时丢失登录 banner/提示符；Rust Core 增加按 channel 的有界首帧回放窗口，并加入回归测试。
- WebView2 首次终端写入后强制一次有界 refresh，避免首帧 canvas 偶发空白，直到窗口 resize 才显示。
- `save_host` 不再信任 renderer 提交的凭据引用；SSH 密码与 Tailscale auth-key 引用只能由专用凭据命令维护，删除 Tailscale 配置后自动清理旧 Windows Credential Manager 条目。
- Project/Action 保存禁止跨 Host/Project 静默迁移；按 Action 查询运行记录时强制校验 Host 归属。
- 每台 Host 最多允许 16 个本地 PTY pane，异常 renderer/IPC 调用不能无限消耗 SSH channel 与远端 MaxSessions。

## 0.1.3 — 2026-08-16 — Performance hardening

- 内置 Tailscale 在工作站载入/切换后后台预热，并短时缓存健康状态；真实首次连接从约 25 秒降到 8.1–11.9 秒。
- 修复 SSH exec 的超时边界未覆盖 channel-open/exec acknowledgement，以及运行时轮询重入导致 Herdr 长时间停留“检测中…”；真实 Herdr 0.8.0 与两个 tmux 会话再次通过。
- 隐藏 Herdr/tmux 面板时停止远程轮询；避免相同数组重复 setState；终端 pane 使用 memo 隔离无关重渲染。
- SFTP 流式块从 64 KiB 提升到 256 KiB，在途写请求 8→16，SSH 接收窗口 2→8 MiB；高吞吐进度更新按时间限流。
- 断点下载改为远端 seek，不再重新传输已经存在于本地 `.part` 的前缀。
