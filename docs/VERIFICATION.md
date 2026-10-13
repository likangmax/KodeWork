# Verification report — 2026-10-13 audit and remediation

This document records what was verified about this repository, how each claim was
produced, and what is deliberately **not** verified. It exists so that a reviewer
can re-run the evidence instead of trusting a summary.

Baseline: `3129c01` (the merge of PR #85). Work under review: the 16 commits after
it, ending at the commit that introduced this file.

## 1. Automated gates

CI (`.github/workflows/ci.yml`) runs six jobs; the `rust` context is an aggregate
that requires Windows Rust, Dependency and secret policy, and Desktop build.

| Job | What it proves |
| --- | --- |
| `frontend` | locked install, oxlint (0 warnings), local documentation links, catalogue/version checks, vitest, `node --test` for the release scripts, production build |
| `windows-rust` | sidecar preparation, `cargo fmt --check`, locked clippy with `-D warnings`, full locked workspace tests, two native PowerShell ConPTY readiness repeats |
| `desktop-build` | `tauri build --no-bundle` in the release profile — the Tauri CLI version preflight, codegen, frontend build and release link that no other context exercises |
| `dependency-security` | production `npm audit`, `cargo audit` (RustSec), tracked-secret patterns, and the shipped-graph proof described in §4 |
| `portable-rust` (Linux, macOS) | the core crates build and test off Windows |
| `rust` | aggregate gate for the three jobs above |

Local equivalent of the whole set, on a clean checkout:

```
npm ci && npm run lint && npm run check:docs && npm run check:versions \
  && npm run check:shipped-graph && npm run test:frontend && npm run build
node --test scripts/verify-updater-signature.test.mjs
node --test scripts/check-tauri-versions.test.mjs
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
```

Last observed result at the time of writing: **224 Rust tests passed, 0 failed**
(47 test binaries), 52 frontend tests, fmt and clippy clean, all script suites
green, six-job CI success.

## 2. Long-run reliability

`scripts/run-soak-matrix.ps1 -Hours 1 -IncludeLargeTransfer`, with its own
`CARGO_TARGET_DIR`, on a frozen tree:

- **1364 of 1364 gates passed, 0 failures, 3631 s (60.5 min)**;
- includes 20 concurrent PTY panes, network drop/reconnect state tests, the
  25-test transfer pause/resume/retry/cancel matrix, and a 512 MiB streaming
  transfer;
- the stability loop completed 680 SSH iterations plus 680 SFTP iterations.

The script itself had to be fixed first: a script-wide
`ErrorActionPreference = 'Stop'` turns cargo's stderr progress output into a
terminating error under Windows PowerShell 5.1, so it had only ever run under
pwsh 7, and its stability loop checked one exit code for two commands.

## 3. Reproducibility of the committed state

A pristine `git clone` of the pushed commit, with a cold target directory,
`npm ci`, and no other preparation:

- all 11 frontend and Rust gates pass, `FAILED STEPS: 0`;
- repeated for two different commits during the session with the same result.

## 4. Dependency and shipped-artifact claims

- `cargo tree --target x86_64-pc-windows-msvc` contains **no** `glib`, `gtk`,
  `atk`, `gdk`, `webkit2gtk` or `libappindicator`; the same crates are present in
  the Linux graph. `scripts/check-shipped-graph.mjs` enforces both directions
  (absence in the shipped graph, presence in the unshipped one, so a broken cargo
  invocation cannot make the check vacuous) and runs in CI. Mutation check:
  adding `tauri` — a crate that *is* in the shipped graph — makes it fail.
- `tauri-plugin-updater` is pinned to `>= 2.13`, the first line that implements
  `requireSignedVersion`. With the previous `"2.10.1"` requirement,
  `cargo update -p tauri-plugin-updater --precise 2.11.0` was accepted and would
  have shipped a build where the flag is present but inert; it is now rejected.
- Every workspace member builds in isolation (`cargo check -p <crate>`); two
  crates previously failed because they used `tokio::process` without declaring
  the `process` feature and only compiled through feature unification.

## 5. Update channel

`scripts/verify-release.ps1` fetches a published `latest.json`, downloads the
artifact it points at, and verifies the minisign signature **and** the version
bound into the signed trusted comment. The channel as a whole is rehearsed by
publishing a real signed MSI into a local tree, serving it over TLS, and then
verifying it — plus the three ways a channel can lie. Last result, over
`https://127.0.0.1` with a locally trusted CA and no skipped certificate check:

```
PASS  publish: signed artifact lands in the channel
PASS  serve: manifest is reachable over TLS
PASS  verify: manifest + artifact verify end to end   (version + SHA-256)
PASS  negative: a rollback is detected                (expected-version mismatch)
PASS  negative: a re-labelled manifest is refused     (signed version still old)
PASS  negative: a tampered artifact is refused        (minisign fails)
PASS  verify: restored artifact verifies again
PASS  config restored byte for byte
```

Direct evidence for the version binding, with the matching key: announcing 0.2.5
or 0.2.3 for a 0.2.4-signed artifact exits 1 with `Authenticated signed updater
version does not match the announced version`; announcing 0.2.4 exits 0.

The client enforces the same property: `tauri-plugin-updater` 2.13.1
(`updater.rs`, `verify_signature`) verifies the artifact first and only then reads
the signature's trusted comment to compare the signed version with the announced
one, failing closed with `MissingSignedVersion` when the signature carries none
and `requireSignedVersion` is set.

The manifest carries the artifact under **both** `windows-x86_64-msi` and
`windows-x86_64`, because the plugin searches `{os}-{arch}-{installer}` first and
falls back to `{os}-{arch}`; publishing both spellings removes the dependency on
the fallback, and the generator refuses a manifest whose two keys disagree.

## 6. Installed-client migration

Whether an installed copy migrates is a property of the MSI database, and it was
checked against the copy actually installed on the test machine (Kodework Windows
0.2.2, ProductCode `{E091E48C-A611-48D7-8676-7E4FDF858B4E}`) **without installing
or uninstalling anything**, by reading the Windows Installer databases:

- UpgradeCode matches the installed product, so the package is treated as an
  upgrade rather than a parallel product;
- ProductCode differs, so it is a major upgrade rather than a re-install;
- ProductVersion is greater (0.2.2 → 0.2.4) and the `Upgrade` table covers the
  installed version (`attrs=257`, action `WIX_UPGRADE_DETECTED`);
- `RemoveExistingProducts` is scheduled in `InstallExecuteSequence`
  (sequence 1501) — WiX's `MajorUpgrade` removes the previous version through
  that action, not through the `Upgrade` table's `Remove` column;
- `ALLUSERS=1`, matching the installed scope.

## 7. Behavioural fixes with mutation-checked tests

Each of these shipped with a test, and the test was shown to fail when the fix is
reverted (mutation), so the suite bites rather than merely passing:

| Fix | Mutation that must fail the test |
| --- | --- |
| `shell_quote` quoting rule | remove the single-quote escaping → `sh` reports `unexpected EOF while looking for matching '` |
| interactive prompt budget | use the transport timeout for authentication again → the budget test reports the transport timing |
| updater verifier invoked through a path alias | restore the naive `import.meta.url` comparison → the alias regression test fails |
| keyboard-interactive answers that cannot be delivered | delete either wipe → "an answer whose prompt is gone must be wiped" / "an answer with no waiting prompt must be wiped" |
| renderer effect cleanup | delete a `cancelAnimationFrame` → "registers requestAnimationFrame without a matching dispose" |
| shipped dependency graph | add a crate that is in the shipped graph → "FAIL tauri is in the x86_64-pc-windows-msvc graph" |

Other invariants added in the same style: dead i18n catalogue keys and catalogue
parity, `danger_policy` (whitespace/case evasion, composition never Safe, the
observational set stays Safe), and a source scan for `unreachable!`/`todo!`/
`unimplemented!` in production sources (the library crates also carry
`#![cfg_attr(not(test), deny(clippy::unreachable))]`; `src-tauri` cannot, because
`#[tauri::command]` expands to `unreachable!()`).

## 8. Native Windows observations

Measured on the release binary produced by `scripts/build-release.ps1`
counterparts, using `scripts/measure-startup.ps1`:

- window creation 144–740 ms (p50 158 ms), idle RSS ≈ 27 MB;
- the local PTY suite, including the real ConPTY round trip, passed 20 consecutive
  runs (1.52–3.22 s each).

## 9. Not verified

- **Public update endpoint** (`updates.kodework.dev`): no DNS, origin or bucket is
  provisioned. The channel format, transport (locally over TLS), signature and
  version binding are verified; the origin itself is an operational step.
- **Authenticode**: requires a commercial certificate; every rehearsal passes
  `-RequireAuthenticode:$false` deliberately.
- **A real install/upgrade on a machine**: not performed, because a Kodework
  installation already exists on the test machine and mutating it was out of
  scope. The MSI-level contract in §6 is the evidence instead.
- **The in-app update check button**: driving it requires UI automation, which was
  deliberately not performed; the client-side logic it calls is covered by §5
  (source reading plus an equivalent end-to-end verification).
- **Linux/macOS desktop bundles**: not built or shipped; the portable CI jobs
  cover the core crates only.
- **The `glib` advisory**: still open upstream (gtk 0.18 requires glib ^0.18, the
  fix is 0.20.0, and only Tauri moving to gtk-rs 0.20 can reach it). It is not in
  the shipped artifact, which §4 enforces mechanically.

## 10. Process notes

- The 16 commits were pushed **directly to `main`**, not through pull requests:
  `enforce_admins` is `false`, so an owner push bypasses the required checks. They
  were validated by CI on `main` afterwards, but there is no review trail. To make
  that class of mistake impossible rather than discouraged:
  `gh api -X PUT repos/likangmax/KodeWork/branches/main/protection/enforce_admins`.
- The harness scripts used for the one-off checks in §2, §3, §5 and §6 live outside
  the repository as development aids; the method and the exact commands are
  described above so they can be re-created, and the two that are meant to be
  re-run by maintainers (`scripts/verify-release.ps1`, `scripts/serve-update-channel.mjs`,
  `scripts/publish-update.ps1`) are in the tree.
