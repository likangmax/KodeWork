# Claude Code project entry point

Read the current handoff at [`docs/HANDOFF-CODEX.zh-CN.md`](docs/HANDOFF-CODEX.zh-CN.md) 
before changing, committing, or publishing anything.

Current checkout facts (verify again before mutation):

1. Branch: `main`; current local `HEAD` and `origin/main`: `aba95e5`.
2. Working tree has **one uncommitted fix**: `disconnect()` now calls 
   `event_pump_stopped()` to properly signal SFTP workers on teardown.
3. The only untracked item is `.claude/` (local settings, never commit).
4. Do not reset, clean, force-push, rewrite history, create releases, or mutate
   GitHub assets without explicit authorization in the current conversation.
5. Local code and fresh test evidence take priority over stale handoffs.
6. Never put credentials, private host details, signing keys, secret-bearing
   logs, or real infrastructure data into source, tests, issues, or PR text.

The handoff records the verified implementation inventory, exact local and 
GitHub CI evidence, known gaps, and the safe next sequence.
