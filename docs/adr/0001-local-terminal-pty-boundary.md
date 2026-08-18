# ADR 0001: Local terminals use an independent PTY manager

## Decision

Kodework keeps local PowerShell/CMD/WSL sessions in a dedicated `kodework-local-pty` crate backed by `portable-pty`. The SSH `SessionManager` remains responsible only for remote channels. Both surfaces stream bytes through the same xterm renderer and share the same bounded session limit (20).

## Rationale

- A local process has a different lifecycle from an SSH transport and must not be affected by remote reconnects.
- `portable-pty` provides ConPTY on Windows and native PTYs on Unix, preserving the cross-platform roadmap.
- Commands use argv, never a shell-built command string; WSL distribution names are validated against `wsl --list --quiet`.
- Output is bounded and back-pressured, with a small replay buffer for tabs that are not yet mounted.

## Consequences

The UI can create multiple independent local, WSL, and remote terminal sessions. Local sessions are explicitly terminated on application exit; remote long-running work remains protected by the existing remote tmux/herdr model.
