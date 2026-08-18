# ADR-0006: Separate UI boundaries and release trust mechanisms

## Status
Accepted

## Context

The Windows client combines a high-throughput terminal, SFTP, remote runtime
discovery, Tailscale startup and credential-bearing SSH authentication. The
previous implementation placed most orchestration and rendering in one React
component and treated updater signing as if it were equivalent to Windows
installer signing.

## Decision

- Keep session, transfer and authentication state in Rust/Core; React owns
  view state and uses typed IPC.
- Split terminal, runtime, files, settings and workspace surfaces into
  memoized modules. Large directory rendering uses a fixed-row virtual window
  with overscan rather than mapping every remote entry to a DOM node.
- Persist only authentication mode, selected private-key path and opaque
  credential references. Use Windows OpenSSH Agent/Pageant for agent mode and
  an expiring keyboard-interactive broker for MFA prompts.
- Require both Tauri updater signatures and Authenticode for public Windows
  releases. A static HTTPS `latest.json` channel is deployed separately from
  the desktop binary.

## Consequences

### Positive

- UI rewrites do not change SSH lifecycle code.
- Huge directories and terminal floods have explicit rendering/backpressure
  bounds.
- Passwords, passphrases and MFA responses do not enter SQLite or logs.
- SmartScreen trust and update-integrity trust are independently verifiable.

### Negative

- Authentication UI requires a broker/polling path and more state transitions.
- A real release requires DNS/origin access, an updater signing key and a
  commercial Authenticode certificate; local builds cannot manufacture these.

## Alternatives Considered

- Keep all UI in `App.tsx`: rejected because it couples unrelated lifecycles
  and makes regressions difficult to isolate.
- Use a virtual-list dependency: rejected for this surface; fixed-row math is
  small, auditable and avoids another runtime dependency.
- Treat minisign updater signatures as Authenticode: rejected; they protect
  different artifacts and Windows trust surfaces.
