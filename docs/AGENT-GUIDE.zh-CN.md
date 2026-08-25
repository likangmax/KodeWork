# KodeWork Agent 与维护者指南

这份文档面向需要配置、测试、修改或发布 KodeWork 的 AI Agent、自动化脚本和维护者。目标是让一个不了解项目历史的执行者，也能安全地完成一次配置、验证或代码变更。

## 1. 先确认产品边界

- 当前公开桌面安装包是 Windows 10/11 x64 MSI，版本以仓库和 Release 的一致版本号为准。
- Rust 核心在 Windows、macOS、Linux 上做可移植性检查；这不等于 macOS/Linux 桌面安装包已经发布。
- KodeWork 的核心工作流是：私有 Linux 主机 + SSH/PTY + Tailscale 或跳板机 + Herdr/tmux 持久会话 + SFTP/文件 + 端口转发预览。
- 不能把“编译通过”写成“真实网络、GUI、安装、睡眠恢复或长时间传输已经验证”。必须标明证据类型。

## 2. 安全起步清单

1. 阅读 [文档总目录](README.md)、[架构说明](ARCHITECTURE.md) 和 [英文 Agent 指南](AGENT-GUIDE.md)。
2. 运行 git status --short，不要覆盖用户未提交的改动。
3. 运行 git log --oneline -8 和 git remote -v，确认仓库、分支和远端。
4. 用合成值做测试：IP 使用 203.0.113.10，用户名使用 testuser，不要使用真实主机、真实邮箱、真实截图或真实凭据。
5. 任何密码、SSH 私钥、Tailscale Auth Key、更新签名私钥都不能进入源码、日志、Issue、PR、截图或测试 fixture。

## 3. 工作站配置语义

| 界面字段 | 传给核心的含义 | 规则 |
| --- | --- | --- |
| 名称 | Host.label | 只用于显示，可以使用中文 |
| 地址/端口 | SSH 候选地址 | 端口通常为 22；备用地址按优先级尝试 |
| 用户名 | SSH 登录用户 | 不能写入公开日志 |
| 认证方式 | 密码、私钥、SSH Agent 或 keyboard-interactive | 私密材料只在内存/系统安全存储边界内 |
| 默认文件夹 | Project cwd | 必须是远端存在且有权限的目录 |
| 地址类型 | 手动、LAN、系统 Tailscale、内置 Tailscale | 一次只启用明确需要的路径 |
| 跳板机 | SSH ProxyJump 链 | 先验证跳板机，再验证目标机 |
| Herdr/tmux | 远端持久会话探测/附加 | 检测失败不能伪报“已配置” |

安全合成配置示例：

{
  "label": "Example private host",
  "address": "203.0.113.10",
  "port": 22,
  "username": "testuser",
  "remote_folder": "/home/testuser/workspace",
  "address_mode": "manual",
  "auth_mode": "ssh_agent",
  "jump_host": null,
  "tailscale": { "mode": "system", "auth_key": null }
}

## 4. 从零配置的可执行流程

1. 确认远端 Linux 可以通过普通 SSH 登录，且目标目录存在。
2. 在 KodeWork 中新建工作站，只填写名称、地址、端口、用户名、默认文件夹和一种认证方式。
3. 直连可用时先使用手动/LAN/公网；只有需要 Tailnet 时才选择系统或内置 Tailscale；需要隔离网络时再增加跳板机。
4. 首次连接出现 Host Key 指纹时，必须与独立可信来源核对；密钥变化必须中止连接。
5. 连接成功后验证：终端提示符、Files 默认目录、文本复制回 Windows、图片/PDF 粘贴上传、Herdr/tmux、Web Preview、断线重连。
6. 每项验证记录为 pass、fail 或 not tested，并写清环境和时间，不允许用推测替代证据。

## 5. 代码变更流程

1. 先定位边界：领域模型在 crates/kodework-domain，业务编排在 kodework-core，SSH/SFTP/平台适配器各自保持独立；UI 只负责展示和调用类型化 IPC。
2. 状态使用显式 enum 和 generation/取消语义，不新增互相矛盾的布尔状态。
3. 终端和传输数据使用有界队列/Channel；不能逐字符 IPC，也不能 read_to_end 读取大文件。
4. 前端文案必须同时更新 src/i18n.ts 的中英文键；新增按钮要有可访问名称、禁用原因和错误反馈。
5. 修改后按顺序运行格式、Clippy、Rust 测试、前端 lint、前端测试、前端 build、依赖审计和敏感信息扫描。

## 6. 发布流程

1. 更新 Cargo.toml、package.json、src-tauri/tauri.conf.json、docs/CHANGELOG.md 的同一版本号。
2. 运行 Windows 测试矩阵；MSI、updater 签名、SHA-256 和 Authenticode 状态分别记录。
3. 没有商业证书时，明确写“未进行 Authenticode 签名”；updater 签名不能替代 Windows 代码签名。
4. 发布前扫描：真实 IP/用户名、Auth Key、私钥头、token、.env、数据库、日志、target/debug 和临时压缩包。
5. GitHub 使用受保护 main：推送分支并开 PR，不 force push，不把本地旧敏感历史整体推上去。

## 7. 报告格式

每次交付至少写清：

- 变更内容和文件范围；
- 自动化测试结果；
- 真机/真实网络验证结果；
- 未验证或被环境阻塞的项目；
- 产物路径、版本、SHA-256；
- 是否已推送、是否已创建 Release；
- 仍需用户操作的下一步。

如果 GitHub 身份验证不可用，必须停在本地已提交状态并明确说明，不能声称已经发布。
