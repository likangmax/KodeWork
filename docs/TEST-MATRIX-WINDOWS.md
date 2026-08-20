# Windows sustained reliability matrix

## Automated gates

| Scenario | Automation | Pass condition |
|---|---|---|
| 20 concurrent PTYs | `run-soak-matrix.ps1` + fake SSH | all 20 panes open, accept input, 21st is rejected by the explicit bound |
| network interruption | core generation/reconnect tests | stale generation cannot overwrite the new session; panes are recreated |
| transfer fault handling | SFTP pause/resume/retry tests | byte-exact result, `.part` retained only when resumable, no destination exposure before rename |
| stale partial with same size | SFTP prefix-integrity regression tests | mismatched upload/download prefix restarts from zero and finishes byte-exact |
| detached Run reconciliation | core/storage tests plus remote probe | tmux launch remains Running; exit marker maps to Succeeded/Failed; missing evidence maps to Unknown |
| logical HostId host-key identity | SSH broker/storage tests | the same key passes across fallback addresses; a changed key is a hard failure |
| Herdr bridge ownership | core fake SSH bridge tests | exact PID is captured, readiness is checked, stop cannot pattern-kill unrelated socat |
| large streaming transfer | optional 512 MiB release test | upload/download complete byte-exact without file-size-proportional memory growth |
| multi-hour stability | repeated SSH/SFTP integration loop | every iteration passes and process memory/handle counts do not trend upward |

Run a four-hour preflight:

```powershell
.\scripts\run-soak-matrix.ps1 -Hours 4 -IncludeLargeTransfer
```

`-AllowSleepCycle` is deliberately opt-in because it suspends Windows. The application additionally probes session state immediately on WebView focus, visibility restoration, or network-online notification; its normal three-second transport poll remains the fallback.

## Physical Windows acceptance

Record OS build, network path (LAN/Tailscale DERP/direct), remote OpenSSH version, Herdr version, memory/handle baselines, and timestamps. Execute:

1. Keep 20 panes producing distinct tagged output for 30 minutes; verify no cross-pane bytes and bounded scrollback.
2. Suspend for 10 minutes, resume, then verify state converges to Ready or gives one actionable credential prompt.
3. Disable all adapters for 30 minutes, restore them, and verify bounded reconnect attempts without CMD-window storms.
4. Transfer a file larger than available RAM, interrupt the network twice, resume, and compare SHA-256 hashes.
5. Run an eight-hour mixed workload with terminal flood, SFTP, runtime polling, and Web Preview; compare RSS/private bytes/handles at start, 1h, 4h, and 8h.

Public release is blocked until this matrix has a dated result log from the intended distribution hardware. CI simulations are necessary but do not replace sleep, driver, VPN, SmartScreen, and certificate-chain tests on real Windows.
