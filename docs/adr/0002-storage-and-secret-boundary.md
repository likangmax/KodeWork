# ADR-0002: SQLite metadata and OS-backed secret references

## Status

Accepted — 2026-08-14

## Context

Hosts need to survive application restarts, but passwords, private keys, passphrases, and Tailscale auth keys must never be exposed to the renderer, ordinary SQLite columns, logs, or crash reports. SSH configuration also needs schema migrations so upgrades remain deterministic.

## Decision

- Store only non-secret metadata in SQLite. Run history persists lifecycle
  metadata and byte counts, never stdout/stderr previews; bounded output is
  returned only to the active renderer.
- Store credentials as `{ provider, opaque_id }` references.
- Use Windows Credential Manager for passwords, passphrases, and tokens; use a DPAPI-protected per-user file for managed private-key material when a file copy is required.
- Keep a fake in-memory `SecretStore` for tests; it redacts `Debug` output and zeroizes owned buffers.
- Apply numbered, idempotent SQLite migrations at startup. Host runtime preference and Tailscale configuration are migrated without rewriting secrets.

## Consequences

### Positive

- Renderer state and database dumps cannot directly reveal credentials from
  the secret store or persisted run output. User-authored action commands and
  snippets remain workspace text and are not treated as a credential store.
- Database backups remain useful without becoming credential backups.
- Migration and round-trip tests can run offline with an in-memory database.

### Negative

- A host record alone is not sufficient to connect; the referenced OS credential must exist.
- Windows-specific integration tests require a Windows runner and must not be replaced by claims based on the fake store.

## Alternatives Considered

- Encrypting every secret inside SQLite was rejected because key lifecycle and renderer boundaries would be harder to audit.
- Putting large private keys directly in Windows Credential Manager was rejected because credential blob size is limited and file semantics are useful for SSH clients.
