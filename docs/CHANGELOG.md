# Changelog

## 0.2.2 — 2026-08-18 — Local terminals, release hygiene, and startup performance

- 增加独立的 Windows ConPTY 本机终端工作区：PowerShell、CMD、WSL 发行版、多标签、关闭、事件背压和 20 会话上限。
- 增加 PowerShell/CMD 真机往返、关闭后 I/O 拒绝、快速 resize 与会话上限回归测试；WSL 会严格拒绝失败的发行版探测结果。
- 本机终端渲染器改为按需加载，生产首屏主 JS 从约 554 KB 降至约 270 KB，xterm.js 不再阻塞首屏。
- 修复远程最后一个终端不能关闭、关闭后空状态误导，以及本机终端标签的嵌套交互元素问题。
- 清理公开仓库中的生成目录、内部验收交接材料和真实测试主机信息；测试 fixture 改用文档地址与通用账号。
- 当前构建已生成新的 release EXE 和 Tailscale sidecar；本机 WiX ICE 校验因自动化会话无法访问 Windows Installer，MSI 本轮未标记为通过。

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

## [Unreleased] — 2026-08-16 — 成品候选收口

### 修复与验收

- 发布版本提升到 0.1.2；修复 Tailscale 官方 `null` collection JSON、隐藏所有 sidecar
  控制台、认证超时错配、state 多实例冲突与异常退出 daemon 遗留恢复。
- 修复 SSH exec 的 EOF/exit-status 合法乱序，消除 Herdr/Actions/tmux 偶发退出码 `-1`；
  Herdr 增加登录 shell 与 Cargo/pipx/uv/Conda 安装路径发现，并延迟确认“未安装”。
- 真实远端完成内置 Tailscale → SSH/PTTY、Herdr 0.8.0/agent、tmux、SFTP 根目录、PNG/PDF
  剪贴板上传及异常终止后 daemon 清理重连验收。
- 修复 Tauri dialog/updater capability ACL，补 Web Preview loopback CSP。
- 加固 kodework-agent 令牌协议、连接并发、行长度、超时和命令输入边界。
- 移除 Windows SSH 路径的 `rsa` 可选依赖，RustSec 漏洞审计为 0 vulnerabilities。
- 完成新版工作站编辑器、Tailscale/Herdr 表单和紧凑布局打磨。
- 完成 Release MSI、更新器权限、单实例/托盘恢复和系统 Tailscale 可达性验收。
- 修复新版 Herdr CLI 检测与 agent envelope 解析；增加原生图片/PDF 剪贴板上传并
  仅在 SFTP 原子完成后粘贴远端路径；终端布局和 PTY fit 稳定性增强。
- 将固定版本 Tailscale CLI/daemon 作为 MSI externalBin 分发，补齐普通用户
  EmbeddedUserspace named-pipe 支持、运行时组件检查和许可证。
- 重新设计应用图标：保留原珊瑚橙/薄荷绿品牌识别，改为终端提示符与远程链路组合
  标志；重新生成 Windows ICO、任务栏/托盘 PNG、Store、iOS 与 Android 尺寸资源。
- 自动化覆盖范围与仍需真机验证的项目见 [TEST-MATRIX-WINDOWS.md](TEST-MATRIX-WINDOWS.md)。

## [Unreleased] — 2026-08-15 — Codex 深度审计第一阶段

### 安全与正确性

- Action 服务端确认策略、数据库权威读取、cwd/env shell 安全引用和 `~/` 展开修复。
- Background Action 迁移到 detached tmux；RunRepository 接入 Quick/Background 生命周期。
- Credential Manager“记住密码”接线；密码移出 React state，删除 Host 时清理凭据引用。
- Herdr socket 安全引用与 bridge stop 错误传播。
- Tailscale CLI stdout/stderr 并发限额读取，读取错误不再静默忽略。

### 性能与工程体验

- xterm.js 按需加载，主 bundle 从约 526 KiB 降至约 243 KiB。
- Vite 依赖扫描与 watch 排除 `references/`、`target/`，dev server 不再扫描上游研究仓库或被 Cargo 输出触发刷新。
- React lint 归零；补 Action、Background tmux、Run 持久化回归测试。

### 已验证

- fmt、clippy `-D warnings`、workspace 全测试、npm lint/build 全部通过。
- release MSI（7,475,200 bytes）与 updater `.sig` 重新生成成功。
- UI 在默认窗口、1024×700、最小窗口 960×620 下完成视觉和流程检查。

### 未完成

- 真实 auth-key 注册与真实远端 Herdr/剪贴板联调仍需在用户目标机完成；本机 daemon
  控制管道、协议和全量自动化门禁已通过。

## [0.1.0] — 2026-08-14 — M0 工程重构完成

### 新增

- Cargo workspace（12 个成员）：kodework-domain / kodework-core / kodework-storage / kodework-secrets / kodework-network / kodework-ssh / kodework-sftp / kodework-tailscale / kodework-herdr / kodework-platform-win / kodework-testkit / kodework-tauri（src-tauri）。
- 领域模型：Host/Address/Project/Action/Run/Session/Transfer/Tunnel/Snippet，UUID 类型化 ID，连接/会话/运行/传输/agent 状态机，危险命令分类，路径与端口校验。
- kodework-core：ConnectionSnapshot 状态机 + connection_generation 递增，ActionPlan 三模式（Interactive/Quick/Background）与危险确认策略。
- kodework-storage：SQLite（WAL 就绪）schema v3，幂等迁移（schema_migrations），Host/Address 往返持久化，Tailscale 配置引用与默认运行时。
- kodework-secrets：SecretStore trait + MemorySecretStore（zeroize 清零、Debug 脱敏），凭据仅存 CredentialRef 引用。
- kodework-network：候选地址排序（Tailscale > LAN > Public > JumpHost）与失败分类（认证/host key 失败不回退）。
- kodework-tailscale：tailscale status --json 容错解析（Self/Peer 官方结构、离线节点排除），SystemDaemon/EmbeddedUserspace 后端计划（不含任何 auth key）。
- kodework-herdr：argv 结构化命令构造（status/api schema），schema 容错解析，agent 状态枚举。
- kodework-sftp：TransferRequest 校验、64KiB 分块与并发上限常量、.part 路径策略。
- kodework-platform-win：LifecyclePolicy（自启/托盘/会话恢复/5 次有界指数退避 ≤30s）。
- kodework-testkit：FakeRemoteHost 故障建模。
- src-tauri：list_hosts / save_host / delete_host typed IPC，%LOCALAPPDATA%/Kodework/kodework.sqlite3。
- 前端 React 壳：Host CRUD 表单、Tailscale/运行时选择、拖拽暂存（不伪造上传）、Preview 模式隔离（浏览器不写盘）。
- Windows 打包资源：合法 ICO/PNG 图标集、tauri.conf.json（MSI bundle、CSP）。

### 修复

- npm run lint 门禁：oxlint 不再解析 references/ 与 target/（CLI --ignore-pattern）。
- LifecyclePolicy 重复实现去重：src-tauri 复用 kodework-platform-win。

### 工程化

- 建立 git 仓库（references/ 只读研究资料不纳入版本控制，见 THIRD-PARTY-NOTICES.md）。
- 补齐 docs/THIRD-PARTY-NOTICES.md、docs/ARCHITECTURE.md。

### 已验证

- cargo fmt --all -- --check：通过
- cargo clippy --workspace --all-targets --all-features -- -D warnings：通过
- cargo test --workspace --all-features：通过（12 crate 单元测试 + doc-tests）
- npm run lint：通过（0 warnings / 0 errors）
- npm run build：通过（tsc -b + vite build）

### 已知限制（M1 前）

- kodework-ssh / kodework-sftp 尚未引入 russh，无真实连接与传输。
- Windows Credential Manager / DPAPI 适配器未实现。
- 无 Tauri Channel 终端流、托盘/自启/单实例、host_keys 持久化。
- 无集成测试与故障测试（16.2/16.3 矩阵）。

## [0.1.0] — 2026-08-15 — M3 完成：语音输入 / Snippets / Yazi / Workspace Controls / kodework-agent / 签名更新器

### 新增

- Snippets：全局命令片段 CRUD（storage v6 迁移），面板运行/编辑/删除。
- Yazi：远程探测 + 终端内启动 yazi 文件管理器（pane 0）。
- Workspace Controls：Projects/Actions CRUD + run_action（危险分类 Safe/Review/Dangerous × 确认策略 Never/OnDangerous/Always，输出有界预览）。
- 语音输入：Web Speech API（SpeechRecognition/webkitSpeechRecognition），mic 按钮 pulse 动画，识别文本发送到 pane 0。
- kodework-agent（crates/kodework-agent）：可选远程 agent——JSON-over-TCP、loopback-only、token 门控、有界输出（256 KiB 截断）、exec 超时 kill、跨平台 shell（Windows cmd / Unix sh）；集成测试 spawn 真实二进制。
- 签名更新器：tauri-plugin-updater（Builder 注册 + JS 插件），minisign 密钥对（私钥/密码在 %USERPROFILE%\.tauri\，绝不入仓库），createUpdaterArtifacts=true 自动生成 MSI 的 .sig，设置弹窗"检查更新/下载并安装"，scripts/build-release.ps1 一键打包。

### 修复

- kodework-agent 测试在 Windows 上的可移植性：exec/hostname 按平台选择（cmd /C、COMPUTERNAME）。
- clippy map_flatten / unwrap_used：status.ok().and_then、测试内 unreachable!。
- tauri build 签名环境变量：TAURI_SIGNING_PRIVATE_KEY_PATH 不被 build 阶段识别，改为 TAURI_SIGNING_PRIVATE_KEY 内容变量。
- PowerShell 5.1/OneDrive 构建脚本统一使用 UTF-8 no BOM，并由门禁验证产物。

### 已验证

- cargo fmt --all -- --check：通过
- cargo clippy --workspace --all-targets --all-features -- -D warnings：通过
- cargo test --workspace --all-features：通过（100+ 测试全绿，含 agent 协议集成测试）
- npm run lint：通过（0 warnings / 0 errors）
- npm run build：通过（tsc -b + vite build）
- release 打包：MSI 7.4 MB + updater 签名 .sig 生成并验证
- UI 实机：窗口正常创建（标题 Kodework Windows）、RSS ~39 MB、第二实例被 single-instance 拦截退出

### 已知限制（如实记录）

- updater endpoints 为占位域名（未部署更新服务器）；Authenticode 代码签名未做（需商业证书）。
- 真机故障矩阵剩余项：Wi-Fi/Tailscale 真实断开、CJK/IME、20 终端并发、真实 herdr/tmux、卸载流程。
