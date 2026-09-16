# Support

KodeWork is an actively developed open-source `0.x` project. Support is community/maintainer based and does not include a guaranteed response-time SLA.

## Choose the right channel

| Need | Where to go |
| --- | --- |
| Installation, connection, terminal, file-transfer, or configuration help | [User guide](docs/USER-GUIDE.md) and [Troubleshooting](docs/TROUBLESHOOTING.md) first, then [GitHub Discussions](https://github.com/likangmax/KodeWork/discussions) |
| Reproducible product bug | [Bug report form](https://github.com/likangmax/KodeWork/issues/new/choose) |
| Feature / workflow proposal | [Feature request form](https://github.com/likangmax/KodeWork/issues/new/choose) or Discussions |
| Contribution / development question | [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/README.md](docs/README.md) |
| Suspected vulnerability | [SECURITY.md](SECURITY.md) — never a public issue |
| Conduct concern | [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) |

## Supported desktop scope

The currently released desktop target is **Windows 10/11 x64**. Portable Rust checks on Linux/macOS are engineering evidence, not a promise of native Linux/macOS desktop support.

The source repository may be ahead of the latest published installer. For installation support, always identify the exact GitHub Release/tag you are using rather than assuming the version in `main` is publicly available.

## Before asking for help

Please collect only information that is safe to share publicly:

- KodeWork release/tag or source commit;
- Windows version and architecture;
- connection path type (direct/LAN, Tailscale, fallback address, SSH jump host) without real private addresses;
- authentication mode without passwords, private keys, passphrases, MFA answers, or tokens;
- minimal reproduction steps;
- expected vs. actual behavior;
- sanitized logs/screenshots when necessary.

Replace real usernames, hostnames, IP addresses, file paths, project names, terminal content, account identifiers, and private infrastructure details with synthetic placeholders.

## Please do not post

Do not put any of the following in Issues, Discussions, PRs, screenshots, recordings, or logs:

- SSH passwords or private keys;
- private-key passphrases or keyboard-interactive/MFA answers;
- Tailscale auth keys or account identifiers;
- updater signing keys, certificate material, tokens, cookies, or API keys;
- sensitive terminal output or private files;
- real infrastructure details that are not required to reproduce a public bug.

If the problem could expose another user's data, bypass authentication/trust, execute unintended commands, weaken signing/update trust, or leak credentials, stop and follow [SECURITY.md](SECURITY.md) instead.

## Maintainer expectations

Small, reproducible reports with sanitized evidence are easier to triage. Maintainers may close issues that are duplicates, unsupported-platform requests without a concrete proposal, missing essential reproduction information, or security reports that should move to a private channel.

For project direction and support boundaries, see [ROADMAP.md](ROADMAP.md), [docs/STATUS.md](docs/STATUS.md), and [docs/RELEASE-MATRIX.md](docs/RELEASE-MATRIX.md).
