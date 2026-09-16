## Problem / user need

What observable problem or maintainer need does this PR address?

## What changed

Describe the smallest relevant implementation or documentation change.

## Related issue(s)

Closes #
Related to #

## Verification

List the checks you actually ran and their results. Do not mark native or real-network behavior as verified from unit tests alone.

- [ ] Focused tests for the changed area
- [ ] `npm run lint`
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
- [ ] Not applicable

**Not tested / blocked:**

Describe any evidence that was unavailable.

## Security and privacy

- [ ] No passwords, private keys/passphrases, Tailscale auth keys, signing keys, real hostnames, private files, or sensitive terminal output are included
- [ ] SSH host-key / identity behavior remains fail-closed where applicable
- [ ] Renderer/UI code does not become a credential store or the final authority for dangerous actions
- [ ] Security-sensitive behavior has regression coverage or an explicit reason why it cannot be automated

Describe security/privacy impact, or write `None`:

## Documentation / product claims

- [ ] User-facing docs were updated if behavior changed
- [ ] English/Chinese docs remain consistent where applicable
- [ ] No new platform/release capability is claimed without the required evidence
- [ ] `docs/STATUS.md`, `docs/RELEASE-MATRIX.md`, or `ROADMAP.md` were updated if their source-of-truth scope changed

## Screenshots / demo

For UI work, add sanitized screenshots or recordings when useful. Remove usernames, hostnames, file paths, terminal contents, credentials, and private infrastructure details.

## Breaking changes

- [ ] This PR introduces a breaking change

If checked, describe the migration path.

## Additional notes

Anything reviewers should know that is not covered above.
