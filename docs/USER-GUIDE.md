# KodeWork user guide

This guide is written for someone who has never used KodeWork, SSH, Tailscale, Herdr, or a jump host. Follow it in order. Do not copy credentials into screenshots, issues, logs, or chat.

> Package status: the downloadable desktop installer is currently for **Windows 10/11 x64**. The portable Rust core is checked on Windows, macOS, and Linux, but native macOS/Linux desktop installers are not published yet.

## 0. Five-minute first setup

For the shortest path to a working Linux host, follow these steps in order:

1. Confirm SSH works outside KodeWork: `ssh user@host`.
2. Choose **English** on the first-run language screen. You can change it later in **Settings → Language**.
3. Click **Workstations +** and fill only the name, address, port, username, and one authentication method. Do not add a jump host, fallback addresses, or embedded Tailscale on the first attempt.
4. Choose **Manual / LAN / Public** for a directly reachable host, **System Tailscale** for an existing Tailnet client, or **Embedded Tailscale** only when you intentionally want KodeWork to manage a userspace client.
5. Save and click **Connect**. If an SSH host-key fingerprint appears, save it only after verifying it belongs to the intended host.
6. The first stage is successful only when the terminal accepts input, `pwd` works, and Files opens the configured default folder.
7. Then verify clipboard copy, image/PDF paste, Herdr/tmux continuity, disconnect/reconnect, and Web Preview. Troubleshoot failures using section 9.

> A connected label is not full acceptance. Verify terminal input, file read/write access, secret boundaries, and reconnect behavior.

## 1. Before you begin

You need:

- a Windows 10/11 x64 computer;
- a Linux account on the computer you want to control;
- SSH enabled on that Linux computer;
- one usable network path to it; and
- one supported SSH authentication method.

Herdr and tmux are optional. Install one on the Linux host if remote work must survive a Windows restart, network change, or KodeWork exit.

### Choose the connection mode

| Your situation | Choose in KodeWork | What must already work |
| --- | --- | --- |
| The Linux host has a reachable IP or DNS name | LAN, Public, or Manual address | Windows can reach the host and SSH port |
| Both computers already use the same Tailscale network | System service | Tailscale is installed and signed in on Windows |
| You do not want to manage a separate Windows Tailscale client | Embedded userspace | A least-privilege Tailscale auth key is available for first registration |
| Only a bastion/VPS can reach the Linux host | Jump host | Windows can SSH to the bastion, and the bastion can reach the final host |

Start with the simplest working path. Add fallback addresses only after the first path connects successfully.

### Choose the authentication method

| Server setup | Authentication choice | What you provide |
| --- | --- | --- |
| Password login is enabled | Password | The Linux account password when prompted |
| You have an OpenSSH private key file | Private key | The local key path and, if encrypted, its passphrase |
| The key is loaded into Windows OpenSSH Agent/Pageant | SSH Agent | No private-key material in KodeWork |
| The server uses prompts, OTP, or MFA | Keyboard-interactive | Answers to the prompts displayed during connection |

## 2. Install, upgrade, and choose a language

1. Open the latest GitHub Release.
2. Download the Windows x64 MSI listed in the release assets.
3. Compare its SHA-256 with the value in the release notes when one is provided.
4. Run the MSI. Community builds may show an unknown-publisher warning until the project has an Authenticode certificate.
5. Launch KodeWork and choose **简体中文** or **English** at the first-launch prompt.
6. To change the language later, open **Settings / 设置**, select a language, then close the panel.

The language prompt belongs to KodeWork. The current MSI wizard itself is not localized.

Installing a newer MSI with the same application identity upgrades the existing installation and keeps local application data. Do not uninstall first unless the release notes explicitly require it.

## 3. Prepare the Linux host

From a trusted terminal, first prove that ordinary SSH works:

```bash
ssh your-user@your-host
```

If that fails, fix the server, network, firewall, username, port, or authentication before debugging KodeWork.

Optional durable runtimes:

```bash
command -v tmux && tmux -V
command -v herdr && herdr --version
```

KodeWork checks command lookup and common login-shell paths for Herdr. If Herdr exists only in a private directory, add that directory to the PATH used by non-interactive/login SSH shells. A command that works only in one interactive shell may still be invisible to a desktop SSH client.

Choose a writable project directory, for example `/home/alice/project`. KodeWork uses the configured default remote folder for the Files page and as the default destination for pasted images/PDFs.

## 4. Add or edit a workstation

Select **+** beside **Workstations / 工作站**. To change an existing workstation later, select it and choose **Edit / 编辑**.

### Field reference

| Field | What to enter | Rules and examples |
| --- | --- | --- |
| Name | A label only you see | Example: `GPU server` |
| Address | Hostname or IP of the final Linux host | Example: `server.example.net`, a private IP, or a Tailscale/MagicDNS name |
| Port | SSH port | Usually `22`; valid range is 1-65535 |
| Username | Linux login account | Example: `alice`; this is not a Windows username |
| Runtime | tmux, Herdr, or Plain shell | Choose Herdr/tmux only when it is installed and visible to the SSH login environment |
| Address type | LAN, Tailscale, Public, Jump host, or Manual | This describes the route; it does not change SSH authentication |
| Default remote folder | Initial Files/paste destination | Must be an absolute Linux path beginning with `/` |
| Authentication | Password, Private key, SSH Agent, or Keyboard-interactive | Match the SSH server policy |
| Private key file | Local OpenSSH private-key path | Never select the public `.pub` file |
| Private key passphrase | Unlocks an encrypted key | Leave blank to keep the already saved passphrase |
| Jump host | Bastion hostname, port, username | Leave disabled unless the final host is reachable only through it |
| Tailscale mode | System service or Embedded userspace | Tailscale supplies a network path; SSH still authenticates the Linux account |
| Device name | Optional embedded Tailscale device label | Use a recognizable, non-secret name |
| Private state file | Optional embedded Tailscale state path | Leave blank unless you deliberately manage its location |
| Auth key | Tailscale registration credential | Needed only for embedded registration; never commit or publish it |

Save the workstation only after checking the address, username, port, and default folder. A wrong default folder does not normally prevent SSH, but Files and asset paste will fail until it is corrected.

### Direct address

Choose LAN, Public, or Manual. Enter the final Linux hostname/IP and SSH port. Use Public only when you intentionally expose SSH and have secured it appropriately.

### System Tailscale

Choose **System service** when Tailscale is installed and signed in on Windows. KodeWork reads peer information but does not change the system Tailscale login. Confirm that the desired Linux peer appears in the Tailscale client before connecting.

### Embedded Tailscale

Choose **Embedded userspace** when KodeWork should run a private bundled Tailscale instance.

1. Create a short-lived or reusable auth key with only the tags and lifetime required.
2. Paste it only into KodeWork's masked Auth Key field.
3. Save the workstation. The key is stored behind the native secret boundary and is not displayed again.
4. Connect and wait for registration plus SSH. Bundled Tailscale processes must remain hidden; flashing command windows are a defect.
5. Revoke the key immediately if it was ever pasted into source code, chat, a screenshot, an issue, or a log.

### Jump host

Enable the jump-host section and enter the bastion hostname, port, and username. Keep the main Address field set to the final Linux destination. The bastion must be able to resolve and reach that final address. Test both hops independently before expecting the chain to work.

## 5. Make the first connection

1. Select the workstation.
2. Choose **Connect / 连接**.
3. On first contact, review the SSH key algorithm and SHA-256 fingerprint.
4. Compare the fingerprint with a value obtained from the server administrator or another trusted channel.
5. Trust and save only when it matches.
6. Supply the requested password, key passphrase, or interactive answers.
7. Wait for the state to become **Connected / 已连接** and for a shell prompt to appear.

A later host-key change is a hard failure. Do not bypass it until the server replacement or key rotation has been independently confirmed.

If every address candidate fails, test in this order: hostname resolution, TCP/SSH port, Tailscale peer state, local/server firewall, username/authentication, then jump-host reachability.

## 6. Confirm the configuration really works

Do not call a workstation “configured” until all applicable checks pass:

- the selected workstation shows Connected;
- **New terminal / 新建终端** opens an independent shell and displays a prompt;
- typing `pwd` and pressing Enter produces output without noticeable input delay;
- the Files page opens the configured default remote folder;
- selecting terminal text places it in the Windows clipboard, and a paste into Notepad returns the same text;
- **Paste image/PDF / 粘贴图片/PDF** uploads a clipboard image or copied PDF to a writable remote folder and inserts the remote path into the active terminal;
- Herdr or tmux is detected when the selected runtime is actually installed and available to the SSH login environment;
- a Web Preview opens only after a loopback SSH tunnel has been created for the correct remote port;
- Disconnect followed by Connect returns to a usable terminal; and
- no credential, private key, auth key, or real infrastructure value appears in logs or screenshots.

Mark non-applicable checks explicitly instead of silently skipping them.

## 7. Use the workspace

- **New terminal** opens another remote PTY. KodeWork limits concurrent panes to protect responsiveness.
- **Split right / Split below** changes the layout; each pane has its own close action.
- **Select text** copies terminal selection to the Windows clipboard. Clipboard access can be blocked by OS/WebView policy; test with Notepad when diagnosing it.
- **Paste image/PDF** reads only after an explicit click, streams the asset through SFTP, and inserts the remote path. The destination must exist and be writable.
- **Voice** captures input only after an explicit action and inserts recognized text into the active terminal.
- **Files** browses and transfers through SFTP. Pin the workstation's default remote folder by editing the workstation.
- **Preview** forwards a remote development port to loopback. The remote application must listen on the selected port; its own CSP may still restrict embedding.
- **Activity** shows Action/Run information and errors.
- **Local** opens PowerShell, Command Prompt, or an installed WSL distribution through Windows ConPTY. These local sessions do not use the remote host.
- **Focus** gives terminal output the maximum workspace area; leave focus mode to reach normal navigation again.

## 8. Keep remote work alive

For work that must continue while Windows sleeps, the network disconnects, or KodeWork exits, run it inside Herdr or tmux on the Linux host. A tray icon, autostart, or a reconnect loop is not a durability guarantee for a local Windows process.

Typical tmux fallback:

```bash
tmux new -As kodework
```

Reconnect later and attach to the same runtime/session. If KodeWork reports that Herdr is missing while it exists, compare `command -v herdr` in an SSH login shell and ensure its directory is exported before non-interactive commands run.

## 9. Common failures

### Connection timed out

- Confirm the hostname/IP and port.
- Test the same route with a trusted SSH client.
- For Tailscale, confirm both devices are online and ACLs allow the connection.
- For a jump host, prove Windows-to-bastion and bastion-to-target separately.

### Embedded Tailscale does not start

- Confirm the installation contains both bundled Tailscale components.
- Reopen the workstation and check that Embedded userspace is selected.
- Re-enter a valid, unexpired auth key only if registration is required.
- Do not repeatedly retry if command windows flash; capture sanitized diagnostics and stop.

### Herdr is not detected

- Run `command -v herdr` and `herdr --version` through the same SSH account.
- Check login/non-interactive PATH setup.
- Use tmux or Plain shell until the environment is corrected.

### Clipboard selection says copied, but Windows has nothing

- Click inside the terminal, select a short plain-text line, then paste into Notepad.
- Check Windows clipboard policy and whether another clipboard manager is interfering.
- Reconnect once to replace a stale terminal subscription.
- Report the app version and sanitized reproduction steps; never include terminal secrets.

### Image/PDF paste is unavailable or fails

- Select an active remote terminal first.
- Copy an actual image or PDF file into the Windows clipboard.
- Confirm the default remote folder exists and is writable.
- Confirm SFTP works by opening Files.

### The final terminal row is clipped

- Leave and re-enter Focus mode, then resize the window once.
- Confirm Windows display scaling and app version.
- If reproducible, report the scaling percentage, window size, and a screenshot with infrastructure details redacted.

### File transfer is slow

- Compare transfer speed over the same network route outside KodeWork.
- Avoid unnecessary jump-host or public fallback routes.
- Check latency, packet loss, server disk speed, and server CPU.
- Keep the app open for local transfer continuity; remote commands remain durable only through Herdr/tmux.

## 10. Safe diagnostics and recovery

Before sharing diagnostics, remove passwords, auth keys, private-key paths, real hostnames/IPs, usernames, terminal content, and private filenames. Prefer a short sequence of actions plus the visible error text.

If configuration becomes confusing:

1. Edit the workstation and disable every fallback except one known route.
2. Set Runtime to Plain shell.
3. Test Password or one known key method.
4. Connect and complete the verification checklist.
5. Add Tailscale, jump hosts, Herdr/tmux, and fallback routes back one at a time.

This isolates the failing layer instead of changing several variables at once.
