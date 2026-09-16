# KodeWork troubleshooting

Use this guide for common Windows client, SSH, terminal, transfer, and installation problems. For the supported product surface, use [STATUS.md](STATUS.md); for installation and normal workflows, use the [user guide](USER-GUIDE.md).

## Before troubleshooting

Use the latest installer that is actually published on [GitHub Releases](https://github.com/likangmax/KodeWork/releases). The version in the `main` source tree can be newer than the latest installable release.

Keep diagnostics safe to share. Do not post passwords, private keys, passphrases, Tailscale auth keys, tokens, real private infrastructure, terminal contents, or private files. Sanitize screenshots and logs before opening an issue.

A changed SSH host key is a security boundary, not a routine connectivity error. Do not bypass it merely to restore a connection.

## Connection problems

### Cannot connect to a host

Check the selected address, port, username, and route first. From PowerShell, test the same SSH endpoint outside KodeWork:

```powershell
Test-NetConnection -ComputerName your-host.example.com -Port 22
ssh testuser@your-host.example.com
```

On the Linux host, verify that the SSH service is running and listening on the expected port:

```bash
systemctl status ssh || systemctl status sshd
ss -ltnp | grep ':22'
```

For Tailscale routes, verify the relevant device is online and reachable before treating the problem as a KodeWork failure. For a jump-host configuration, verify the jump host first, then the destination reachable from that jump host.

Do not change firewall or SSH authentication policy solely to make KodeWork connect. Confirm the existing server policy and intended access path first.

### Connection drops or reconnects repeatedly

Check whether Windows slept, the network interface changed, the VPN/Tailscale route changed, or the remote SSH service restarted. KodeWork distinguishes disconnected/reconnecting/failed states; a transport interruption is not treated as proof that a remote command succeeded or failed.

Useful checks:

```powershell
ping -t your-host.example.com
Get-NetAdapter | Where-Object Status -eq 'Up'
```

```bash
journalctl -u ssh --since '10 minutes ago' 2>/dev/null || \
journalctl -u sshd --since '10 minutes ago'
```

If the failure appears only after sleep/resume, include that fact in the bug report. Do not disable host-key verification or credential protections to work around reconnect problems.

### Host key changed

Stop and verify the server identity through a trusted channel. A legitimate change can happen after a server reinstall or deliberate SSH host-key rotation, but an unexpected change can also indicate that you reached a different machine or that the route was intercepted.

On the server console, for example, inspect the intended host-key fingerprint:

```bash
ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub
```

Compare that fingerprint with the value KodeWork shows. Only reset trust after you have independently verified the new key.

Do **not** edit KodeWork's SQLite host-key tables manually. If an intentionally rotated key cannot be reset through the normal workstation workflow, open a sanitized support/bug report rather than weakening or manually rewriting the trust store.

## Authentication problems

### Password or keyboard-interactive authentication fails

First verify that the same account is allowed to authenticate by the server. Inspect the effective SSH configuration instead of blindly enabling password authentication:

```bash
sshd -T | grep -E 'passwordauthentication|pubkeyauthentication|kbdinteractiveauthentication'
```

Also check account state and server authentication logs where you have permission:

```bash
passwd -S testuser
journalctl -u ssh --since '10 minutes ago' 2>/dev/null || \
journalctl -u sshd --since '10 minutes ago'
```

If MFA/OTP is expected, select keyboard-interactive authentication and answer only the prompts shown for that connection attempt.

### Private-key authentication fails

Confirm the selected key path and test the key with the Windows OpenSSH client:

```powershell
$key = "$env:USERPROFILE\.ssh\id_ed25519"
icacls $key
ssh-keygen -y -f $key | Out-Null
ssh -i $key testuser@your-host.example.com
```

On the remote host, the corresponding public key must be present in `~/.ssh/authorized_keys`, with permissions appropriate for the SSH server configuration. A common baseline is:

```bash
chmod 700 ~/.ssh
chmod 600 ~/.ssh/authorized_keys
```

For encrypted private keys, verify the passphrase with `ssh-keygen -y -f <key-path>` before diagnosing KodeWork.

### SSH Agent or Pageant is not supplying a key

For Windows OpenSSH Agent:

```powershell
Get-Service ssh-agent
ssh-add -l
```

If the service is intentionally used but stopped, start it and load the intended identity using normal Windows/OpenSSH administration. For Pageant, confirm the intended key is loaded in that Pageant instance. Then select **SSH Agent** in the workstation authentication settings.

## Tailscale problems

KodeWork can use either the user's existing system Tailscale installation or its configured embedded userspace path. These modes have different ownership and lifecycle boundaries.

If a Tailscale address is unavailable, first verify the selected mode and whether the expected device/address exists. A Tailscale failure should not be worked around by copying auth keys into issue text, logs, screenshots, shell history, or repository files.

If direct SSH works but the Tailscale route does not, report the route/mode difference with synthetic or redacted addressing rather than real tailnet details.

## SFTP and transfer problems

### Destination busy

KodeWork prevents concurrent writes to the same destination. Let the existing transfer finish or cancel it before starting another write to the same path.

### Source changed during upload

KodeWork revalidates source metadata before final commit. If the local source changes while it is being uploaded, retry after the source is stable. This behavior protects against publishing a file that no longer matches the bytes that were originally selected.

### Resume restarts instead of continuing

Resume verifies the existing partial prefix before continuing. If the partial destination does not match the current source, KodeWork must restart rather than append incompatible bytes.

### Transfers are unexpectedly slow

Separate network, CPU, disk, and route effects before changing application settings. Compare the same path with another trusted SSH/SFTP client and note whether the slowdown occurs only through a jump host or Tailscale route. For reproducible performance reports, include approximate file size and throughput without sharing private filenames or contents.

## Remote terminal problems

### Output is garbled

Check the remote locale and terminal state:

```bash
printf 'TERM=%s\n' "$TERM"
locale
```

If a program left the terminal in a bad state, `reset` or `tput reset` can restore it. CJK rendering also requires a valid UTF-8 remote locale; do not force a locale that is not installed on the server.

### Terminal appears frozen

`Ctrl+S` can trigger terminal flow control in some shells; `Ctrl+Q` resumes it. Also confirm the KodeWork connection state and whether the remote shell/process is still alive before reconnecting.

### Copy or paste behaves unexpectedly

Remote content can write bounded clipboard text through the supported OSC 52 path, but remote sessions are not allowed to read arbitrary local clipboard contents. Paste remains an explicit local user action. If a terminal application has its own mouse or clipboard mode, test in a plain shell to distinguish application-specific behavior from KodeWork behavior.

## Local PowerShell, CMD, or WSL problems

Use the refresh control in the local-terminal workspace after installing or removing shells. For WSL, verify that Windows can see an initialized distribution:

```powershell
wsl.exe --list --quiet
```

If PowerShell/CMD/WSL works normally outside KodeWork but cannot be opened inside the application, include the shell type and Windows build in the bug report.

## tmux and Herdr problems

For tmux, verify the remote binary and current sessions:

```bash
command -v tmux
tmux list-sessions
```

For Herdr, verify the installed command and its normal remote runtime using the Herdr project's own instructions. KodeWork discovery cannot attach to a service or socket that is not available to the logged-in SSH user.

Do not loosen remote socket permissions broadly just to make discovery succeed. Report the ownership/permission shape with sensitive path components redacted if needed.

## Installation and update problems

### Verify an installer before running it

Download installers only from the repository's GitHub Releases page. Compare the MSI with its published SHA-256 data:

```powershell
Get-FileHash .\Kode*.msi -Algorithm SHA256
```

For releases produced under the current stable release policy, the MSI is also expected to pass Windows Authenticode verification:

```powershell
Get-AuthenticodeSignature .\Kode*.msi | Format-List Status,StatusMessage,SignerCertificate
```

If a release that is supposed to be signed reports an invalid or unexpected signature, or Windows SmartScreen presents an unexpected trust warning, **do not bypass the warning with “Run anyway.”** Stop and verify the release source and signature first.

Historical releases may have been produced under an older policy; use the release notes and current [release matrix](RELEASE-MATRIX.md) to understand what evidence applies to a particular artifact.

### MSI installation fails

Close running KodeWork instances and retry the official installer. Do not assume administrator elevation is required unless Windows Installer explicitly requests it. If installation still fails, capture the Windows Installer error and OS build; do not disable endpoint protection as a generic workaround.

### Application does not start

Check Windows Event Viewer under **Windows Logs → Application** for a relevant application error. KodeWork's Tauri shell depends on the Microsoft Edge WebView2 runtime; if WebView2 is missing or damaged, repair/install the supported WebView2 Runtime from Microsoft and retry.

### In-app update check fails

Updater verification support in the application does not by itself prove that a public update endpoint is currently available. If the in-app check fails, compare against GitHub Releases manually. Do not infer that the version in `main` is an installable update.

## Performance and stability

For high CPU or memory use, first close unused panes, stop continuous-output commands, and determine whether the behavior occurs while disconnected as well as connected. Avoid generic antivirus exclusions or other security weakening as a performance workaround.

The repository includes `scripts/measure-startup.ps1` for repeatable local startup/RSS measurements. Build a release binary first, then run from the repository root, for example:

```powershell
.\scripts\measure-startup.ps1
```

You can pass `-Executable` for another build path and `-Runs` to change the sample count. Benchmark output is local diagnostic evidence, not a release-performance guarantee.

For long-running validation, maintainers can use `scripts/run-soak-matrix.ps1`; its output belongs under the ignored local `test-results` directory and should be sanitized before sharing.

## What to include in a bug report

Use the repository bug-report form and include the KodeWork release/tag or source commit, Windows version/build, connection path, smallest reproduction steps, expected behavior, actual behavior, and sanitized logs/screenshots when useful.

Do not attach credentials, private terminal/file contents, private IP/host inventories, tailnet identifiers, or signing material. Security-sensitive reports should follow [SECURITY.md](../SECURITY.md) rather than a public issue.

For general help, see [SUPPORT.md](../SUPPORT.md). For current capability boundaries, see [STATUS.md](STATUS.md) and [RELEASE-MATRIX.md](RELEASE-MATRIX.md).
