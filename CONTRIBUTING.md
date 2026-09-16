# Contributing to KodeWork

Thanks for considering a contribution. KodeWork welcomes focused bug fixes, tests, documentation improvements, accessibility work, performance work backed by evidence, and well-scoped product changes.

The project handles SSH credentials, host identity, remote execution, file transfer, clipboard data, and updater trust. Contributions are expected to preserve those boundaries and to be precise about what was actually tested.

## Before you start

Please read:

1. [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — system boundaries and data flow;
2. [`docs/STATUS.md`](docs/STATUS.md) — current release scope and known gaps;
3. [`docs/RELEASE-MATRIX.md`](docs/RELEASE-MATRIX.md) — what counts as verified/released on each platform;
4. the relevant ADR under [`docs/adr/`](docs/adr/) for the subsystem you are changing;
5. [`SECURITY.md`](SECURITY.md) when the change touches authentication, credentials, host identity, remote execution, CI, dependencies, signing, or update delivery.

The [documentation map](docs/README.md) links the maintained user, engineering, support, testing, and release references. Repository history remains available through Git rather than transient handoff files in the current documentation tree.

For user support rather than code contribution, start with [`SUPPORT.md`](SUPPORT.md).

## Development environment

The current desktop development baseline is:

- Windows 10/11 x64 for native desktop work;
- Rust **1.98.0**, pinned by [`rust-toolchain.toml`](rust-toolchain.toml), with the MSVC toolchain on Windows;
- Node.js **24+** and npm;
- Tauri 2 Windows prerequisites;
- Git.

Portable Rust crates are also checked in CI on Linux and macOS. That does not make those operating systems released desktop targets.

### First-time setup

```powershell
git clone https://github.com/likangmax/KodeWork.git
cd KodeWork
npm ci
cargo build --locked --workspace
```

For the native desktop app:

```powershell
npm run desktop
```

For the browser-only UI preview, which does not provide native SSH/credential behavior:

```powershell
npm run dev
```

## Choose a contribution

Good contributions start with a concrete observable problem.

- Check [open issues](https://github.com/likangmax/KodeWork/issues) first.
- For a new bug, use the structured bug-report form and remove secrets/private infrastructure from logs and screenshots.
- For a feature, describe the user problem before proposing an implementation.
- For security vulnerabilities, **do not open a public issue**; follow [`SECURITY.md`](SECURITY.md).
- For usage/help requests, use [`SUPPORT.md`](SUPPORT.md) and Discussions rather than opening a product bug without a reproduction.

If an issue is small enough for a first contribution, maintainers may label it `good first issue`. A label is an invitation to work on that issue, not a promise that every proposed implementation will be merged.

## Branch and commit style

Create a focused branch from current `main`:

```powershell
git switch main
git pull --ff-only
git switch -c fix/short-description
```

Common prefixes are:

- `feat:` user-visible feature;
- `fix:` bug fix;
- `docs:` documentation only;
- `test:` tests only;
- `refactor:` behavior-preserving code change;
- `perf:` measured performance change;
- `chore:` maintenance;
- `security:` security-sensitive fix.

Keep unrelated refactors out of focused fixes. Avoid committing generated output, caches, machine-specific fixtures, real hostnames, credentials, signing keys, or private files.

## Architecture and safety rules

The most important invariants are:

- Rust owns connection truth, reconnect generations, authentication boundaries, transfer state, and remote-session continuity.
- React owns presentation and renderer lifecycle; it must not become a credential store or the final authority for dangerous actions.
- Unknown SSH host keys require an explicit trust decision; changed keys remain hard failures.
- Fail closed when identity/trust stores cannot be read.
- Passwords, private-key material/passphrases, Tailscale auth keys, updater private keys, and private clipboard/file contents must not enter normal logs or committed fixtures.
- Terminal and transfer streams must remain bounded/batched instead of emitting one IPC event per byte/character.
- Large transfers stay streamed/staged; convenience changes must not silently weaken integrity or atomicity.
- Product claims must follow evidence. A unit test is not proof of real-network, installer, GUI, sleep/resume, or platform-release behavior.

Use [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), [`SECURITY.md`](SECURITY.md), and [`docs/RELEASE-MATRIX.md`](docs/RELEASE-MATRIX.md) as the maintained references for these boundaries.

## Supply-chain, CI, and publishing rules

Treat dependency, workflow, and publishing changes as security-sensitive maintenance.

- Use synthetic fixtures and obvious placeholder credentials; never add real secrets, private terminal content, or infrastructure details to tests or docs.
- Justify new direct dependencies and review provenance, maintenance history, install/build hooks, transitive changes, and lockfile diffs.
- Keep `package-lock.json` and `Cargo.lock` reproducible; use `npm ci` and Cargo `--locked` in verification/release paths.
- Pin third-party GitHub Actions to reviewed full commit SHAs, not floating tags.
- Keep workflow/job permissions least-privilege; do not expose secrets or write-capable tokens to untrusted pull-request code or artifacts.
- Do not weaken required checks, secret scanning patterns, RustSec/npm audit gates, release-lineage checks, updater signatures, or Authenticode requirements merely to make a build pass.
- A configured workflow is not proof that external certificate chains, signing secrets, updater hosting, or registry/release bindings are operational. Document verified evidence separately.
- Ordinary dependency version updates use a cooldown; security updates remain eligible immediately and still require review/checks.

## Testing a change

Start with the smallest relevant test, then make a best effort to run the full gates before opening a PR.

```powershell
npm ci
npm run lint
npm run test:frontend
npm run build
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
npm audit --omit=dev --audit-level=high
git diff --check
```

CI also performs RustSec/dependency policy, tracked-secret pattern checks, and portable Rust checks.

### Evidence categories

Be explicit about what kind of verification you performed:

- **automated** — unit/integration/build/lint checks;
- **native Windows** — packaged or development desktop behavior on Windows;
- **protected real-network** — SSH/Tailscale/jump-host testing against non-public test infrastructure, with sensitive details removed;
- **release artifact** — installer/signature/checksum/lineage checks against the intended release artifact;
- **not tested / blocked** — required evidence was unavailable.

Do not convert `not tested` into `passed`.

## Pull requests

A good PR should explain:

- the observable problem or user need;
- the smallest implementation change that addresses it;
- tests/checks run and their results;
- native, real-network, or release-artifact checks, if relevant;
- security/privacy/supply-chain implications;
- what remains unverified;
- related issue(s), when applicable.

Use the repository PR template. Screenshots and recordings are welcome for UI work, but sanitize hostnames, usernames, file paths, terminal output, credentials, and private infrastructure first.

Maintainer response times vary with scope and availability; the repository does not guarantee a fixed review SLA. Small, reproducible, well-tested PRs are easier to review.

## Documentation changes

Documentation should describe verified behavior, not intended behavior.

When changing user-visible behavior:

- update the relevant English and Chinese guide when practical;
- update [`docs/STATUS.md`](docs/STATUS.md) only when release scope or verified boundaries actually change;
- update [`docs/RELEASE-MATRIX.md`](docs/RELEASE-MATRIX.md) for packaging/support evidence changes;
- update [`ROADMAP.md`](ROADMAP.md) when public direction meaningfully changes;
- update [`docs/CHANGELOG.md`](docs/CHANGELOG.md) for user-visible or release-relevant changes;
- avoid copying transient commit SHAs, local working-tree state, or time-sensitive test counts into durable contributor instructions.

Use the words `configured`, `tested`, `verified`, `supported`, and `released` deliberately. They are not interchangeable.

## Release changes

Do not publish releases, overwrite release assets, change signing material, or claim new platform support as part of an ordinary contribution.

Stable release work must follow [`docs/RELEASE-MATRIX.md`](docs/RELEASE-MATRIX.md) and the existing release workflow. Tauri updater signatures and Windows Authenticode are separate trust layers.

Release/workflow paths are owned through `.github/CODEOWNERS`; CODEOWNERS expresses ownership but does not by itself prove branch-protection enforcement.

## Security reporting

Never put suspected vulnerabilities, passwords, SSH private keys, Tailscale auth keys, signing material, or private host details into a public issue or PR.

Follow [`SECURITY.md`](SECURITY.md) for private reporting.

## Code of conduct

Participation is governed by [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md). Be respectful, specific, and constructive when reviewing or discussing contributions.
