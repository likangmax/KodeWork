# KodeWork 下一步优化计划

> 基于 HANDOFF-CODEX.zh-CN.md，聚焦最高优先级改进
> 创建时间：2026-08-26
> 目标：打造顶级开源 SSH 远程开发工具

---

## 立即执行（1-3 天）

### 1. 合并 Dependabot PR

**PR #12**: oxlint 1.78.0 → 1.79.0
- 状态：CI 有 1 个 flaky 测试失败（非回归）
- 操作：本地验证后直接合并
```powershell
gh pr merge 12 --squash
```

**PR #3**: @types/node 24.13.3 → 26.2.0
- 风险：API 可能变更
- 操作：本地测试通过后合并
```powershell
npm ci
npm run lint
npm run test:frontend
npm run build
```

**PR #2**: TypeScript 6.0.3 → 7.0.2
- 风险：主版本升级，可能有 breaking changes
- 操作：仔细审查编译错误，必要时调整代码
```powershell
npm ci
npm run build  # 检查类型错误
```

### 2. Node.js 24 兼容（CI 升级）

编辑 `.github/workflows/ci.yml`：

```yaml
# 当前（Node 20 deprecated 警告）
- uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4
- uses: actions/setup-node@49933ea5288caeca8642d1e84afbd3f7d6820020 # v4

# 升级到（明确版本 + Node 24 支持）
- uses: actions/checkout@v4.3.0
- uses: actions/setup-node@v4.1.0
  with:
    node-version: 24
```

---

## 短期优化（1-2 周）

### 3. GitHub 仓库视觉优化

#### 3.1 README.md 增强

**截图/动图**：
- Hero 截图：主界面 + 连接面板（1920×1080）
- 功能动图：
  - 连接流程（点击 → 认证 → 就绪）
  - 终端使用（命令输入 + 输出）
  - 文件传输（拖拽上传 + 进度条）

**Badges**：
```markdown
![CI](https://github.com/likangmax/KodeWork/workflows/CI/badge.svg)
![License](https://img.shields.io/github/license/likangmax/KodeWork)
![Release](https://img.shields.io/github/v/release/likangmax/KodeWork)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue)
```

**新增章节**：
- **为什么选择 KodeWork**：与 PuTTY/MobaXterm/Termius 对比表格
- **核心特性**：纯 Rust SSH、原生重连、Herdr 集成、Tailscale 支持
- **安全承诺**：禁用 unsafe code、凭据隔离、主机密钥 TOFU

#### 3.2 Social Preview

设计 Open Graph 图片（1200×630）：
- KodeWork logo + 标语
- 代码编辑器风格背景
- 上传到 GitHub 仓库设置

#### 3.3 文档静态站

**技术选型**：VitePress（与 Vite 技术栈一致）

```bash
npm install -D vitepress
```

**目录结构**：
```
docs/
├── .vitepress/
│   └── config.ts         # 导航配置
├── index.md              # 首页（README.md 转换）
├── guide/
│   ├── getting-started.md
│   ├── user-guide.md
│   └── troubleshooting.md
├── architecture/
│   ├── overview.md
│   └── decisions.md
└── api/
    └── rust-crates.md
```

**GitHub Pages 自动部署**：
```yaml
# .github/workflows/docs.yml
name: Deploy Docs
on:
  push:
    branches: [main]
    paths: ['docs/**']
jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 24
      - run: npm ci
      - run: npm run docs:build
      - uses: peaceiris/actions-gh-pages@v4
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: docs/.vitepress/dist
```

### 4. 前端性能优化

#### 4.1 React 性能分析

使用 React DevTools Profiler：
1. 在 Chrome 安装 React DevTools
2. 打开 Profiler 标签
3. 录制典型操作（连接 → 终端输入 → 文件传输）
4. 识别热点组件（高 render 次数 + 长 render 时间）

**常见优化**：
- 过度 re-render：用 `React.memo` + `useMemo` + `useCallback`
- 大列表卡顿：用 `react-window` 虚拟滚动
- 昂贵计算：移到 Web Worker 或 Rust 侧

#### 4.2 Bundle 大小分析

```bash
npm run build -- --mode analyze
```

查找：
- 未使用的依赖
- 重复打包的模块
- 可按需加载的组件

#### 4.3 xterm.js 优化

```typescript
// 当前（默认配置）
const terminal = new Terminal();

// 优化（性能优先）
const terminal = new Terminal({
  fastScrollModifier: 'shift',
  fastScrollSensitivity: 5,
  scrollSensitivity: 3,
  rendererType: 'canvas',  // canvas 比 dom 快
  rows: 30,                // 限制初始行数
  cols: 120,
  scrollback: 10000,       // 限制回滚缓冲
});
```

### 5. 可访问性 (a11y) 审计

#### 5.1 自动扫描

使用 axe DevTools：
1. 安装 Chrome 扩展
2. 打开 KodeWork
3. 运行 "Scan ALL of my page"
4. 修复 Critical + Serious 问题

**常见问题**：
- 缺失 `alt` 文本（图标按钮）
- 缺失 `aria-label`（交互元素）
- 对比度不足（文本颜色）
- 焦点指示器不可见

#### 5.2 键盘导航测试

检查清单：
- Tab / Shift+Tab：遍历所有可交互元素
- Enter / Space：激活按钮
- Esc：关闭对话框 / 取消操作
- 方向键：列表导航

#### 5.3 屏幕阅读器测试

使用 NVDA（免费）：
1. 下载安装 NVDA
2. 启动后打开 KodeWork
3. 用键盘导航，听语音反馈
4. 确保所有信息可访问

---

## 中期优化（2-4 周）

### 6. Rust 错误消息 i18n

#### 6.1 技术方案

**库选择**：`fluent-rs`（Mozilla 的本地化方案）

```toml
# Cargo.toml
[dependencies]
fluent = "0.16"
fluent-bundle = "0.15"
unic-langid = "0.9"
```

**目录结构**：
```
crates/kodework-domain/
├── locales/
│   ├── zh-CN/
│   │   └── errors.ftl
│   └── en-US/
│       └── errors.ftl
```

**示例 .ftl 文件**：
```fluent
# zh-CN/errors.ftl
ssh-connection-failed = SSH 连接失败：{ $reason }
ssh-authentication-failed = 认证失败：{ $method }
transfer-destination-busy = 目标文件正在被其他传输占用
```

#### 6.2 实现步骤

1. 创建 `kodework-i18n` crate
2. 定义 `LocalizedError` trait
3. 重构 40+ 处中文错误消息
4. Tauri 侧传递用户 locale
5. 测试覆盖（中英文切换）

### 7. 前端 E2E 测试

#### 7.1 Playwright 设置

```bash
npm install -D @playwright/test
npx playwright install
```

**配置**：
```typescript
// playwright.config.ts
export default defineConfig({
  testDir: './e2e',
  use: {
    baseURL: 'http://localhost:1420',  // Tauri dev server
  },
  webServer: {
    command: 'npm run desktop',
    port: 1420,
    reuseExistingServer: !process.env.CI,
  },
});
```

#### 7.2 关键场景

```typescript
// e2e/connection.spec.ts
test('connect to SSH host', async ({ page }) => {
  await page.goto('/');
  await page.click('[data-testid="add-host-btn"]');
  await page.fill('[name="hostname"]', 'test.example.com');
  await page.fill('[name="username"]', 'testuser');
  await page.click('[data-testid="connect-btn"]');
  await expect(page.locator('[data-testid="status"]')).toContainText('Ready');
});
```

### 8. 性能 Profiling + 优化

#### 8.1 Rust 侧 Flamegraph

```bash
cargo install flamegraph
cargo flamegraph --bin kodework-tauri
# 打开 KodeWork，执行典型操作
# Ctrl+C 停止，生成 flamegraph.svg
```

**寻找**：
- CPU 热点函数
- 频繁分配 / 克隆
- 锁竞争

#### 8.2 二进制大小优化

```bash
cargo install cargo-bloat
cargo bloat --release -n 20
```

**常见优化**：
- 移除未使用的 feature
- 优化依赖（如 tokio 只启用必需 feature）
- LTO + strip：
  ```toml
  [profile.release]
  lto = true
  strip = true
  codegen-units = 1
  ```

---

## 长期规划（1-3 个月）

### 9. 跨平台桌面支持

#### 9.1 macOS 打包

**CI 配置**：
```yaml
# .github/workflows/release.yml
macos-build:
  runs-on: macos-latest
  steps:
    - uses: dtolnay/rust-toolchain@stable
      with:
        targets: aarch64-apple-darwin, x86_64-apple-darwin
    - run: npm run build
    - run: npx tauri build --target universal-apple-darwin
```

**代码签名**：
- 申请 Apple Developer ID
- 配置 `tauri.conf.json`:
  ```json
  "bundle": {
    "macOS": {
      "signingIdentity": "Developer ID Application: Your Name"
    }
  }
  ```

**公证**：
```bash
xcrun notarytool submit KodeWork.dmg --apple-id ... --password ...
```

#### 9.2 Linux 打包

**格式选择**：
- AppImage（通用）
- .deb（Debian/Ubuntu）
- .rpm（Fedora/RedHat）

**Tauri 配置**：
```json
"bundle": {
  "linux": {
    "appimage": {
      "bundleMediaFramework": true
    },
    "deb": {
      "depends": ["libwebkit2gtk-4.0-37"]
    }
  }
}
```

**测试矩阵**：
- Ubuntu 22.04 / 24.04
- Fedora 39 / 40
- Arch Linux（社区打包）

### 10. 移动平台探索

#### 10.1 技术验证

**Tauri Mobile**（实验性）：
- 支持 iOS / Android
- 复用 Rust 后端 + React 前端
- 终端渲染适配（xterm.js → 原生 TextView）

**React Native**（备选）：
- 成熟生态
- 需重写 UI
- SSH 通过 FFI 调用 Rust

#### 10.2 POC 目标

- SSH 连接成功
- 基础终端输入 / 输出
- 简单文件传输
- 不求功能完整，验证可行性

### 11. 社区建设

#### 11.1 GitHub Discussions

启用 Discussions，分类：
- 📢 Announcements（发布公告）
- 💡 Ideas（功能建议）
- 🙏 Q&A（问题解答）
- 🐛 Bug Reports（非严重 bug）

#### 11.2 贡献者体验

**Issue 模板**：
```markdown
---
name: Bug Report
about: Report a bug in KodeWork
labels: bug
---

**描述问题**
简要描述 bug。

**复现步骤**
1. 打开 KodeWork
2. 点击「连接」
3. 输入主机名...

**预期行为**
应该发生什么。

**实际行为**
实际发生了什么。

**环境**
- OS: Windows 11 Pro
- KodeWork 版本: 0.2.3
- 连接方式: 直连 / Tailscale / 跳板机
```

**PR 模板**：
```markdown
**改动内容**
简要描述这个 PR 做了什么。

**测试**
- [ ] 所有测试通过
- [ ] 已手动测试
- [ ] 文档已更新

**相关 Issue**
Closes #123
```

**"Good First Issue" 标签**：
- 挑选简单任务（文档修复、UI 调整、测试增强）
- 打上标签 + 添加指导注释

#### 11.3 发布自动化

**Release Workflow**：
```yaml
# .github/workflows/release.yml
on:
  push:
    tags: ['v*']
jobs:
  build:
    strategy:
      matrix:
        os: [windows-latest, macos-latest, ubuntu-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - run: npm run build
      - uses: softprops/action-gh-release@v1
        with:
          files: |
            src-tauri/target/release/bundle/**/*.msi
            src-tauri/target/release/bundle/**/*.dmg
            src-tauri/target/release/bundle/**/*.AppImage
```

**CHANGELOG 自动生成**：
```bash
npm install -D conventional-changelog-cli
npx conventional-changelog -p angular -i CHANGELOG.md -s
```

---

## 成功指标

### 短期（1 个月）
- ✅ CI 全绿（无 flaky 测试）
- ✅ 文档站点上线
- ✅ README badges 完整
- ✅ 前端性能基线建立
- ✅ a11y 扫描 0 Critical 问题

### 中期（3 个月）
- ✅ macOS / Linux 版本发布
- ✅ 前端 E2E 测试覆盖关键路径
- ✅ Rust 错误消息全面 i18n
- ✅ GitHub stars > 100
- ✅ 社区贡献 > 5 人

### 长期（6 个月）
- ✅ 移动版 POC 完成
- ✅ 插件 API 设计
- ✅ CI/CD 全自动发布
- ✅ 月活用户 > 500
- ✅ 成为「Awesome SSH Tools」列表推荐项目

---

## 资源链接

**文档**：
- [HANDOFF-CODEX.zh-CN.md](HANDOFF-CODEX.zh-CN.md)（完整交接）
- [IMPROVEMENT-ROADMAP.md](IMPROVEMENT-ROADMAP.md)（6 阶段计划）
- [CROSS-PLATFORM-ROADMAP.md](CROSS-PLATFORM-ROADMAP.md)（多平台策略）

**工具**：
- React DevTools: <https://react.dev/learn/react-developer-tools>
- axe DevTools: <https://www.deque.com/axe/devtools/>
- Playwright: <https://playwright.dev/>
- VitePress: <https://vitepress.dev/>

**社区**：
- GitHub: <https://github.com/likangmax/KodeWork>
- Issues: <https://github.com/likangmax/KodeWork/issues>
- Discussions: <https://github.com/likangmax/KodeWork/discussions>（待启用）

---

**执行原则**：
1. **质量优先**：每个改动都要通过 CI + 手动测试
2. **增量迭代**：小步快跑，频繁发布
3. **用户至上**：每个决策都问「这对用户有帮助吗？」
4. **文档同步**：代码改动必须更新相应文档
5. **社区友好**：响应 issue < 48 小时，PR 审查 < 72 小时

**开始吧！🚀**
