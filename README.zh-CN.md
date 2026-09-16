<p align="center">
  <img src="assets/branding/kodework-icon-master.png" width="112" alt="KodeWork 图标">
</p>

<h1 align="center">KodeWork</h1>

<p align="center"><strong>面向私有 Linux 主机的本地优先 Windows 远程编码工作台。</strong></p>

<p align="center">
  <a href="https://github.com/likangmax/KodeWork/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/likangmax/KodeWork/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/likangmax/KodeWork/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/likangmax/KodeWork?display_name=tag"></a>
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-green.svg"></a>
  <img alt="Windows 10/11 x64" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4">
</p>

<p align="center">
  <a href="README.md">English</a> · <strong>简体中文</strong>
</p>

<p align="center">
  <a href="https://github.com/likangmax/KodeWork/releases/latest">下载</a> ·
  <a href="docs/USER-GUIDE.zh-CN.md">使用指南</a> ·
  <a href="ROADMAP.md">路线图</a> ·
  <a href="CONTRIBUTING.md">参与贡献</a> ·
  <a href="SECURITY.md">安全</a>
</p>

KodeWork 把一台没有公网暴露的 Linux 电脑变成可恢复的远程编码工作区。它把 SSH/PTY、SFTP、Tailscale 或跳板机网络路径、tmux/Herdr 会话保持、文件与图片/PDF 上传、SSH 端口转发和 Windows 原生桌面流程整合在一起。

> 在远程 Linux 主机上开始工作，需要时随时断开，之后重新连接并继续原来的持久会话。

## 快速开始

### 1. 安装

从 [GitHub Releases](https://github.com/likangmax/KodeWork/releases/latest) 下载最新的 **Windows x64 MSI**，安装并启动 KodeWork。

首次启动会让你选择 **简体中文** 或 **English**。MSI 安装向导本身目前尚未本地化。

### 2. 添加工作站

点击 Workstations 旁边的 **+**，配置：

- Linux 用户名和地址；
- SSH 认证方式；
- 远程工作目录；
- 可选 Tailscale 或 SSH 跳板机路径；
- 可选 tmux 或 Herdr，用于持久远程会话。

第一次 SSH 连接时，请先核对服务器 Host Key 指纹，再决定是否信任。

### 3. 按指南完成配置

[中文零基础使用指南](docs/USER-GUIDE.zh-CN.md) 覆盖 Linux 准备、各种连接方式、文件传输、图片/PDF 粘贴、本机 PowerShell/CMD/WSL、持久会话、升级和常见故障排查。

## 为什么做 KodeWork

大多数 SSH 客户端的核心是“打开一个 Shell”。KodeWork 更强调把远程机器当作一个长期工作的开发工作区。

| 需求 | KodeWork 的做法 |
| --- | --- |
| 连接私有主机 | 直连/LAN、Tailscale 发现、备用地址或 SSH 跳板机 |
| 保持远端任务 | 本机断开或重启后重新附加 tmux/Herdr |
| 在一个桌面里工作 | 终端分屏、文件、传输、Actions、运行状态和 Web Preview 共用项目上下文 |
| 减少凭据进入前端状态 | 密码/私钥材料留在原生安全处理边界内 |
| 安全传输大文件 | SFTP 流式传输、暂停/继续/重试/取消、暂存后完成 |
| 预览远端 Web 服务 | SSH 本地端口转发到回环地址 Web Preview |

## 当前可用范围

当前正式分发的桌面目标是 **Windows 10/11 x64**。

| 能力 | Windows x64 | macOS 桌面 | Linux 桌面 |
| --- | --- | --- | --- |
| 可安装 KodeWork 桌面版 | **已提供 MSI** | 尚未发布 | 尚未发布 |
| 原生 GUI/安装/签名验收 | Windows 发布基线 | 尚未完成 | 尚未完成 |
| 可移植 Rust 核心 CI | 已检查 | 已检查 | 已检查 |

跨平台 Rust CI 不等于 macOS/Linux 桌面版已经发布。只有原生打包、安装、GUI、sidecar、签名/公证要求和 Release 资产都经过验证后，项目才会声明新的桌面平台可用。详见 [项目状态](docs/STATUS.md)、[发布矩阵](docs/RELEASE-MATRIX.md) 和 [公开路线图](ROADMAP.md)。

## 功能概览

| 领域 | 当前能力 |
| --- | --- |
| 网络 | 直连/备用地址、内置或系统 Tailscale 发现、SSH 跳板机 |
| 远程终端 | Rust SSH/PTY 核心、xterm.js、分屏、中文/IME、重连状态 |
| 持久会话 | tmux 与 Herdr 的发现、附加与恢复流程 |
| 认证 | 密码、公钥、SSH Agent/Pageant、Keyboard-interactive/MFA |
| 文件 | SFTP 浏览、流式传输、暂停/继续/重试/取消、固定远程目录 |
| 剪贴板与素材 | 文本复制，以及显式上传截图、图片、PDF 到当前远程工作区 |
| 本机终端 | Windows ConPTY 下的 PowerShell、CMD 和 WSL |
| 自动化 | Interactive、Quick、Background Actions，Rust 端危险级别判定 |
| 预览 | SSH 本地端口转发和回环 Web Preview |
| 桌面体验 | 中英文、主题、托盘、单实例、自启、updater 签名验证支持 |

## 安全边界

KodeWork 对以下安全边界采用显式策略：

- 未知 SSH Host Key 必须由用户确认指纹；
- 已知 Host Key 发生变化时直接阻断连接；
- 密码、私钥口令、Tailscale Auth Key 不作为普通前端/SQLite/日志数据保存；
- 危险 Actions 会在 Rust 端再次判定，不能只依赖前端；
- 远端 OSC 52 写剪贴板有长度边界，远端读取本机剪贴板则明确忽略；
- SFTP 上传保持流式和暂存式完成，不为方便而整文件读入内存；
- Web Preview 只允许用户主动建立的回环 SSH 转发。

发现疑似安全漏洞时，请不要提交公开 Issue，按 [SECURITY.md](SECURITY.md) 私下报告。

## 架构

```mermaid
flowchart TB
  UI[React + xterm.js] --> IPC[类型化 Tauri 命令 + 有界 Channel]
  Shell[Tauri 2 桌面壳] --> IPC
  IPC --> Core[kodework-core\n会话 · Run · 隧道 · 传输]
  Core --> Domain[kodework-domain\n模型 · 校验 · 危险策略]
  Core --> Adapters[SSH · SFTP · Tailscale · Herdr · 本机 PTY · 存储]
  Adapters --> Host[私有 Linux 主机\nSSH / SFTP / tmux / Herdr]
```

Rust 负责连接真相、认证边界、重连代数、传输状态和远程会话连续性；React 负责界面与渲染器生命周期。Tailscale 可以提供网络路径，但目标认证和 Host Key 校验仍由 SSH 负责。

进一步阅读：[架构说明](docs/ARCHITECTURE.md) 和编号 [ADR](docs/adr/)。

## 开发

### 环境要求

- Windows 10/11 x64（原生桌面开发）
- Rust 1.98.0 + MSVC 工具链（见 `rust-toolchain.toml`）
- Node.js 24+ 和 npm
- Tauri 2 Windows 开发依赖

### 常用命令

```powershell
npm ci
npm run dev              # 仅浏览器 UI 预览，不包含原生 SSH/凭据能力
npm run desktop          # Tauri 原生桌面开发
npm run lint
npm run test:frontend
npm run build
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
```

仓库开发规则、测试证据和 PR 要求见 [CONTRIBUTING.md](CONTRIBUTING.md)。Coding Agent 应先读 [AGENTS.md](AGENTS.md)。

## 仓库结构

```text
crates/        Rust 领域、核心编排、传输、存储和平台适配器
src-tauri/     精简 Tauri 桌面壳、类型化 IPC、插件和原生资源
src/           React 工作区、终端、文件、运行时与设置界面
docs/          用户指南、架构、ADR、测试与发布证据
scripts/       可复现构建、sidecar 和验证脚本
.github/       CI、发布自动化、Issue 表单、PR 模板
```

## 项目状态与路线图

KodeWork 当前已经可以使用，但仍是持续开发中的 `0.x` 项目。近期重点是连接/恢复可靠性、终端与传输性能、Windows 原生验收证据、可访问性和外部贡献体验。

- [当前项目状态](docs/STATUS.md)
- [公开路线图](ROADMAP.md)
- [Windows 测试矩阵](docs/TEST-MATRIX-WINDOWS.md)
- [发布矩阵](docs/RELEASE-MATRIX.md)
- [更新日志](docs/CHANGELOG.md)

## 参与贡献

欢迎范围明确的 Issue 与 Pull Request。请先读 [CONTRIBUTING.md](CONTRIBUTING.md)，优先使用结构化 Issue 表单，并确保公开内容里没有真实凭据、主机名、私人文件或签名材料。

一般问题与想法可以放到 [GitHub Discussions](https://github.com/likangmax/KodeWork/discussions)。

## 开源协议

KodeWork 使用 [MIT License](LICENSE)。内置 Tailscale 组件继续遵循上游 BSD-3-Clause License，详见 [第三方版权与许可证说明](docs/THIRD-PARTY-NOTICES.md)。
