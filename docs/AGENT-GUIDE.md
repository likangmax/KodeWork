# Guide for coding agents and maintainers

This document is the operational contract for an AI coding agent or human maintainer working on KodeWork. It is written so an agent can configure a synthetic workstation, verify each boundary, and report what was and was not tested without guessing.

## 1. Product truth

- The currently distributable desktop artifact is a Windows 10/11 x64 MSI.
- Portable Rust crates are checked on Windows, Linux, and macOS CI. Do not call macOS/Linux a released desktop product until native bundles, signing, installation, GUI smoke tests, and release assets pass.
- Tailscale supplies a network path. SSH still owns the Linux username, authentication, and host-key verification.
- Herdr/tmux provide remote continuity. Tray residency, autostart, or a reconnect loop cannot guarantee that a local Windows process never stops.
- The renderer is not a secret store. Passwords, private-key material, Tailscale auth keys, and updater private keys must never enter React persistence, ordinary logs, or committed fixtures.

## 2. Safe repository orientation

Read in this order:

1. README.md or README.zh-CN.md.
2. docs/README.md to choose the right audience document.
3. docs/ARCHITECTURE.md and the relevant ADR in docs/adr/.
4. docs/STATUS.md, docs/TEST-MATRIX-WINDOWS.md, and docs/RELEASE-MATRIX.md.
5. The owning crate/component, then its tests.

Never ingest or publish local references/, target/, dist/, node_modules/, generated sidecar executables, secrets, screenshots with real infrastructure, or machine-specific fixtures. A public diff must contain source, reproducible configuration, safe fixtures, and documentation—not a developer's build cache.

## 3. Workstation configuration semantics

| UI concept | Domain field | Meaning |
| --- | --- | --- |
| Name | label | Human-readable local label only |
| User | username | Final Linux SSH account, never inferred from Windows |
| Port | port | Final SSH port; normally 22 |
| Address candidates | addresses[] | Ordered route candidates; lower priority is tried first, disabled entries are skipped |
| Address type | addresses[].kind | Lan, Tailscale, Public, JumpHost, or Manual; route metadata only |
| Default remote folder | default_remote_path | Absolute Linux path used by Files and asset paste; must begin with / |
| Authentication | auth_mode | Password, PublicKey, SshAgent, or KeyboardInteractive |
| Private key file | private_key_path | Local path metadata; key bytes never belong in the Host record |
| Saved credential | auth_ref | Opaque native-secret-store reference, never a password or auth key |
| Jump host | jump | Optional bastion hostname, port, and username; final target stays in addresses[] |
| Tailscale | tailscale | SystemDaemon or EmbeddedUserspace route discovery configuration |
| Runtime | default_runtime | Tmux, Herdr, or PlainShell remote session behavior |

Validation rules are part of the product contract: ports must be 1–65535; an address must be non-empty; the default remote folder must be absolute; an enabled workstation needs a usable address; and a host-key change is a hard failure. Do not weaken these rules merely to make a fixture pass.

### Safe synthetic example

Use a documentation-only example like this. It is not a real host and must never be replaced with a user's live values in a committed file:

~~~text
Name: Example GPU host
User: testuser
Port: 22
Address: 203.0.113.10
Address type: Manual
Default remote folder: /home/testuser/project
Authentication: PublicKey
Private key file: C:\Users\test\.ssh\id_ed25519
Runtime: Tmux
Tailscale: Disabled
Jump host: Disabled
~~~

### Copy-paste configuration runbook

When a user gives an Agent connection details, never guess missing values. Normalize the request into this record first, masking secrets in any output:

~~~text
label: <local name>
username: <Linux account>
port: <1-65535>
addresses: [{kind: Manual|Lan|Tailscale|Public, value: <host>, enabled: true}]
default_remote_path: /home/<user>/<project>
auth_mode: Password|PublicKey|SshAgent|KeyboardInteractive
runtime: PlainShell|Tmux|Herdr
tailscale_mode: Disabled|SystemDaemon|EmbeddedUserspace
jump_host: disabled|configured
~~~

Then execute the smallest safe configuration:

1. Reject the record if the address, username, port, or absolute remote path is missing or ambiguous.
2. Start with one enabled address, no jump host, no fallback, and the authentication method the user explicitly selected.
3. Save the workstation and verify the persisted non-secret fields; do not echo passwords, private-key contents, or Tailscale Auth Keys.
4. Connect and capture only state names and sanitized error categories: resolving → connecting → host-key verification → authenticating → ready.
5. Run `pwd`, verify the Files root equals `default_remote_path`, and verify a harmless write/delete test only inside that directory.
6. Test clipboard round-trip, image/PDF upload, and the selected Herdr/tmux runtime separately; each is an independent acceptance item.
7. Add Tailscale, fallback addresses, jump host, extra panes, or background Actions one at a time, rerunning the relevant check after each change.
8. Report a table with `passed`, `failed`, `blocked`, or `not tested`, the evidence type, and the next safe action. Never convert `blocked` or `not tested` into `passed`.

If the user provides a real credential in chat, do not copy it into a file, command line, issue, log, screenshot, or final response. Use it only through the intended secret input flow, and recommend rotation if exposure is possible.
### Mode decision matrix

| Test condition | Configuration to exercise | Evidence required |
| --- | --- | --- |
| Direct route | One Manual/LAN candidate | SSH connection, prompt, pwd |
| System Tailscale | SystemDaemon + Tailscale candidate | typed peer discovery and SSH over peer address |
| Embedded Tailscale | EmbeddedUserspace + masked test credential | hidden sidecars, registration result, no visible console, then SSH |
| Jump host | jump plus final target candidate | bastion and final channel both succeed |
| Durable runtime | Tmux or Herdr | disconnect/reconnect attaches to same remote work |
| Plain shell fallback | PlainShell | ordinary prompt without runtime discovery |

Never claim that a mode works because its form can be saved. It works only after the observable evidence in the table is recorded.

## 4. Configuration and acceptance procedure

When asked to configure or verify a workstation, use this sequence:

1. Start with one safe synthetic or protected test host and one address candidate.
2. Validate the saved model: port, absolute path, non-empty address, authentication mode.
3. Connect and record the state transitions: resolving, connecting, host-key verification, authentication, ready.
4. Verify a prompt and pwd; verify Files opens default_remote_path.
5. Verify terminal text selection reaches the Windows clipboard by pasting into a separate trusted app.
6. Verify explicit image/PDF paste uploads to a writable directory and inserts the remote path.
7. Verify the selected Herdr/tmux runtime only if the same SSH login environment can find it.
8. Verify disconnect/reconnect; for durable runtime, verify the same remote session remains.
9. Add fallback addresses, Tailscale, jump hosts, or extra panes one variable at a time.
10. Save sanitized evidence and clearly label each item as automated, native GUI, protected real-network, or unverified.

An agent must not declare configured successfully from a compile result, a saved JSON object, a mocked success, or a screenshot of a connected label alone.

## 5. Build and quality gates

From the repository root on Windows PowerShell:

~~~powershell
npm ci
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm run lint
npm run test:frontend
npm run build
npm audit --omit=dev
~~~

Run the repository secret scan and git diff --check before staging. A passing compiler is not a GUI, network, installer, sleep/resume, or real-host validation result.

## 6. Implementation boundaries

- Domain models and state machines must not depend on Tauri.
- Tauri commands are thin adapters into Rust core.
- Terminal and transfer streams use bounded channels and batching; never emit one IPC message per character.
- Connection, transfer, and run states use explicit enums and generations; stale events cannot overwrite a newer connection.
- Clipboard reads require an explicit user paste action. OSC 52 is write-only and size-limited.
- Large files are streamed through a temporary partial file and completed atomically.
- SSH host-key changes stop the connection; no accept-any-key shortcut is acceptable.
- Embedded Tailscale processes must be hidden, cancellable, and diagnosed without flashing command windows.
- Local PowerShell, CMD, and WSL PTY sessions are separate from remote SSH sessions and must not leak remote credentials into local logs.

## 7. Change procedure

1. Reproduce or define an observable failure.
2. Add the smallest regression test that fails before the fix.
3. Modify the owning module without moving unrelated code.
4. Run focused tests, then every gate above.
5. For UI changes, launch the packaged app and test Windows scaling, resize, CJK/IME, keyboard focus, clipboard, paste assets, and the last terminal row.
6. Update the English and Chinese user guides only with behavior verified in code or on the target OS.
7. Scan the staged diff for secrets, real addresses/users, generated artifacts, and license conflicts.
8. Use a feature branch and pull request. Never force-push protected main.

## 8. Release procedure

Follow docs/RELEASE-MATRIX.md. A release requires a clean tag, passing CI, a newly built installer, checksum, updater signature, license notices, installation/upgrade/uninstall smoke tests, and an explicit statement of Authenticode status. The updater signature is not the same thing as Authenticode. Public updater hosting must be reachable and its manifest/signatures must be verified before claiming automatic updates are available. Never reuse an old MSI because a new bundle failed.

## 9. Evidence and reporting format

Every handoff or release note should include:

- commit/tag and exact artifact names;
- automated gates and their exit status;
- native Windows checks performed;
- protected real-network checks, if any, with infrastructure redacted;
- known unverified areas;
- security/privacy scan result; and
- rollback or recovery instructions.

Use not tested, not available, and not released precisely. Do not turn an absent test environment into a claim of failure, and do not turn a green unit test into a claim of real-network success.

## 10. Test credentials and remote systems

Use documentation-range addresses such as 203.0.113.10, synthetic users, and fake SSH/SFTP servers in committed fixtures. Real Tailscale keys and remote-host credentials belong only in protected, short-lived test environments. Revoke any key pasted into chat or accidentally logged. Never print or echo secret values while diagnosing a failure.

