# KodeWork Claude Code 交接快照（历史记录）

> **状态：已退役 / 历史快照**  
> 原始快照时间：2026-08-23（Asia/Shanghai）

这份文件曾用于一次性的 KodeWork → Claude Code 操作级交接。原文记录了当时尚未提交的本地工作树、PR #8 前后的授权边界、机器路径、提交状态、测试数量、实现细节和后续操作建议。

这些信息属于特定时间点的执行上下文，已经不能代表当前 `main`，也不应再作为 Coding Agent 的执行入口。原始完整内容仍保存在 Git 历史中，可用于审计、追溯和恢复历史上下文。

## 当前应读取的资料

需要了解现在的仓库时，请使用持续维护的来源，而不是沿用历史 handoff：

1. [`../README.md`](../README.md) / [`../README.zh-CN.md`](../README.zh-CN.md) — 产品入口与当前公开能力。
2. [`README.md`](README.md) — 文档地图，以及当前事实与历史记录的边界。
3. [`../AGENTS.md`](../AGENTS.md) — Coding Agent 的仓库级执行约束。
4. [`AGENT-GUIDE.md`](AGENT-GUIDE.md) — 详细维护、测试、安全与发布流程。
5. [`ARCHITECTURE.md`](ARCHITECTURE.md) 与 [`adr/`](adr/) — 当前架构和重要设计决策。
6. [`STATUS.md`](STATUS.md) — 当前实现状态和明确限制。
7. [`TEST-MATRIX-WINDOWS.md`](TEST-MATRIX-WINDOWS.md) — 自动化与原生 Windows 验证证据。
8. [`RELEASE-MATRIX.md`](RELEASE-MATRIX.md) — 发布、打包、签名和平台声明的证据要求。
9. [`../ROADMAP.md`](../ROADMAP.md) 与 [GitHub Issues](https://github.com/likangmax/KodeWork/issues) — 当前方向与尚未完成的实际工作。

## 关于旧交接中的内容

如果你在 Git 历史中查看原始 2026-08-23 交接，请把其中所有下列信息视为历史快照：

- 本地 Windows 路径、工作树状态和未提交文件；
- PR #8/#18 等当时的合并状态和授权边界；
- 当时的 commit SHA、依赖版本和测试数量；
- 当时建议的下一步、优先级和发布操作；
- 当时的机器配置、验证结果和临时限制；
- 指向后续 handoff 的“请改读”说明。

需要确认任何当前事实时，应直接检查当前代码、CI、持续维护文档和当前 GitHub Issues。不要从历史 handoff 推导今天的仓库状态。
