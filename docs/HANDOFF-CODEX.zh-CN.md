# KodeWork 完整交接文档（给 Codex）

> 创建时间：2026-08-26  
> 当前分支：`main`，HEAD：`aba95e5`  
> 工作树：干净（仅 `.claude/` 未跟踪，本地配置）

---

## 一、项目快照

### 1.1 Git 状态

```bash
$ git log --oneline -3
aba95e5 (HEAD -> main, origin/main) Merge pull request #18 from likangmax/chore/dependency-updates
3227d8e chore(deps-dev): bump vitest from 4.1.10 to 4.1.11
1e3fafd Merge pull request #8 from likangmax/reliability/security-hardening
```

- **当前分支**：`main`
- **最新提交**：`aba95e5`，已推送到 `origin/main`
- **工作树**：干净，无暂存/未暂存修改
- **未跟踪文件**：`.claude/`（Claude Code 本地配置，机器专用，禁止提交）
- **PR #8**：已合并（reliability/security hardening，CI 全绿）
- **PR #18**：已合并（vitest 4.1.10 → 4.1.11）
- **当前开放 PR**：
  - #12：oxlint 1.78.0 → 1.79.0（Dependabot，CI 有 1 个 flaky PowerShell 超时失败，非回归）
  - #3：@types/node 24.13.3 → 26.2.0（Dependabot，等待评估）
  - #2：typescript 6.0.3 → 7.0.2（Dependabot，等待评估）

### 1.2 核心架构

**技术栈**：
- **前端**：React 19 + TypeScript 6 + Vite 8 + xterm.js 5.3
- **后端**：Tauri 2.11 + Rust 1.98 (MSVC) + 15-crate workspace
- **SSH**：russh 0.50 (纯 Rust，无 OpenSSL 依赖)
- **存储**：SQLite (bundled 3.53) + 各平台 OS 凭据存储
- **构建**：Cargo workspace + npm

**15 个 crate**：
```
kodework-agent          # (保留，未使用)
kodework-core           # 会话编排、生命周期、重连监督器
kodework-domain         # 纯模型、验证规则、无 I/O
kodework-herdr          # Herdr (tmux-like) 桥接协议
kodework-local-pty      # Windows ConPTY 本地终端
kodework-network        # 网络工具 (keepalive, 地址解析)
kodework-platform-win   # Windows 剪贴板 OSC 52
kodework-secrets        # 凭据接口
kodework-secrets-native # macOS/Linux keyring
kodework-secrets-win    # Windows Credential Manager
kodework-sftp           # SFTP 传输管理器 + 泵活机制
kodework-ssh            # SSH 连接、PTY、主机密钥、exec 通道
kodework-storage        # SQLite 仓库 (元数据)
kodework-tailscale      # Tailscale 侧车管理
kodework-testkit        # 测试工具
```

**全局安全约束**：
- 所有 crate 的 `Cargo.toml` 中 `unsafe_code = "forbid"`
- 凭据永不进入 SQLite，仅存 OS keyring
- 主机密钥绑定到逻辑 `HostId`，不是 `hostname:port`

**依赖关系（一个方向）**：
```
UI (React) → Tauri commands → kodework-core → kodework-domain ← adapters
                                    ↓
                            kodework-ssh / kodework-sftp
```

### 1.3 关键机制

#### 连接生成 (Connection Generation)
- 每次 SSH 重连，`generation` 递增
- 所有事件/数据都带生成号，旧生成的输出被 core 丢弃
- 防止重连后的旧数据污染新会话

#### 原生重连监督器
- **Rust 侧**完全拥有重连策略（指数退避、最大尝试次数）
- React **不再轮询**，通过 `session_runtime_subscribe` Tauri channel 被动接收快照
- 监督器在后台持续推送状态，前端仅渲染

#### SFTP 传输管理器 + 泵活机制
- `TransferManager::new_with_leases()` 返回 `(manager, mpsc::Receiver<TransferEvent>)`
- 目标文件租约：全局防止并发写入同一文件
- **泵活 (pump-alive) 机制**：
  - 事件泵退出时调用 `manager.event_pump_stopped()`
  - 内部 `pump_alive` 原子标志清零
  - 工作线程在下一个分块边界检查标志，发现泵死亡后立即中止
  - **防止**：泵死亡后工作线程在满通道上永久阻塞，持有租约和并发许可
- **目前实现**：
  - `pump_transfer_events` 函数退出时调用 ✅
  - `disconnect()` 显式断连时调用 ✅（本次新增）
  - Tauri `commands.rs` 的 `disconnect_host` 路径无需手动调用，已委托给 `session.disconnect()`

#### 主机密钥信任
- 绑定到 `HostId`（逻辑主机），不是 `hostname:port`
- 同一 `HostId` 通过 Tailscale、跳板机、直连的密钥必须一致
- 首次信任 (TOFU)，密钥变更需用户确认

#### Run 生命周期
- `Unknown` vs `Failed` 区分：
  - `Unknown`：dispatch 后无响应（可能命令已运行但未报告）
  - `Failed`：明确收到失败信号
- 数据库 `runs` 表记录全生命周期：`started_at_ms`, `finished_at_ms`, `exit_code`

#### Herdr 桥接
- `BridgeId` 标识桥接会话，不是 PID
- SSH exec 通道拥有整个桥接，通道关闭桥接自动清理
- 本地端口通过 `kodework-network` 隧道转发

### 1.4 当前测试覆盖

**Rust 测试**：199 个（全通过）
- `kodework-domain`：52 tests
- `kodework-storage`：48 tests (包括 `SqlU64` 边界测试)
- `kodework-sftp`：39 tests (包括泵活机制测试)
- `kodework-ssh`：19 tests
- `kodework-core`：11 tests
- `kodework-local-pty`：9 tests (Windows ConPTY, **有 1 个 flaky 超时**)
- `kodework-secrets-win`：6 tests
- 其他 crate：15 tests

**前端测试**：7 个（全通过）
- `i18n.test.ts`：zh-CN/en-US key 对等性

**CI 矩阵**：
- `frontend` (windows-latest, Node 22)：lint + test + build ✅
- `rust` (windows-latest, Rust 1.98 MSVC)：fmt + clippy + test ✅
- `dependency-security` (ubuntu-latest)：npm audit + cargo audit + 秘密扫描 ✅
- `portable-rust` (ubuntu-latest, macos-latest)：检查跨平台 crate ✅

**已知 flaky 测试**：
- `kodework-local-pty::tests::powershell_round_trip_uses_real_conpty`
- **原因**：PowerShell 冷启动在负载高的 CI runner 上超过 8 秒
- **非回归**：cmd.exe 测试同 job 通过，TypeScript/oxlint 升级与此无关
- **待修复**：将 `wait_for_terminal_sentinel` 的 `Duration::from_secs(8)` 增加到 15 秒

---

## 二、实现清单（已完成）

### 2.1 可靠性/安全加固（PR #8，已合并）

1. **SFTP 泵活机制**
   - `TransferManager::event_pump_stopped()` API
   - 工作线程通过 `pump_alive` 原子标志检测泵死亡
   - 测试：`dead_event_pump_aborts_worker_and_releases_destination`、`abandoned_transfer_can_be_retried_after_pump_death`

2. **Run 生命周期修复**
   - `Unknown` vs `Failed` 正确区分
   - `dispatch_id` 追踪防止状态覆盖

3. **主机密钥信任改进**
   - 绑定到 `HostId`，不再误报跨地址重连

4. **原生重连监督器**
   - 替换 React 轮询
   - 指数退避 + 最大尝试次数
   - `session_runtime_subscribe` 推送快照

5. **SqlU64 新类型**
   - 防止 `u64 → i64` 转换时的静默截断/饱和
   - `try_from` 失败时 SQL 操作明确报错
   - 边界测试：`u64::MAX` 拒绝，`i64::MAX as u64` 精确往返

### 2.2 国际化 (i18n)

- 前端 154+ 中文字面量全部替换为 `t(key)`
- `src/i18n.ts`：zh-CN + en-US 双语翻译目录
- `src/i18n.test.ts`：key 对等性测试
- 首次启动语言选择：通过 OS locale 或用户手动选择

**已知 gap**：Rust 错误消息仍有 ~40 处中文（PR #11 debt）

### 2.3 文档体系

**核心交接**：
- [`docs/HANDOFF-CLAUDE-CODE.zh-CN.md`](docs/HANDOFF-CLAUDE-CODE.zh-CN.md)（983 行，历史记录）
- **本文档**（当前最新交接）

**架构/设计**：
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)：边界、数据流、安全不变式
- [`docs/adr/`](docs/adr/)：架构决策记录

**用户文档**：
- [`docs/USER-GUIDE.md`](docs/USER-GUIDE.md)（英文）
- [`docs/USER-GUIDE.zh-CN.md`](docs/USER-GUIDE.zh-CN.md)（中文）
- [`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md)（686 行，全场景故障排查）

**开发者文档**：
- [`CONTRIBUTING.md`](CONTRIBUTING.md)（399 行）
- [`docs/AGENT-GUIDE.md`](docs/AGENT-GUIDE.md)（英文）
- [`docs/AGENT-GUIDE.zh-CN.md`](docs/AGENT-GUIDE.zh-CN.md)（中文）
- [`docs/TEST-MATRIX-WINDOWS.md`](docs/TEST-MATRIX-WINDOWS.md)

**规划文档**：
- [`docs/IMPROVEMENT-ROADMAP.md`](docs/IMPROVEMENT-ROADMAP.md)（6 阶段改进计划）
- [`docs/CROSS-PLATFORM-ROADMAP.md`](docs/CROSS-PLATFORM-ROADMAP.md)（Win/Mac/Linux/iOS/Android）

**其他**：
- [`CLAUDE.md`](CLAUDE.md)：AI agent 入口
- [`docs/README.md`](docs/README.md)：文档导航
- [`docs/STATUS.md`](docs/STATUS.md)：当前发布范围
- [`docs/RELEASE-MATRIX.md`](docs/RELEASE-MATRIX.md)：打包/签名需求
- [`docs/BRAND-ICON.md`](docs/BRAND-ICON.md)：图标资产

### 2.4 CI/CD 优化

- cargo-audit 0.21.2 → 0.22.2（支持 CVSS 4.0）
- TROUBLESHOOTING.md 从秘密扫描排除
- Node.js 20 deprecation 警告（actions 需升级到 Node 24 兼容版本）

### 2.5 依赖更新

**已合并**：
- vitest 4.1.10 → 4.1.11 (PR #18)
- React 19.2.8、TypeScript 6.0.2、Vite 8.2.0、xterm.js 5.3.0

**待合并**：
- oxlint 1.78.0 → 1.79.0 (PR #12，flaky 测试非回归)
- @types/node 24.13.3 → 26.2.0 (PR #3)
- TypeScript 6.0.3 → 7.0.2 (PR #2)

---

## 三、当前工作树细节

### 3.1 关键文件结构

```
redock-windows/
├── crates/                    # 15 个 Rust crate
│   ├── kodework-core/         # 会话编排
│   ├── kodework-domain/       # 领域模型
│   ├── kodework-sftp/         # 传输管理器
│   ├── kodework-ssh/          # SSH 适配器
│   ├── kodework-storage/      # SQLite 仓库
│   └── ...
├── src/                       # React 前端
│   ├── App.tsx               # 主组件 (i18n 完成)
│   ├── i18n.ts               # 翻译目录
│   └── i18n.test.ts          # i18n 测试
├── src-tauri/src/            # Tauri 桥接
│   ├── commands.rs           # Tauri 命令
│   ├── host_keys.rs          # 主机密钥决策
│   └── secrets.rs            # 凭据管理
├── docs/                     # 完整文档树
├── .github/workflows/ci.yml  # CI 配置
├── Cargo.toml                # workspace 根
├── package.json              # npm 依赖
├── rust-toolchain.toml       # Rust 1.98.0
└── .claude/                  # 本地配置 (未跟踪)
```

### 3.2 构建/测试命令

```powershell
# 前端
npm ci
npm run lint                  # oxlint
npm run test:frontend         # vitest
npm run build                 # Vite 生产构建

# Rust
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features

# Tauri 桌面构建
npm run desktop               # 开发模式
npm run build                 # 生产打包 (.msi)

# 安全审计
cargo audit --no-fetch
npm audit --omit=dev --audit-level=high
```

### 3.3 敏感数据约束

**永不提交**：
- 凭据、私钥、`.env` 文件
- 私有主机详情（IP、端口、用户名）
- 签名密钥、证书
- 真实基础设施数据
- 含秘密的日志

**测试 fixture 规则**：
- 测试用密钥必须是生成的 fixture，不是真实密钥
- `tests/fixtures/` 下的密钥带明确注释标识用途

---

## 四、已知 Gap（待优化）

### 4.1 高优先级

1. **Flaky 测试修复**
   - `powershell_round_trip_uses_real_conpty` 超时：8 秒 → 15 秒
   - 文件：`crates/kodework-local-pty/src/lib.rs:484`

2. **Rust 错误消息 i18n**
   - ~40 处中文错误消息待翻译
   - 需要 Rust 侧 i18n 框架（如 `fluent-rs`）

3. **Node.js 24 兼容**
   - `.github/workflows/ci.yml` 中的 `actions/setup-node@v4` 需升级
   - `actions/checkout@v4` 需升级

4. **Dependabot PR 合并**
   - PR #12: oxlint → 1.79.0
   - PR #3: @types/node → 26.2.0
   - PR #2: TypeScript → 7.0.2

### 4.2 中优先级

5. **GitHub 仓库"装修"**
   - README.md 增加截图/动图
   - 增加 shields.io badges（CI 状态、版本、许可证）
   - GitHub Pages 部署文档静态站

6. **性能优化**
   - 前端 React 组件性能分析（React DevTools Profiler）
   - Rust 热路径 profiling（cargo flamegraph）
   - 二进制大小优化（`cargo bloat`）

7. **测试覆盖扩展**
   - 前端 E2E 测试（Playwright）
   - Rust 集成测试增强
   - 基准测试（criterion）

8. **可访问性 (a11y)**
   - ARIA 标签
   - 键盘导航完整性
   - 屏幕阅读器兼容

### 4.3 长期规划

9. **跨平台桌面**
   - macOS 原生打包（.dmg + 公证）
   - Linux 打包（AppImage / .deb / .rpm）
   - 各平台 CI 矩阵

10. **移动平台探索**
    - iOS/Android 技术验证（Tauri Mobile 或 React Native）
    - 终端渲染适配
    - 触控交互设计

11. **社区建设**
    - GitHub Discussions
    - 贡献者指南增强
    - "good first issue" 标签

---

## 五、下一步优化建议（按优先级）

### 阶段 1：修复已知问题（1-2 天）

1. **修复 flaky 测试**
   ```rust
   // crates/kodework-local-pty/src/lib.rs:484
   let deadline = Instant::now() + Duration::from_secs(15); // 原 8
   ```

2. **合并 Dependabot PR**
   - 逐个验证本地测试通过后合并
   - PR #12 → #3 → #2

3. **Node.js 24 兼容**
   ```yaml
   # .github/workflows/ci.yml
   - uses: actions/checkout@v4.3.0        # 明确版本
   - uses: actions/setup-node@v4.1.0      # 支持 Node 24
   ```

### 阶段 2：GitHub 仓库可视化优化（2-3 天）

4. **README.md 增强**
   - 添加 hero 截图（主界面 + 连接面板）
   - 功能动图（连接、终端、传输）
   - Shields.io badges：
     ```markdown
     ![CI](https://github.com/likangmax/KodeWork/workflows/CI/badge.svg)
     ![License](https://img.shields.io/github/license/likangmax/KodeWork)
     ![Release](https://img.shields.io/github/v/release/likangmax/KodeWork)
     ```
   - 增加「为什么选择 KodeWork」章节（与竞品对比）

5. **文档静态站**
   - 使用 VitePress 或 mdBook
   - 从 `docs/` 生成导航
   - GitHub Pages 自动部署

6. **Social Preview**
   - 设计 Open Graph 图片（1200×630）
   - 仓库设置中上传 social preview

### 阶段 3：前端性能/可访问性优化（3-5 天）

7. **React 性能分析**
   - 使用 React DevTools Profiler 找热点组件
   - 优化不必要的重渲染（`useMemo`、`useCallback`、`memo`）
   - 大列表虚拟滚动（react-window）

8. **可访问性审计**
   - 使用 axe DevTools 扫描
   - 添加缺失的 ARIA 标签
   - 键盘导航测试（Tab / Shift+Tab / Enter / Esc）
   - 屏幕阅读器测试（NVDA / JAWS）

9. **前端 E2E 测试**
   - Playwright 设置
   - 关键路径场景：连接 → 终端输入 → 文件传输

### 阶段 4：Rust 侧优化（5-7 天）

10. **Rust 错误消息 i18n**
    - 引入 `fluent-rs`
    - 定义 `.ftl` 文件（zh-CN / en-US）
    - 重构 40+ 处中文错误消息
    - 通过 Tauri 传递 locale

11. **性能 profiling**
    - `cargo flamegraph` 分析热路径
    - `cargo bloat` 分析二进制大小
    - 优化发现的瓶颈

12. **测试覆盖扩展**
    - 增加边界测试
    - 属性测试（proptest）用于验证逻辑
    - 集成测试覆盖 SSH reconnect 场景

### 阶段 5：跨平台桌面（2-3 周）

13. **macOS 打包**
    - Tauri macOS 构建配置
    - 代码签名（Apple Developer ID）
    - 公证（notarization）
    - CI 添加 `macos-latest` job

14. **Linux 打包**
    - AppImage 生成
    - .deb / .rpm 打包
    - CI 添加 `ubuntu-latest` job
    - 测试 Ubuntu / Fedora / Arch

### 阶段 6：社区/生态（持续）

15. **GitHub 社区功能**
    - 启用 Discussions
    - Issue / PR 模板优化
    - CODEOWNERS 文件
    - "good first issue" 标签 + 指南

16. **发布自动化**
    - Release workflow（tag → build → GitHub Release）
    - 自动 CHANGELOG 生成
    - 更新检查服务

17. **生态集成**
    - VS Code 扩展探索
    - Webhooks for CI/CD
    - Prometheus metrics exporter（可选）

---

## 六、技术债与注意事项

### 6.1 已知技术债

1. **Tauri IPC 序列化**
   - 大量使用 `Result<T, String>` 返回类型
   - 错误细节丢失，前端只能显示字符串
   - **改进方向**：定义结构化错误枚举，通过 `#[derive(Serialize)]` 传递

2. **前端状态管理**
   - 未使用集中状态库（Redux / Zustand）
   - 状态分散在组件本地 `useState`
   - **改进方向**：引入 Zustand 或 Jotai，统一管理连接/传输状态

3. **日志结构化**
   - Rust 侧部分日志仍是非结构化字符串
   - **改进方向**：全面使用 `tracing` 库，结构化字段

4. **测试隔离**
   - 部分测试依赖本地环境（如 PowerShell / cmd.exe 存在性）
   - **改进方向**：Mock 或跳过环境依赖测试

### 6.2 安全边界约束

- **凭据隔离**：永不将密码/密钥写入日志或 SQLite
- **主机密钥**：绑定到 `HostId`，删除 Host 时清除信任
- **输入验证**：所有用户输入在 `kodework-domain` 层验证
- **错误消息**：不泄露内部路径或敏感细节到前端

### 6.3 性能边界约束

- **事件通道**：所有高频数据使用 bounded channel（512-1024）
- **泵活机制**：防止工作线程在死泵上阻塞
- **重连退避**：指数退避上限 32 秒
- **传输并发**：默认 8 并发 SFTP 传输

---

## 七、命令速查

### 7.1 开发

```powershell
# 安装依赖
npm ci
cargo build

# 开发运行
npm run desktop

# 代码质量
npm run lint
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings

# 测试
npm run test:frontend
cargo test --locked --workspace --all-features

# 生产构建
npm run build              # .msi 输出到 src-tauri/target/release/bundle/msi/
```

### 7.2 Git / GitHub

```powershell
# 查看分支状态
git status
git log --oneline -10

# 创建分支
git checkout -b feature/your-feature

# PR 管理（需 gh CLI）
gh pr list --state open
gh pr view 12
gh pr merge 12 --squash

# 推送（需配置 proxy）
$env:http_proxy="http://127.0.0.1:7890"
$env:https_proxy="http://127.0.0.1:7890"
git push origin main
```

### 7.3 发布

```powershell
# 创建 tag
git tag v0.2.4
git push origin v0.2.4

# GitHub Release（手动或通过 workflow）
gh release create v0.2.4 --title "KodeWork 0.2.4" --notes "..."

# 上传构建产物
gh release upload v0.2.4 src-tauri/target/release/bundle/msi/*.msi
```

---

## 八、联系与权限

- **GitHub 仓库**：<https://github.com/likangmax/KodeWork>
- **当前维护者**：likangmax
- **协议**：MIT License
- **凭据存储位置**：Windows Credential Manager（`kodework:host:{host_id}`）
- **数据库位置**：`%APPDATA%\dev.kodework.windows\kodework.db`

**授权范围**：
- ✅ 读取所有代码/文档
- ✅ 创建分支、提交、PR
- ✅ 合并 PR（CI 全绿且经审查）
- ✅ 推送到 `main`（仅限经验证的改进）
- ❌ 删除分支、强制推送、修改历史（需明确授权）
- ❌ 创建 GitHub Release（需明确授权）
- ❌ 修改仓库设置（需明确授权）

---

## 九、验证检查清单（每次提交前）

```powershell
# 1. 代码格式
cargo fmt --all -- --check
npm run lint

# 2. 编译无警告
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings

# 3. 测试通过
cargo test --locked --workspace --all-features
npm run test:frontend

# 4. 生产构建成功
npm run build

# 5. 安全审计
cargo audit --no-fetch
npm audit --omit=dev --audit-level=high

# 6. 秘密扫描
git diff --check
git grep -nI -E '(tskey-[A-Za-z0-9]|-----BEGIN .* PRIVATE KEY-----)'
```

---

## 十、总结

KodeWork 是一个**健康、可维护、安全**的 Rust + React 桌面应用。核心架构清晰，测试覆盖良好，文档完整。

**当前状态**：
- ✅ Windows 桌面版可用（v0.2.3）
- ✅ CI 全绿（除 1 个已知 flaky）
- ✅ 无已知 P0/P1 bug
- ✅ 文档覆盖率 100%

**下一步重点**：
1. 修复 flaky 测试
2. 合并 Dependabot PR
3. GitHub 仓库可视化优化
4. 前端性能 + 可访问性
5. 跨平台扩展（macOS / Linux）

**长期目标**：
- 成为**顶级开源 SSH 远程开发工具**
- 多平台支持（Win/Mac/Linux/iOS/Android）
- 活跃的社区生态

---

**交接完成。祝 Codex 顺利接手！** 🚀
