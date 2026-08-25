# Security policy

## Supported versions

Only the latest tagged release and the current default branch receive security fixes. Old MSI files should not be used for handling new credentials.

## Reporting a vulnerability

Please use GitHub's private vulnerability reporting / Security Advisory flow once it is enabled for this repository. If that feature is unavailable, contact the repository maintainer privately through the account that owns the repository. Do **not** open a public issue for a suspected vulnerability.

Include the affected version or commit, Windows version and installation mode, minimal reproduction steps, impact and required privileges, and logs with credentials, auth keys, private keys and hostnames removed.

We will acknowledge receipt, reproduce the issue, coordinate a fix, and publish a concise advisory after a fix is available. Never paste Tailscale auth keys, SSH passwords, private keys, or updater signing material into an issue or pull request.

## Runtime security boundaries

- Credential bytes are materialized only for an in-flight native connection
  attempt and are reacquired from the OS secure store for later attempts. They
  are not retained by the reconnect supervisor, renderer, SQLite, or logs.
- SSH failures have a typed policy kind (network, timeout, authentication,
  host-key, credential-required, configuration, or protocol). Retry behavior
  must never depend on localized diagnostic text.
- A remote Run is `Unknown` whenever the client cannot prove its business
  outcome. A started marker alone does not prove liveness, and a local wait
  timeout does not prove that a remote process was killed.
- Terminal output and credentials are kept out of durable Run history. When
  reporting diagnostics, include only safe error kind/detail, host identity,
  generation, and timestamps; never include passwords, passphrases, auth keys,
  keyboard-interactive answers, or private clipboard contents.
- Herdr bridges are owned by the SSH exec channel and addressed by a scoped
  `BridgeId`; cleanup must close that owner and its loopback tunnel rather than
  searching for processes by command-line text.
