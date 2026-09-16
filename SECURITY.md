# Security policy

KodeWork handles SSH identity, credentials, remote command execution, file transfer, clipboard data, tunnels, and release-signing boundaries. Security reports are therefore handled separately from ordinary bugs and feature requests.

## Supported versions

| Version / branch | Security support |
| --- | --- |
| Latest release published on GitHub Releases | Supported |
| Current `main` | Receives fixes, but may be ahead of the latest published binary |
| Older releases | Best effort only; upgrade to the latest release before handling new credentials |

A version number in the source tree does not prove that a matching installer has been released. Use [GitHub Releases](https://github.com/likangmax/KodeWork/releases) as the distribution source of truth.

## Reporting a vulnerability

**Do not open a public issue, pull request, or discussion with vulnerability details.**

Preferred private reporting path:

1. Open the repository **Security** tab and use **Report a vulnerability** when that private GitHub flow is available.
2. If the repository does not expose a private reporting form, use a private contact method published on the repository owner's GitHub profile.
3. If no private channel is available, do not post exploit details publicly. Share only a request for private contact, without reproduction steps, credentials, host details, or sensitive logs.

Include, when relevant:

- affected release/tag or source commit;
- Windows version and installation mode;
- a concise description of security impact and required privileges;
- sanitized reproduction steps;
- whether the issue affects SSH identity, credentials, remote execution, SFTP, clipboard, tunnels, updater/signing, CI, or release infrastructure;
- sanitized logs or screenshots with usernames, hostnames, paths, tokens, keys, terminal content, and private infrastructure removed.

Never include live SSH passwords, private keys, passphrases, Tailscale auth keys, updater private keys, Authenticode material, cookies, tokens, or private user files.

## Coordinated disclosure

The maintainer will make a best effort to:

1. acknowledge the report through the private channel;
2. reproduce and assess affected versions and impact;
3. prepare a focused fix and regression coverage;
4. coordinate release timing with the reporter when appropriate;
5. publish a concise advisory after a fix is available, when public disclosure is warranted.

This project does not promise a fixed response SLA. Please keep undisclosed vulnerability details private until disclosure is coordinated.

## Security-sensitive contribution rules

Changes involving authentication, host-key trust, credential storage, command execution, file transfer, clipboard data, network destinations, redirects, dependencies, CI, signing, or releases require focused security review appropriate to the affected boundary.

- Use synthetic users, documentation-range IP addresses, placeholder credentials, and sanitized fixtures.
- Never place real credentials, private infrastructure, secret-bearing logs, or private terminal/file content in source, tests, snapshots, documentation, issues, or PRs.
- Review direct and transitive dependency changes, lockfile changes, package provenance, and install/build hooks.
- Pin third-party GitHub Actions to reviewed full commit SHAs and keep workflow permissions least-privilege.
- Keep secrets and write-capable tokens away from untrusted pull-request code.
- Do not disable security checks, secret protections, release-lineage checks, or signing requirements merely to obtain a green build.
- Treat workflow configuration as configuration, not proof that external secrets, certificate chains, registry bindings, or release provenance are correctly provisioned.

## Runtime security invariants

- Unknown SSH host keys require an explicit trust decision; changed known keys are hard failures.
- Host-key store read failures block verification instead of being treated as an unknown key.
- Credential bytes are materialized only for an in-flight native connection attempt and are reacquired from the OS secure store for later attempts; they are not retained by the reconnect supervisor, renderer, normal SQLite state, or logs.
- SSH failures use typed policy kinds rather than localized diagnostic text for retry decisions.
- A remote Run is `Unknown` whenever the client cannot prove its business outcome; launcher or transport success is not command success.
- Terminal output and credentials are kept out of durable Run history.
- Dangerous Actions are classified/enforced by the Rust side rather than trusting renderer state as the final authority.
- SFTP writes remain streamed/staged and transfer identity is revalidated before final commit.
- Web Preview uses explicit loopback SSH forwarding rather than exposing a listener on all interfaces.

For architecture context, see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/STATUS.md](docs/STATUS.md).

## What belongs elsewhere

General bugs, feature requests, usage questions, performance suggestions, and non-sensitive hardening ideas should use [SUPPORT.md](SUPPORT.md), GitHub Issues, or Discussions. When in doubt, choose the private security path first and let the maintainer reclassify the report.
