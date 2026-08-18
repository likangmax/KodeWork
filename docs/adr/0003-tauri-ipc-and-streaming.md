# ADR-0003: Thin Tauri shell with typed control plane and bounded data plane

## Status

Accepted — 2026-08-14

## Context

Terminal output and transfer progress are high-frequency streams. A command-per-byte or unbounded event design would create backpressure and memory problems, while placing SSH state in React would make reconnect and cancellation unreliable.

## Decision

- Keep SSH, SFTP, Tailscale, Herdr, and state machines in Tauri-independent Rust crates.
- Expose small typed commands for control operations and use bounded, ordered Tauri Channels for terminal, run, and transfer data.
- Aggregate terminal bytes in Rust by size/time before crossing IPC; associate every frame with a session/run/transfer ID.
- Reject stale output using connection generations after reconnect.

## Consequences

### Positive

- The UI can be replaced without rewriting network code.
- Backpressure and cancellation are explicit and testable.
- No fabricated UI state is needed to infer connection status.

### Negative

- DTOs and channel schemas need versioning.
- Integration tests must cover UI restart, slow renderers, and output floods.

## Alternatives Considered

- A localhost WebSocket between Rust and React was rejected as an unnecessary extra hop for the in-process Tauri shell.
- Keeping all commands in one `commands.rs` module was rejected; handlers are split and remain thin so business logic does not accumulate at the boundary.
