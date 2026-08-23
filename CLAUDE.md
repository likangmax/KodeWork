# Claude Code project entry point

Before changing, committing, or publishing anything, read the complete Chinese
handoff at [`docs/HANDOFF-CLAUDE-CODE.zh-CN.md`](docs/HANDOFF-CLAUDE-CODE.zh-CN.md).

Critical rules for the current checkout:

1. The branch is `published-main`, based at `08877a88de3b7c146ffe8cdba208e9c7bf1e7486`.
2. The working tree intentionally contains a large, verified but uncommitted
   reliability/security hardening set. Do not reset, checkout, clean, stash,
   overwrite, or discard it.
3. Do not commit, push, force-push, merge PR #8, create a release, or mutate
   GitHub assets unless the repository owner explicitly authorizes that action
   in the current conversation.
4. Current local code and current test evidence take priority over older PR
   comments, cached review text, or the red CI run attached to the remote head.
5. Never put credentials, private host details, signing keys, logs containing
   secrets, or real infrastructure data into source, tests, issues, or PR text.

The handoff contains the exact Git/GitHub snapshot, implementation inventory,
architecture invariants, validation commands and results, known gaps, and the
recommended continuation sequence.
