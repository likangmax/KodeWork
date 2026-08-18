# Kodework Windows 深度审计与收口记录（2026-08-16）

## 追加审计（0.1.4）

在真实 Windows 窗口复核中发现并修复了两类自动化测试未覆盖的问题：

- **终端首帧竞态**：`open_pane` 成功后，远端登录 banner/提示符可能先于 renderer 的
  `session_subscribe` 到达，导致首屏空白或缺少提示符。Core 现在为每个 channel 保留
  有界（256 KiB/256 事件）首帧回放窗口，并在订阅注册时原子地先回放再接收实时事件；
  xterm 首次写入后再执行一次受限 refresh，避免 WebView2 canvas 首帧不刷新。
- **本地数据完整性**：`save_host`、`project_save`、`action_save` 与 `run_list` 的 renderer
  输入边界进一步收紧，防止凭据引用篡改、Project/Action 跨工作站移动和跨 Host 运行记录
  查询。Tailscale 配置被删除后旧 Credential Manager 引用会在数据库提交成功后清理。

新增回归测试：`late_pane_subscription_replays_initial_output`。

## 结论

本轮是在 DeepSeek 交接版本之上进行的代码级、架构级、安全级、并发级、性能级和产品逻辑级收口。项目当前已经具备可交付的 Windows 远程编码工作台基础：Rust Core 与 Tauri 壳分离，SSH/PTY、SFTP、tmux、Herdr、Jump Host、端口转发、Web Preview、Actions/Runs、托盘、单实例、自启、更新签名和 Tailscale 地址发现均已接入。

本轮没有把“测试通过”表述成“绝对没有 bug”。自动化门禁只能证明当前代码在可复现测试条件下满足约束；真实网络、真实远程 Linux、真实 Tailscale、输入法、睡眠恢复和长期运行仍必须按文末清单在目标环境中执行。尤其是聊天中曾出现过的 Tailscale auth key 已视为泄露凭据，**本轮没有使用、没有写入文件、没有写入日志，也没有用它连接任何主机**。

## 本轮审计范围

- Rust workspace 的依赖方向、错误处理、状态机、并发、取消、资源生命周期和边界校验。
- russh PTY/exec/Jump Host/host-key 处理及终端事件泵。
- SFTP 流式传输、续传、暂停、取消、重试、并发槽位和完成后回收。
- Tailscale system-daemon 与 embedded userspace 的生命周期、状态隔离和 SSH 数据路径。
- Windows Credential Manager、DPAPI、临时文件和敏感数据生命周期。
- Tauri command 的权威数据来源、跨 host 隔离、危险 Action 重算和前端状态生命周期。
- React/xterm.js 的输入输出批处理、订阅释放、历史保留和 Activity 过滤。
- SQLite 迁移、WAL、Run 记录和删除 Host 时的资源清理。
- release MSI/updater 构建链路和仓库机密扫描。

## 已修复的高风险问题

### 1. 受管 Tailscale 不再只是规划模型

- 新增 `TailscaleRuntime`，可在 `EmbeddedUserspace` 模式启动与应用隔离的 `tailscaled` 子进程。
- 使用私有 state 文件和 Windows named pipe；不同 state path 不允许复用同一个活跃 daemon，避免跨 tailnet 串线。
- auth key 只从内存进入短生命周期 `file:` handoff 文件，认证完成或异常退出后删除；不会出现在 argv、React state、SQLite 或日志中。
- SSH 不绑定 Tailscale SSH 认证，而是通过 `tailscale --socket ... nc host port` 建立原始 TCP，再由 russh 完成普通 SSH host-key/密码/私钥认证。
- daemon 启动后先轮询控制面可达，再执行 `tailscale up`，避免 Windows named pipe 尚未就绪时的启动竞态。
- SystemDaemon 模式保持只读，不会擅自接管用户机器上的 Tailscale 账号。

### 2. SSH/PTY/事件泵资源生命周期

- `disconnect` 现在完整关闭 SSH connection、PTY panes、SFTP、transfer subscriptions 和 host tunnels，且可重复调用。
- connect/disconnect 使用 per-host guard 串行化，避免快速点击产生交叉连接和旧状态覆盖。
- exec/SFTP/forward channel 关闭或失败时释放过滤集合，避免 channel id 泄漏和终端输出串扰。
- 事件泵清理已关闭订阅者，可靠 primary 使用有序发送，镜像订阅使用有界 try-send；无界 channel 不再作为数据平面。
- 旧连接 generation 的事件不能覆盖新连接状态。
- 代理命令子进程隐藏 Windows 控制台，并在流释放时终止，避免后台残留进程。

### 3. SFTP 传输可靠性

- 本地/远端路径拒绝控制字符，远端支持 `/` 和 `~/...`，禁止隐式危险路径输入。
- 传输 worker 使用 generation 保护；旧 worker 的延迟 reaper 不会删除新一轮 retry 的控制槽位。
- 手工 retry 会恢复最大重试预算，不会因为上一轮失败直接耗尽。
- `.part` 文件继续采用流式、原子 rename 和可恢复 offset；完成后延迟回收 registry，避免 UI 刚收到 Completed 就丢失最终状态。
- 测试故障注入已修正为真正生效，重试/失败路径不再是“假绿”。

### 4. 权威性、安全边界和危险操作

- `connect_host`、`save_host_password`、`run_action` 以 SQLite 中的 Host/Action/Project 为权威，不信任 renderer 传入的可篡改完整对象。
- Action 的 danger level 在 Rust 服务端重新分类；前端声明 Safe 不能绕过危险确认。
- 增加/强化对递归删除、`git reset/clean`、磁盘格式化、关机、生产部署、Terraform/Kubernetes/Helm 删除、强制 push、管道执行 shell、fork bomb、`dd` 写设备等模式的识别。
- Host label、username、jump 字段、隧道目标、远程路径和 session 名称拒绝控制字符/注入字符。
- Host 删除前先断开运行时资源，再清理关联 Windows Credential Manager 引用。

### 5. 凭据和 Windows 持久化

- 普通 SQLite 只保存 opaque credential reference，不保存密码正文。
- 密码 IPC 传入后在 Rust 侧进入 zeroize 容器；前端使用非受控输入并在提交后清空。
- DPAPI 文件写入采用唯一临时文件和 Windows 原子替换，避免崩溃留下截断密文。
- 临时 Tailscale auth 文件使用 create-new，失败路径和 guard drop 都会清理。

### 6. 前端性能和使用逻辑

- xterm 输入按 animation frame 批处理并保持顺序，单次 payload 分块；输出继续按 rAF 合并，避免逐键/逐事件 IPC。
- scrollback 限制为 20,000，隐藏 tab 保持挂载但不重复创建终端。
- Activity/Run 列表按 host 过滤，切换主机不会显示上一台机器的执行记录。
- 终端、文件、传输和 Tailscale 订阅在卸载/断连时显式 dispose。
- 大目录最多返回 5,000 项并稳定排序，避免一次性渲染导致 WebView 卡死。

## 当前架构验收点

```text
React + xterm.js
        │ typed commands / bounded Channels
        ▼
Tauri thin shell
        │
        ▼
kodework-core (Session / Tunnel / Transfer / Run orchestration)
   ┌──────────┬──────────┬────────────┬─────────────┐
   ▼          ▼          ▼            ▼
kodework-ssh sftp      storage      tailscale runtime
   │          │          │            │
   └──────────┴──────────┴────────────┴───► remote Linux / tmux / herdr
```

约束已经锁定：`kodework-core` 不依赖 Tauri 类型；数据平面使用有界流；秘密不进入前端持久化；远程长任务依赖 tmux/Herdr 持久化，而不是依赖 Windows UI 永不退出。

## 自动化验证结果

以下命令在本轮最终代码上执行并通过：

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --quiet
npm run lint
npm run build
git diff --check
```

覆盖内容包括：

- fake SSH host-key、PTY flood、Jump Host、exec 输出隔离、断线和重订阅。
- SFTP round-trip、512 MiB 流式路径、`.part` 续传、暂停/取消、失败注入、retry/reaper 竞态。
- SQLite 迁移、WAL、host 过滤的 RunRepository 和外键清理。
- Tailscale JSON 容错、system/embedded backend plan、私有 state 隔离和 Windows daemon 路径。
- kodework-agent 协议、token 拒绝、超时 kill、Herdr bridge、隧道半关闭。
- React lint、TypeScript 构建和生产 bundle 分块。

依赖审计说明：`npm audit` 在当前机器配置的 `registry.npmmirror.com` 上返回 404/Not Implemented，不能把该网络错误当成漏洞结论；仓库未安装 `cargo-audit`/`cargo-deny`。这两项应在可用官方 registry/CI 中另行执行并保存报告。

## 发布链路和未完成项

### 可以交付的部分

- MSI 生成和 Tauri updater `.sig` 签名流程已有脚本：`scripts/build-release.ps1`。
- 签名私钥只从用户目录读取，不在仓库中；构建脚本不把 key 内容打印到日志。
- 单实例、托盘隐藏、登录自启和 per-user MSI 验证已在交接记录中完成。

### 不能虚报为已完成的部分

1. **真实 Tailscale 端到端**：需要新的、短期、最小权限、可撤销 auth key，并在授权的 Linux 主机上验证登录、`tailscale nc`、SSH、断网重连和 state 隔离。
2. **真实运行时矩阵**：Wi-Fi 断开/恢复、Windows 睡眠唤醒、Tailscale daemon 重启、CJK/IME、20 个并发终端、长时间传输、磁盘满、远端 tmux/Herdr/socat。
3. **更新服务**：`tauri.conf.json` 的 updater endpoint 仍是占位地址；部署前必须提供 HTTPS latest.json、MSI、`.sig` 和版本回滚策略。
4. **Windows 分发签名**：updater 签名不等于 Authenticode；正式分发仍需商业代码签名证书并接入 WiX/CI。
5. **产品补强**：完整 Run History 筛选/重试/重新附着 UI、凭据管理页、超大目录虚拟滚动、退出时传输/隧道有界停止提示。

## 发布前强制清单

- [ ] 撤销聊天中暴露的旧 Tailscale auth key，并生成新测试 key。
- [ ] 在隔离测试 tailnet 中执行 embedded userspace 登录和 `tailscale nc` SSH。
- [ ] 记录 host-key 指纹、断网恢复时间、重连次数和 tmux 任务连续性。
- [ ] 执行 CJK/IME、20 terminal flood、SFTP 大文件、睡眠恢复和卸载/重装。
- [ ] 替换 updater endpoint，上传 latest.json/MSI/.sig，并验证签名失败时客户端拒绝更新。
- [ ] 接入 Authenticode 证书，验证 SmartScreen/安装包签名。
- [ ] 在干净 Windows 用户目录执行一次冷启动、单实例、托盘、自启和 per-user 安装验证。

## 最终判断

当前版本已经从“功能堆叠的交接版本”收口为“代码级可交付候选版本”：高风险并发、凭据、危险命令、跨主机状态污染、Tailscale 受管生命周期和终端 IPC 性能问题已被处理，自动化质量门禁全部通过。正式宣称生产稳定版之前，必须完成上面的真实网络/Windows 矩阵；这是验证边界，不是继续堆功能的理由。
