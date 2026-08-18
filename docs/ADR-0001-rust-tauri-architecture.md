# ADR-0001: Rust/Tauri desktop architecture

## Status

Accepted — 2026-08-14

## Context

Kodework Windows must remain responsive while maintaining long-lived SSH/tmux sessions and transferring files. It must use little memory, keep credentials local, and make network failures observable instead of silently losing work.

## Decision

- Use Rust as the system/core layer.
- Use Tauri 2 as the Windows shell and a web UI only for presentation.
- Keep connection, command, transfer, and persistence operations behind typed Rust commands.
- Use the operating system OpenSSH client first; add an embedded SSH implementation only where streaming/control requires it.
- Treat Tailscale as a transport/discovery provider. Kodework does not implement a VPN.
- Store only metadata in the app database; secrets are delegated to Windows Credential Manager in a later milestone.

## Consequences

Positive: low idle memory, fast startup, native process and filesystem access, testable core, no credential exposure to the renderer.

Trade-off: Tauri/Rust setup is more involved than Electron, and streaming PTY support needs platform-specific tests.

## Failure policy

Every network operation returns a typed error with a user-safe message and a diagnostic code. A disconnected session is marked stale and may be reattached; it is never represented as connected solely because a UI tab is open.
