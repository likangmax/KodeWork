# Third-Party Notices

## Tailscale

Kodework Windows bundles the `tailscale` CLI and `tailscaled` userspace
daemon built from Tailscale v1.102.2, commit
`eb67e5dcbe145d63e1128b9b4b630f8a82da101f`.

Copyright (c) 2020 Tailscale Inc & AUTHORS All rights reserved.

Licensed under the BSD 3-Clause License. The complete upstream license is
packaged with the application as `TAILSCALE-LICENSE.txt`. The Windows named
pipe security descriptor is minimally patched by
`patches/tailscale-embedded-user-pipe.patch` so a per-user embedded daemon can
own its private pipe without administrator privileges; the upstream access
control list is otherwise unchanged.

> 更新日期：2026-08-14（M0 收尾）
> 政策依据：本项目的 MIT 许可证、各依赖的原始许可证和本文件中的逐项记录。

## 总声明

除上述固定版本 Tailscale sidecar 外，本项目（Kodework Windows）**没有复制**任何第三方应用实现代码。其他 references/ 下的仓库仅作为只读研究资料（产品形态、架构边界、错误处理与 UX 参考），不参与 Cargo/npm build，也未纳入 git 版本控制（见 .gitignore 的 /references/ 条目）。Tailscale 源码仅由发布脚本按固定 commit 获取、应用仓库内可审计补丁并编译为外部二进制。

若未来确实需要复制任何第三方代码，必须在本文件中登记：来源仓库、commit SHA、原始文件路径、许可证、修改内容与发布义务，并逐文件做许可证合规审查。

## 参考项目许可证矩阵

| 项目 | 本地目录 | 许可证 | 研究吸收点 | 不直接采用 |
|---|---|---|---|---|
| r-shell | references/r-shell | MIT | Tauri 2、Rust、PTY、SFTP、分屏、传输队列、托盘、恢复 | localhost WebSocket 作为高频终端数据通道 |
| termex | references/termex | MIT | Rust Core/Bridge 拆分、AI-native SSH 工作流、加密配置 | 未经分析的旧 Tauri/Flutter bridge |
| oryxis | references/oryxis | AGPL-3.0-or-later | 原生 Rust SSH、vault、多跳、key import、主机密钥 | 任何实现代码；闭源发行前必须进行许可证审查 |
| wrolp | references/wrolp | MIT | Tauri command 分层、russh/russh-sftp、SFTP、session recording、stale-task guard | 前端状态直接拥有网络生命周期 |
| termscp | references/termscp | MIT | Rust 文件传输、队列、暂停/恢复、书签、系统 vault、性能取向 | 将其 TUI 直接嵌入桌面 UI |
| remux | references/remux | MIT | tmux-first workspace、快捷命令、附件、图片 markup、Web preview 思路 | iOS-specific gesture/terminal core |
| tabby | references/tabby | MIT | 成熟 SSH UX、profile、jump host、agent forwarding、快捷键、主题 | Electron 架构和高内存常驻模式 |
| waveterm | references/waveterm | Apache-2.0 | durable SSH、workspace/block、远端编辑/预览、command blocks、wsh 思路 | Go/Electron runtime |
| xterm.js | references/xtermjs | MIT | VT buffer、CJK/IME、WebGL renderer、search/fit/serialize addon | 自己重写 VT parser |
| russh | references/russh | MIT | Tokio SSH、PTY、SFTP、forwarding、keepalive、认证和错误边界 | 直接暴露 russh 类型到 UI |
| herdr | references/herdr | Apache-2.0 | 常驻 server/client、workspace/tab/pane、agent 状态、CLI/socket API、远程 attach | 复制 Herdr server；Kodework 只做可选适配器 |
| ts-ssh | references/ts-ssh | MIT（TypeScript SSH 实现，结构参考） | SSH 协议边界与 DTO 设计参考 | 不引入 TypeScript SSH 运行时 |

> 注：references/ts-ssh 的许可证以该仓库自带 LICENSE 为准；如后续启用其任何设计，需在本文件补充登记。

## 开源依赖（构建时引入）

以下依赖由 Cargo/npm 管理，许可证由上游声明，构建产物中按上游要求保留 notice：

- Rust：tauri、russh（M1 引入）、russh-sftp（M1 引入）、clipboard-rs、rusqlite、tokio、serde/serde_json、thiserror、uuid、zeroize、dirs 等，见 Cargo.lock。
- JS：react、react-dom、@tauri-apps/api、vite、typescript、oxlint、xterm（M1 引入）等，见 package-lock.json。

## 复制登记表

（当前为空 — 无任何复制代码。若未来新增，按下表登记：）

| 日期 | 来源仓库 | Commit SHA | 原始文件 | 许可证 | 修改说明 | 发布义务 |
|---|---|---|---|---|---|---|
| 2026-08-16 | tailscale/tailscale | eb67e5dcbe145d63e1128b9b4b630f8a82da101f | safesocket/pipe_windows.go | BSD-3-Clause | 删除只适用于系统服务的 Builtin Administrators owner 声明，保留原 DACL，使 per-user 私有 named pipe 可创建 | 安装包携带完整 BSD-3-Clause LICENSE |
