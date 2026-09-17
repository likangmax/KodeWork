## Problem / user need

What observable problem or maintainer need does this PR address?

## What changed

Describe the smallest relevant implementation/documentation change. Keep unrelated refactors out of a focused PR.

## Related issue(s)

Closes #
Related to #

## Verification

List the checks you actually ran and their results. Do not mark native, real-network, or release behavior as verified from unit tests alone.

- [ ] Focused tests for the changed area
- [ ] `npm ci`
- [ ] `npm run lint`
- [ ] `npm run check:docs`
- [ ] `npm run test:frontend`
- [ ] `npm run build`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --locked --workspace --all-features`
- [ ] `npm audit --omit=dev --audit-level=high`
- [ ] `git diff --check`

### Evidence category

Check only what was actually exercised:

- [ ] Automated tests/build/lint
- [ ] Native Windows desktop behavior
- [ ] Protected real-network SSH/Tailscale/jump-host behavior
- [ ] Installer/upgrade/uninstall behavior
- [ ] Release artifact/signature/checksum behavior
- [ ] Not applicable

**Not tested / blocked:**

Describe any evidence that was unavailable. `Not tested` is acceptable; do not convert it into `passed`.

## Security and privacy

- [ ] No passwords, private keys/passphrases, Tailscale auth keys, signing keys, real hostnames, private files, or sensitive terminal output are included
- [ ] Synthetic/documentation-range fixtures are used where public examples are needed
- [ ] SSH host-key / identity behavior remains fail-closed where applicable
- [ ] Renderer/UI code does not become a credential store or the final authority for dangerous actions
- [ ] Security-sensitive behavior has regression coverage or an explicit reason why it cannot be automated

Describe security/privacy impact, or write `None`:

## Dependencies / CI / release trust

Complete this section when the PR touches dependencies, lockfiles, workflows, signing, packaging, or releases.

- [ ] New/updated dependencies were justified and lockfile/transitive changes were reviewed
- [ ] Third-party GitHub Actions remain pinned to reviewed full commit SHAs
- [ ] Workflow/job permissions remain least-privilege
- [ ] Untrusted PR code/artifacts do not gain access to secrets or write-capable publishing credentials
- [ ] Dependency/RustSec/secret gates, release-lineage checks, updater signatures, and Authenticode requirements were not weakened for convenience
- [ ] External provisioning (certificates, secrets, updater endpoints, repository settings) is not claimed as verified solely because workflow YAML exists

Notes / not applicable reason:

## Documentation / product claims

- [ ] User-facing docs were updated if behavior changed
- [ ] English/Chinese docs remain consistent where applicable
- [ ] `configured`, `tested`, `verified`, `supported`, and `released` are not used interchangeably
- [ ] No new platform/release capability is claimed without the required evidence
- [ ] `docs/STATUS.md`, `docs/RELEASE-MATRIX.md`, `ROADMAP.md`, or `docs/CHANGELOG.md` were updated if their source-of-truth scope changed

## Screenshots / demo

For UI work, add sanitized screenshots/recordings when useful. Remove usernames, hostnames, addresses, file paths, terminal contents, credentials, account identifiers, and private infrastructure details.

## Breaking changes

- [ ] This PR introduces a breaking change

If checked, describe the migration path.

## Additional notes

Anything reviewers should know that is not covered above.
