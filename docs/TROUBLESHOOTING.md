# KodeWork Troubleshooting Guide

This guide helps you diagnose and resolve common issues with KodeWork.

## Table of Contents

- [Connection Issues](#connection-issues)
- [Authentication Problems](#authentication-problems)
- [Transfer Issues](#transfer-issues)
- [Terminal/PTY Issues](#terminal-pty-issues)
- [Performance Issues](#performance-issues)
- [Herdr/Tmux Issues](#herdrtmux-issues)
- [Installation Issues](#installation-issues)
- [Diagnostic Tools](#diagnostic-tools)

## Connection Issues

### Cannot Connect to Host

**Symptoms:** Connection fails immediately or times out.

**Diagnosis:**

1. **Check network connectivity:**
   ```powershell
   # Test basic connectivity
   ping your-host.example.com

   # Test SSH port
   Test-NetConnection -ComputerName your-host.example.com -Port 22
   ```

2. **Verify SSH service is running:**
   ```bash
   # On the remote host
   systemctl status ssh
   # or
   systemctl status sshd
   ```

3. **Check firewall rules:**
   - Windows firewall may block outbound SSH
   - Remote firewall may block inbound SSH on port 22

**Solutions:**

- **For Tailscale connections:** Ensure Tailscale is running on both machines
- **For jump host connections:** Verify jump host is reachable first
- **For custom ports:** Ensure the correct port is configured in the Address settings
- **For DNS issues:** Try using IP address directly instead of hostname

### Connection Drops Frequently

**Symptoms:** Connection established but drops within minutes.

**Diagnosis:**

1. **Check for NAT timeout:**
   - Many routers drop idle TCP connections after 5-15 minutes
   - KodeWork sends SSH keepalives, but aggressive NAT may ignore them

2. **Check network stability:**
   ```powershell
   # Monitor packet loss
   ping -t your-host.example.com
   ```

3. **Check SSH server timeout settings:**
   ```bash
   # On remote host, check /etc/ssh/sshd_config
   grep -E "ClientAliveInterval|ClientAliveCountMax" /etc/ssh/sshd_config
   ```

**Solutions:**

- **Enable SSH keepalives on server:**
  ```bash
  # Add to /etc/ssh/sshd_config
  ClientAliveInterval 30
  ClientAliveCountMax 3

  # Restart SSH service
  sudo systemctl restart sshd
  ```

- **Use Tailscale:** Direct Tailscale connections bypass NAT traversal issues

- **Check for sleep/hibernate:** Windows sleep may disconnect network adapters

### "Host Key Changed" Error

**Symptoms:** Connection blocked with host key verification failure.

**Why This Happens:**
- The remote host was reinstalled or had its SSH keys regenerated
- You're connecting to a different machine with the same hostname
- **Potential security issue:** Man-in-the-middle attack

**Solution:**

1. **Verify this is expected:**
   - Did you reinstall the remote OS?
   - Did someone regenerate SSH host keys?
   - Are you certain you're connecting to the right machine?

2. **If legitimate, clear the old key:**
   - KodeWork stores host keys per logical `HostId`
   - Delete the Host and recreate it to clear trust state
   - **Alternative:** Use SQLite browser to manually edit the `host_keys` table (advanced)

3. **If unexpected:**
   - **Do not proceed** - investigate why the key changed
   - Verify the remote host identity through another channel
   - Check for network interception or DNS hijacking

## Authentication Problems

### "Authentication Failed" with Correct Password

**Symptoms:** Password is correct but authentication fails.

**Possible Causes:**

1. **Keyboard layout mismatch:**
   - Special characters may be mistyped
   - Try typing password in a text editor first to verify

2. **Account locked or expired:**
   ```bash
   # On remote host, check user status
   sudo passwd -S username
   sudo chage -l username
   ```

3. **PAM restrictions:**
   ```bash
   # Check /var/log/auth.log on remote host
   sudo tail -f /var/log/auth.log
   ```

4. **SSH server configuration:**
   ```bash
   # Check if password auth is enabled
   grep PasswordAuthentication /etc/ssh/sshd_config
   ```

**Solutions:**

- Ensure `PasswordAuthentication yes` in `/etc/ssh/sshd_config`
- Check for typos, especially with special characters
- Verify account is not locked: `sudo passwd -u username`
- Check for fail2ban or similar security tools blocking your IP

### Private Key Authentication Fails

**Symptoms:** Public key authentication fails even with correct key.

**Diagnosis:**

1. **Verify key permissions:**
   ```bash
   # On local machine
   ls -l C:\Users\YourName\.ssh\id_rsa
   # Should be readable only by you

   # On remote machine
   ls -ld ~/.ssh ~/.ssh/authorized_keys
   # ~/.ssh should be 700, authorized_keys should be 600
   ```

2. **Check authorized_keys format:**
   ```bash
   # On remote host
   cat ~/.ssh/authorized_keys
   # Each public key should be one continuous line
   ```

3. **Verify key format:**
   - KodeWork supports OpenSSH and PEM formats
   - Newer OpenSSH keys use an `OPENSSH PRIVATE KEY` PEM header.
   - Older PEM keys use an `RSA PRIVATE KEY` PEM header.

**Solutions:**

- **Fix remote permissions:**
  ```bash
  chmod 700 ~/.ssh
  chmod 600 ~/.ssh/authorized_keys
  ```

- **Check key fingerprint matches:**
  ```powershell
  # Local machine
  ssh-keygen -lf C:\Users\YourName\.ssh\id_rsa.pub
  ```
  ```bash
  # Remote machine
  ssh-keygen -lf ~/.ssh/authorized_keys
  ```

- **Test with OpenSSH client:**
  ```powershell
  ssh -i C:\Users\YourName\.ssh\id_rsa user@host
  ```
  If this works but KodeWork doesn't, report a bug.

### "Credential Required" - Encrypted Key Passphrase

**Symptoms:** Prompted for passphrase but authentication still fails.

**Diagnosis:**

1. **Verify passphrase is correct:**
   ```powershell
   ssh-keygen -y -f C:\Users\YourName\.ssh\id_rsa
   # Enter passphrase when prompted
   # Should print the public key
   ```

2. **Check key encryption format:**
   - Old PEM format: encrypted with 3DES or AES
   - New OpenSSH format: encrypted with bcrypt-pbkdf

**Solutions:**

- Double-check passphrase (no visual feedback during typing)
- Try saving passphrase to Windows Credential Manager using the "Save to keyring" option
- Test with ssh-keygen first to confirm passphrase works

### SSH Agent/Pageant Not Working

**Symptoms:** KodeWork doesn't use keys from SSH Agent or Pageant.

**Diagnosis:**

1. **Verify agent is running:**
   ```powershell
   # For Windows SSH Agent
   Get-Service ssh-agent

   # For Pageant - check system tray
   ```

2. **Check keys are loaded:**
   ```powershell
   ssh-add -l
   ```

**Solutions:**

- **Start Windows SSH Agent:**
  ```powershell
  Start-Service ssh-agent
  Set-Service ssh-agent -StartupType Automatic
  ```

- **Add keys to agent:**
  ```powershell
  ssh-add C:\Users\YourName\.ssh\id_rsa
  ```

- **For Pageant:** Ensure Pageant is running in system tray with keys loaded

- **Select correct auth mode:** In KodeWork Host settings, choose "SSH Agent" as authentication mode

## Transfer Issues

### Transfer Fails with "Destination Busy"

**Symptoms:** Cannot start transfer to same file.

**Why:** KodeWork prevents concurrent writes to the same destination.

**Solutions:**

- Wait for the existing transfer to complete, pause, or cancel
- Use a different destination filename
- This is a safety feature to prevent corruption

### Transfer Shows "Source Changed"

**Symptoms:** Upload fails at the end with source modification error.

**Why:** File was modified during the transfer.

**Solutions:**

- Ensure source file is not being written to during transfer
- Close applications that may be modifying the file
- For large files, create a snapshot/copy first and transfer that
- This is intentional behavior to prevent uploading inconsistent data

### Resume Fails - Partial File Discarded

**Symptoms:** Transfer resume restarts from zero instead of continuing.

**Why:** Existing `.part` file doesn't match the current source file prefix.

**Solutions:**

- This is correct behavior - the partial file was invalid
- Ensure the source file hasn't changed since the previous attempt
- KodeWork verifies prefix byte-for-byte to ensure correctness

### Slow Transfer Speeds

**Symptoms:** Transfer much slower than expected.

**Diagnosis:**

1. **Check network bandwidth:**
   ```powershell
   # Test with iperf if available
   iperf3 -c remote-host
   ```

2. **Check SSH cipher overhead:**
   - Modern ciphers (chacha20-poly1305, aes256-gcm) are CPU-intensive
   - Older machines may be CPU-bound

3. **Check disk I/O:**
   - Slow source/destination drives
   - Network drives as source/destination

**Solutions:**

- Use Tailscale for better performance than jump hosts
- Check for other network activity consuming bandwidth
- Consider splitting large transfers into multiple smaller files
- Ensure local and remote disks are not bottlenecks

## Terminal/PTY Issues

### Terminal Output Garbled or Corrupted

**Symptoms:** Strange characters, broken rendering, wrong colors.

**Solutions:**

- **Reset terminal:**
  ```bash
  reset
  # or
  tput reset
  ```

- **Check TERM variable:**
  ```bash
  echo $TERM
  # Should be xterm-256color or similar
  ```

- **Clear screen:** `Ctrl+L` or `clear` command

- **Reconnect** if the issue persists

### Cannot Type in Terminal

**Symptoms:** Keypresses not appearing, terminal seems frozen.

**Diagnosis:**

1. **Check if terminal is in scroll mode:**
   - Press `Ctrl+C` to potentially break out

2. **Check for flow control:**
   - `Ctrl+S` pauses terminal output
   - `Ctrl+Q` resumes it

3. **Check connection state:**
   - Verify the connection is still "Ready" in KodeWork

**Solutions:**

- Press `Ctrl+Q` to resume if accidentally paused
- Reconnect if connection state shows "Failed" or "Reconnecting"
- Check that the remote shell hasn't exited

### Chinese/CJK Characters Don't Display

**Symptoms:** Chinese, Japanese, or Korean characters show as boxes or question marks.

**Solutions:**

- Ensure remote locale supports UTF-8:
  ```bash
  locale
  # Should show UTF-8 encoding

  # If not, add to ~/.bashrc or ~/.zshrc:
  export LC_ALL=en_US.UTF-8
  export LANG=en_US.UTF-8
  ```

- Ensure remote terminal emulator uses UTF-8:
  ```bash
  echo $LC_CTYPE
  ```

- KodeWork's xterm.js renderer supports Unicode by default

### Copy/Paste Not Working

**Symptoms:** Cannot copy from or paste to terminal.

**KodeWork Clipboard Behavior:**

- **Copy:** Select text with mouse → automatically copied to Windows clipboard
- **Paste:** Right-click or Shift+Insert
- **OSC 52:** Remote applications (tmux, vim) can write to Windows clipboard
- **Clipboard read is disabled** for security (remote cannot read local clipboard)

**Solutions:**

- Use mouse selection for copy (not Ctrl+C)
- Use right-click for paste (not Ctrl+V - that sends signal)
- For vim: `:set clipboard=unnamed` may not work due to security policy

## Performance Issues

### High CPU Usage

**Symptoms:** KodeWork consumes significant CPU even when idle.

**Possible Causes:**

1. **Excessive terminal output:**
   - Long-running command with continuous output
   - Log streaming filling buffers

2. **Many active terminal panes:**
   - Each pane consumes resources even when not visible

3. **Reconnect loops:**
   - Failed connection retrying continuously

**Solutions:**

- Close unused terminal panes
- Stop long-running output commands: `Ctrl+C`
- Check reconnect backoff is working (should pause between attempts)
- Upgrade to latest version for performance improvements

### High Memory Usage

**Symptoms:** KodeWork memory usage grows over time.

**Possible Causes:**

1. **Large terminal history buffers:**
   - Each terminal keeps bounded replay buffer
   - Many panes × large buffers = significant memory

2. **File listing caching:**
   - Large directories cached in memory

3. **Memory leak (potential bug):**
   - Report if memory grows unbounded

**Solutions:**

- Restart KodeWork periodically for long-running sessions
- Close unused terminals and file panels
- Report memory leak if reproducible

### Application Slow to Start

**Symptoms:** KodeWork takes long time to launch.

**Possible Causes:**

1. **Antivirus scanning:**
   - Real-time protection may scan executable on every launch

2. **Many stored hosts and sessions:**
   - Large SQLite database

3. **Disk I/O bottleneck:**
   - Slow HDD vs SSD

**Solutions:**

- Add KodeWork to antivirus exclusions (if trusted)
- Archive old hosts and projects
- Install on SSD if possible

## Herdr/Tmux Issues

### Cannot Find Herdr Sessions

**Symptoms:** "No Herdr sessions found" despite having active sessions.

**Diagnosis:**

1. **Verify Herdr is installed on remote host:**
   ```bash
   which herdr
   herdr --version
   ```

2. **Check socket path:**
   ```bash
   ls -la ~/.herdr/sockets/
   ```

3. **Verify Herdr daemon is running:**
   ```bash
   ps aux | grep herdrd
   ```

**Solutions:**

- Install Herdr on remote host if missing
- Start Herdr daemon: `herdrd`
- Check Herdr socket permissions

### Tmux Attach Fails

**Symptoms:** Cannot attach to tmux session.

**Diagnosis:**

1. **List tmux sessions:**
   ```bash
   tmux list-sessions
   ```

2. **Check tmux server is running:**
   ```bash
   ps aux | grep tmux
   ```

**Solutions:**

- Ensure tmux is installed: `which tmux`
- Create new session if none exist: `tmux new -s work`
- Detach from terminal before attaching in KodeWork: `Ctrl+B D`

### Session Name Conflicts

**Symptoms:** Cannot create session with desired name.

**Solution:**

- Tmux/Herdr session names must be unique
- Use different names or kill old session first:
  ```bash
  tmux kill-session -t old-session-name
  ```

## Installation Issues

### Windows SmartScreen Warning

**Symptoms:** "Windows protected your PC" warning during installation.

**Why:** Community builds are not Authenticode-signed (no commercial certificate yet).

**Solutions:**

- Click "More info" → "Run anyway" if you trust the source
- Verify download checksum matches release notes
- Download only from official GitHub Releases

### Installation Fails - Permission Denied

**Symptoms:** MSI installer fails with access denied error.

**Solutions:**

- Run installer as Administrator (right-click → "Run as administrator")
- Close any running KodeWork instances
- Check antivirus isn't blocking the installer

### Application Won't Start After Install

**Symptoms:** KodeWork installed but won't launch.

**Diagnosis:**

1. **Check Windows Event Viewer:**
   - Windows Logs → Application
   - Look for KodeWork errors

2. **Check for missing dependencies:**
   - Requires .NET/WebView2 (usually auto-installed)

**Solutions:**

- Install Microsoft Edge WebView2 Runtime manually if missing
- Reinstall with antivirus temporarily disabled
- Check system logs for specific error messages

## Diagnostic Tools

### Enable Debug Logging

**Current Status:** Structured logging hooks exist but verbose mode is not yet exposed in UI.

**Workaround:**

- Check Windows Event Viewer for application errors
- Report issues with steps to reproduce

### Collect Debug Information

When reporting issues, include:

1. **KodeWork version:**
   - Help → About (when available)
   - Or check installed program version

2. **Operating system:**
   ```powershell
   systeminfo | findstr /B /C:"OS Name" /C:"OS Version"
   ```

3. **Network environment:**
   - Direct connection, Tailscale, jump host?
   - LAN, WAN, VPN?

4. **Remote host details:**
   - Linux distribution and version
   - SSH server version: `ssh -V` on remote host
   - Any custom sshd_config settings

5. **Exact error message:**
   - Screenshot or copy full error text
   - Steps to reproduce

6. **Connection/transfer/run status:**
   - What state was it in when the issue occurred?

### SQLite Database Inspection

**Location:** `%APPDATA%\dev.kodework.windows\kodework.db`

**Tools:**
- DB Browser for SQLite
- sqlite3 command-line tool

**Warning:** Modifying the database directly can cause corruption. Back up first.

**Useful queries:**

```sql
-- List all hosts
SELECT id, label, username, port FROM hosts;

-- List all runs
SELECT id, action_name, status, started_at_ms, finished_at_ms FROM runs ORDER BY started_at_ms DESC LIMIT 20;

-- Check host keys
SELECT host_id, algorithm, fingerprint FROM host_keys;
```

### Test with OpenSSH Client

Always test problematic connections with the OpenSSH client first:

```powershell
# Basic connection test
ssh user@hostname

# With specific key
ssh -i path\to\key user@hostname

# With verbose output
ssh -vvv user@hostname

# Through jump host
ssh -J jumpuser@jumphost user@targethost
```

If OpenSSH works but KodeWork doesn't, it's a KodeWork bug - please report it.

## Still Having Issues?

1. **Search existing issues:** [GitHub Issues](https://github.com/likangmax/KodeWork/issues)
2. **Create a new issue:** Include all diagnostic information above
3. **Security issues:** Report privately through [SECURITY.md](../SECURITY.md)

**When reporting:**
- Be specific and detailed
- Include exact error messages
- Provide steps to reproduce
- Redact sensitive information (hostnames, IPs, usernames)
