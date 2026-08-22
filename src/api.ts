// Typed Tauri IPC surface for Kodework. The renderer never touches secrets:
// passwords are passed once per connect and never persisted in React state
// beyond the in-flight dialog.

import { Channel, invoke } from '@tauri-apps/api/core'

export type AddressKind = 'Lan' | 'Tailscale' | 'Public' | 'JumpHost' | 'Manual'
export type RuntimeKind = 'Tmux' | 'Herdr' | 'PlainShell'
export type AuthenticationMode = 'Password' | 'PublicKey' | 'SshAgent' | 'KeyboardInteractive'
export type ConnectionState =
  | 'Disconnected' | 'ResolvingAddress' | 'Connecting' | 'VerifyingHostKey'
  | 'Authenticating' | 'WaitingForCredential' | 'Ready' | 'Reconnecting' | 'Failed'

export type HostAddress = {
  id: string
  kind: AddressKind
  hostname_or_ip: string
  port: number
  priority: number
  enabled: boolean
}

export type TailscaleConfig = {
  enabled: boolean
  mode: 'Disabled' | 'SystemDaemon' | 'EmbeddedUserspace'
  device_name: string | null
  auth_key_ref: { provider: string; opaque_id: string } | null
  state_dir: string | null
}

export type Host = {
  id: string
  label: string
  username: string
  port: number
  auth_ref: { provider: string; opaque_id: string } | null
  auth_mode: AuthenticationMode
  private_key_path: string | null
  default_remote_path: string
  jump: {
    hostname: string
    port: number
    username: string
    auth_ref?: { provider: string; opaque_id: string } | null
    auth_mode?: AuthenticationMode
    private_key_path?: string | null
  } | null
  addresses: HostAddress[]
  tailscale: TailscaleConfig | null
  default_runtime: RuntimeKind
}

export type HostKeyRequest = {
  request_id: number
  info: {
    hostname: string
    port: number
    algorithm: string
    fingerprint: string
    key_blob_base64: string
  }
}
export type KeyboardInteractiveRequest = {
  request_id: number
  name: string
  instructions: string
  prompts: { prompt: string; echo: boolean }[]
}

export type SessionEvent =
  | { Data: { channel: number; bytes: number[] } }
  | { ExtendedData: { channel: number; ext: number; bytes: number[] } }
  | { ExitStatus: { channel: number; status: number } }
  | { ExitSignal: { channel: number; signal: string } }
  | { ChannelClosed: { channel: number } }
  | { AuthBanner: string }
  | { Disconnected: { description: string } }
  | { Error: { description: string } }

export type LocalTerminalKind = 'power_shell' | 'command_prompt' | 'wsl'
export type LocalTerminalDescriptor = { id: number; kind: LocalTerminalKind; label: string }
export type LocalTerminalCapabilities = {
  powershell: boolean
  command_prompt: boolean
  wsl: boolean
  wsl_distributions: string[]
}
export type LocalTerminalEvent =
  | { kind: 'data'; bytes: number[] }
  | { kind: 'exited'; code: number }
  | { kind: 'error'; message: string }

export const localTerminalCapabilities = () => invoke<LocalTerminalCapabilities>('local_terminal_capabilities')
export const localTerminalOpen = (kind: LocalTerminalKind, distribution?: string) =>
  invoke<LocalTerminalDescriptor>('local_terminal_open', { kind, distribution: distribution ?? null, cols: 80, rows: 24 })
export const localTerminalWrite = (id: number, data: Uint8Array) =>
  invoke<void>('local_terminal_write', { id, data: Array.from(data) })
export const localTerminalResize = (id: number, cols: number, rows: number) =>
  invoke<void>('local_terminal_resize', { id, cols, rows })
export const localTerminalClose = (id: number) => invoke<void>('local_terminal_close', { id })
export const subscribeLocalTerminal = (id: number, onEvent: (event: LocalTerminalEvent) => void): (() => void) => {
  if (!isDesktop()) return () => {}
  const channel = new Channel<LocalTerminalEvent>()
  channel.onmessage = onEvent
  void invoke<void>('local_terminal_subscribe', { id, onEvent: channel }).catch(() => {})
  return () => {
    const raw = channel as unknown as { cleanupCallback?: () => void }
    raw.cleanupCallback?.()
  }
}

export const isDesktop = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export const listHosts = () => invoke<Host[]>('list_hosts')
export const saveHost = (host: Host) => invoke<void>('save_host', { host })
export const saveHostPassword = (host: Host, password: string) =>
  invoke<Host>('save_host_password', { host, password })
export const deleteHost = (hostId: string) => invoke<boolean>('delete_host', { hostId })
export const connectHost = (host: Host, password?: string) =>
  invoke<string>('connect_host', { host, password: password ?? null })
export const reconnectHost = (hostId: string) =>
  invoke<string>('reconnect_host', { hostId })
export const prepareHostNetwork = (hostId: string) =>
  invoke<void>('prepare_host_network', { hostId })
export const tailscaleStatus = (hostId?: string) =>
  invoke<unknown>('tailscale_status', { hostId: hostId ?? null })
export type TailscaleRuntimeInfo = {
  cli_available: boolean
  daemon_available: boolean
  bundled: boolean
  bundled_version: string
}
export const tailscaleRuntimeInfo = () => invoke<TailscaleRuntimeInfo>('tailscale_runtime_info')
export const saveTailscaleAuthKey = (hostId: string, authKey: string) =>
  invoke<Host>('save_tailscale_auth_key', { hostId, authKey })
export const disconnectHost = (hostId: string) => invoke<void>('disconnect_host', { hostId })
export const openPane = (hostId: string, cols: number, rows: number) =>
  invoke<[number, number]>('open_pane', { hostId, cols, rows })
export const closePane = (hostId: string, paneId: number) =>
  invoke<void>('close_pane', { hostId, paneId })
export const sendInput = (hostId: string, paneId: number, data: Uint8Array) =>
  invoke<void>('send_input', { hostId, paneId, data: Array.from(data) })
export const resizePty = (hostId: string, paneId: number, cols: number, rows: number) =>
  invoke<void>('resize_pty', { hostId, paneId, cols, rows })
export const sessionState = (hostId: string) => invoke<ConnectionState>('session_state', { hostId })
export const pendingHostKeyRequests = () => invoke<HostKeyRequest[]>('pending_host_key_requests')
export const answerHostKey = (requestId: number, decision: 'trust_once' | 'trust_and_save' | 'reject') =>
  invoke<boolean>('answer_host_key', { requestId, decision })
export const pendingKeyboardInteractiveRequests = () => invoke<KeyboardInteractiveRequest[]>('pending_keyboard_interactive_requests')
export const answerKeyboardInteractive = (requestId: number, responses: string[]) =>
  invoke<boolean>('answer_keyboard_interactive', { requestId, responses })

export const subscribeSession = (hostId: string, channelFilter: number | null, onEvent: (event: SessionEvent) => void): (() => void) => {
  if (!isDesktop()) return () => {}
  const channel = new Channel<SessionEvent>()
  channel.onmessage = onEvent
  void invoke<void>('session_subscribe', { hostId, channel: channelFilter, onEvent: channel }).catch(() => {})
  // Tauri's Channel has no public close API. cleanupCallback unregisters
  // the global callback so the Rust pump's next send fails, the pump task
  // exits, and the backend subscriber is reaped. Without this, every
  // re-subscribe (pane switch, reconnect, tab change) leaks a subscriber
  // and its event callback for the whole session lifetime.
  return () => {
    const raw = channel as unknown as { cleanupCallback?: () => void }
    raw.cleanupCallback?.()
  }
}

export type TmuxSession = {
  name: string
  windows: number
  attached: number
  created: string
}

export type HerdrAgentInfo = {
  name: string | null
  kind: string | null
  status: string | null
  workspace_id: string | null
  pane_id: string | null
}

export type HerdrAgentStatus =
  | 'Unknown' | 'Idle' | 'Working' | 'Blocked' | 'Done'

export const tmuxList = (hostId: string) => invoke<TmuxSession[]>("tmux_list", { hostId })
export const tmuxNew = (hostId: string, name: string) => invoke<void>("tmux_new", { hostId, name })
export const tmuxKill = (hostId: string, name: string) => invoke<void>("tmux_kill", { hostId, name })
export const herdrDetect = (hostId: string) => invoke<string>("herdr_detect", { hostId })
export const herdrAgents = (hostId: string) => invoke<HerdrAgentInfo[]>("herdr_agents", { hostId })
export const herdrAttach = (hostId: string) => invoke<void>("herdr_attach", { hostId })

export const agentStatusLabel = (info: HerdrAgentInfo): HerdrAgentStatus => {
  const raw = info.status ?? ""
  if (raw === 'idle') return 'Idle'
  if (raw === 'working') return 'Working'
  if (raw === 'blocked') return 'Blocked'
  if (raw === 'done') return 'Done'
  return 'Unknown'
}

export type RemoteFileMeta = {
  name: string
  size: number
  is_dir: boolean
  modified_ms: number | null
}

export type TransferEvent =
  | { Progress: { id: string; progress: { transferred: number; total: number | null; speed_bps: number } } }
  | { State: { id: string; status: string } }
  | { Failed: { id: string; message: string } }

export const sftpList = (hostId: string, path: string) =>
  invoke<RemoteFileMeta[]>('sftp_list', { hostId, path })
export const sftpUpload = (hostId: string, localPath: string, remotePath: string) =>
  invoke<string>('sftp_upload', { hostId, localPath, remotePath, resume: true })
export type ClipboardPasteResult =
  | { kind: 'text'; text: string }
  | { kind: 'assets'; remote_paths: string[] }
  | { kind: 'empty' }
export const clipboardPaste = (hostId: string) =>
  invoke<ClipboardPasteResult>('clipboard_paste', { hostId })
export const clipboardCopyText = (text: string) =>
  invoke<void>('clipboard_copy_text', { text })
export const sftpDownload = (hostId: string, remotePath: string, localPath: string) =>
  invoke<string>('sftp_download', { hostId, remotePath, localPath, resume: true })
export const sftpPause = (hostId: string, transferId: string) =>
  invoke<void>('sftp_pause', { hostId, transferId })
export const sftpResume = (hostId: string, transferId: string) =>
  invoke<void>('sftp_resume', { hostId, transferId })
export const sftpCancel = (hostId: string, transferId: string) =>
  invoke<void>('sftp_cancel', { hostId, transferId })

export const subscribeSftp = (hostId: string, onEvent: (event: TransferEvent) => void): (() => void) => {
  if (!isDesktop()) return () => {}
  const channel = new Channel<TransferEvent>()
  channel.onmessage = onEvent
  void invoke<void>('sftp_subscribe', { hostId, onEvent: channel }).catch(() => {})
  return () => {
    const raw = channel as unknown as { cleanupCallback?: () => void }
    raw.cleanupCallback?.()
  }
}
export const setAutostart = (enabled: boolean) => invoke<boolean>('set_autostart', { enabled })
export const autostartStatus = () => invoke<boolean>('autostart_status')
export type TunnelState = "Creating" | "Listening" | "Closed" | "Failed"

export type TunnelInfo = {
  id: string
  host_id: string
  local_addr: string
  remote_host: string
  remote_port: number
  state: TunnelState
  active_connections: number
  error: string | null
}

export const tunnelOpen = (hostId: string, localPort: number, remoteHost: string, remotePort: number) =>
  invoke<TunnelInfo>("tunnel_open", { hostId, localPort, remoteHost, remotePort })
export const tunnelClose = (tunnelId: string) => invoke<void>("tunnel_close", { tunnelId })
export const tunnelList = () => invoke<TunnelInfo[]>("tunnel_list")
export type HerdrBridgeInfo = {
  tunnel: TunnelInfo
  remote_socket: string
  remote_port: number
  remote_pid: number
}

export const herdrBridge = (hostId: string, localPort: number) =>
  invoke<HerdrBridgeInfo>("herdr_bridge", { hostId, localPort })
export const herdrBridgeStop = (hostId: string, remotePort: number, remotePid?: number) =>
  invoke<void>("herdr_bridge_stop", { hostId, remotePort, remotePid: remotePid ?? null })
export type Snippet = {
  id: string
  name: string
  text: string
  sort_order: number
}

export const snippetList = () => invoke<Snippet[]>('snippet_list')
export const snippetSave = (snippet: Snippet) => invoke<void>('snippet_save', { snippet })
export const snippetDelete = (snippetId: string) => invoke<boolean>('snippet_delete', { snippetId })
export const yaziAttach = (hostId: string) => invoke<void>('yazi_attach', { hostId })
export type Project = {
  id: string
  host_id: string
  name: string
  remote_cwd: string
  preferred_runtime: "Tmux" | "Herdr" | "PlainShell"
}

export type Action = {
  id: string
  project_id: string
  name: string
  command: string
  mode: "Interactive" | "Quick" | "Background"
  cwd: string | null
  timeout_ms: number | null
  danger_level: "Safe" | "Review" | "Dangerous"
  confirmation: "Never" | "OnDangerous" | "Always"
  env: Record<string, string>
}

export type RunOutcome = {
  disposition: 'Completed' | 'BackgroundStarted' | 'InteractiveDispatched'
  exit_code: number | null
  stdout_preview: string
  stderr_preview: string
  output_bytes: number
  remote_session_ref: string | null
}

export type RunStatus = 'Created' | 'Confirming' | 'Queued' | 'Running' | 'Succeeded' | 'Failed' | 'Cancelled' | 'TimedOut' | 'Interrupted' | 'Unknown'
export type Run = {
  id: string
  action_id: string | null
  host_id: string
  project_id: string | null
  action_name: string
  command_snapshot: string
  mode: 'Interactive' | 'Quick' | 'Background'
  cwd_snapshot: string | null
  status: RunStatus
  started_at_ms: number | null
  finished_at_ms: number | null
  exit_code: number | null
  remote_session_ref: string | null
  stdout_preview: string
  stderr_preview: string
  output_bytes: number
  last_reconciled_at_ms: number | null
}

export const projectList = (hostId: string) => invoke<Project[]>('project_list', { hostId })
export const projectSave = (project: Project) => invoke<void>('project_save', { project })
export const projectDelete = (projectId: string) => invoke<boolean>('project_delete', { projectId })
export const actionList = (projectId: string) => invoke<Action[]>('action_list', { projectId })
export const actionSave = (action: Action) => invoke<void>('action_save', { action })
export const actionDelete = (actionId: string) => invoke<boolean>('action_delete', { actionId })
export const runAction = (hostId: string, action: Action, confirmed: boolean) =>
  invoke<RunOutcome>('run_action', { hostId, action, confirmed })
export const runList = (actionId?: string, limit = 50, hostId?: string) =>
  invoke<Run[]>('run_list', { actionId: actionId ?? null, hostId: hostId ?? null, limit })
export const runReconcile = (hostId: string) => invoke<number>('run_reconcile', { hostId })

// --- Updater (tauri-plugin-updater) ---
export type UpdateCheck =
  | { status: 'unsupported' }
  | { status: 'up-to-date' }
  | { status: 'available'; version: string }
  | { status: 'error'; error: string }

export const checkForUpdates = async (): Promise<UpdateCheck> => {
  if (!isDesktop()) return { status: 'unsupported' }
  try {
    const { check } = await import('@tauri-apps/plugin-updater')
    const update = await check()
    if (!update) return { status: 'up-to-date' }
    return { status: 'available', version: update.version }
  } catch (error) {
    return { status: 'error', error: String(error) }
  }
}

export const installUpdate = async (): Promise<{ ok: boolean; error?: string }> => {
  if (!isDesktop()) return { ok: false, error: '仅桌面版支持自动更新' }
  try {
    const { check } = await import('@tauri-apps/plugin-updater')
    const update = await check()
    if (!update) return { ok: true }
    await update.downloadAndInstall()
    return { ok: true }
  } catch (error) {
    return { ok: false, error: String(error) }
  }
}
