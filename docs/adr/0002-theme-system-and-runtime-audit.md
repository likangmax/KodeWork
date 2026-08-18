# ADR 0002 — Theme system and runtime audit hardening

## Status

Accepted — 2026-08-17

## Context

The desktop client previously used a single dark palette and the terminal palette was independent from the application palette. A production audit also found that two polling loops could overlap when an IPC call or reconnect backoff lasted longer than their interval. The production CSP included development-server websocket permissions, and the transient status-message timer was not explicitly cleared when the root component unmounted.

## Decision

- Store only the non-sensitive theme preference (`mode` and `accent`) in local storage. Credentials and host secrets remain outside browser state.
- Support `dark`, `light`, and `system` modes, plus six accent colors. Apply the resolved values to `data-theme`/`data-accent` on the document root.
- Bootstrap the theme before the module bundle loads to avoid a first-paint flash. If storage is unavailable, the current-process theme still works.
- Derive xterm colors from the same resolved theme and refresh mounted terminals when the theme changes.
- Keep host-key/keyboard-interactive polling and reconnect polling single-flight. Polls stop mutating state after their owning effect is disposed.
- Clear the status-message timeout on unmount.
- Use a strict production `connect-src 'self'`; loopback preview frames remain explicitly allowed because they are the product's SSH port-forward feature.

## Consequences

The UI can be changed without touching the Rust core or credential boundary. Theme changes are immediate and survive restarts. Development HMR may require the development server's own configuration, while packaged builds use the strict CSP. Reconnect behavior is less likely to create duplicate SSH attempts or stale state updates.

## Verification

- `npm run lint`
- `npm run build`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- Browser smoke test: settings open, light/blue theme applies, reload persists it, no overflow, and no console errors in a fresh page.
- `npm audit --omit=dev --audit-level=high` against the public npm registry: 0 vulnerabilities.
- `cargo audit`: no vulnerability findings; existing unmaintained GTK/Unicode advisory warnings remain transitive packaging warnings.
