# KodeWork Codex 交接快照（历史记录）

> **状态：已退役 / 历史快照**  
> 原始交接创建于：2026-08-27  
> 原始快照对应提交：`bcc9e00`

这份文件曾用于一次性的 Claude Code → Codex 交接。原文包含当时的提交 SHA、依赖版本、测试数量、GitHub 模板路径、工作树状态和后续计划；这些信息会随仓库演进迅速失效，因此**不得再把本文件当作当前项目状态、维护任务清单或 Agent 执行入口**。

原始完整交接内容仍保存在 Git 历史中，便于审计和追溯。当前分支不再复制那份易过期的快照，以避免新的维护者或 Agent 被旧事实误导。

## 当前应读取的资料

需要了解现在的仓库时，请按以下顺序使用持续维护的资料：

1. [`../README.md`](../README.md) / [`../README.zh-CN.md`](../README.zh-CN.md) — 产品入口与当前公开能力。
2. [`README.md`](README.md) — 文档地图，以及“当前事实”与“历史记录”的边界。
3. [`../AGENTS.md`](../AGENTS.md) — Coding Agent 的仓库级执行约束。
4. [`AGENT-GUIDE.md`](AGENT-GUIDE.md) — 详细维护、测试、安全与发布流程。
5. [`STATUS.md`](STATUS.md) — 当前实现状态和明确限制。
6. [`TEST-MATRIX-WINDOWS.md`](TEST-MATRIX-WINDOWS.md) — 自动化与原生 Windows 验证证据。
7. [`RELEASE-MATRIX.md`](RELEASE-MATRIX.md) — 发布、打包、签名和平台声明的证据要求。
8. [`../ROADMAP.md`](../ROADMAP.md) — 当前公开路线图。
9. [GitHub Issues](https://github.com/likangmax/KodeWork/issues) — 尚未完成、可实际参与的工作。

## 关于旧交接中的内容

如果你在 Git 历史中查看 2026-08-27 的原始交接，请把下列内容全部视为**当时的快照**，而不是当前事实：

- `bcc9e00` 等提交号；
- “199 个 Rust 测试 / 7 个前端测试”等数量；
- 当时的 React、TypeScript、Tauri、Rust、SSH/SFTP 等依赖版本；
- `.github/ISSUE_TEMPLATE/*.md` 等旧模板路径；
- 当时记录的性能、技术债和阶段路线图；
- 本地工作树、`.claude/` 或机器相关状态；
- “已完成 / 待完成”判断。

需要确认任何当前事实时，应直接检查当前代码、CI、上述持续维护文档以及当前 GitHub Issues，而不是沿用历史 handoff 的结论。
