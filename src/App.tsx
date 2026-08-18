// Kodework main shell: host rail, connect flow with host-key
// confirmation, xterm terminal pane, status bar. No fabricated success.

import { useCallback, useEffect, useRef, useState } from 'react'
import './App.css'
import type { Action, HerdrAgentInfo, Host, HostKeyRequest, KeyboardInteractiveRequest, Project, RemoteFileMeta, Run, RunOutcome, Snippet, TmuxSession, TunnelInfo, UpdateCheck } from './api'
import {
  answerHostKey, answerKeyboardInteractive, connectHost, deleteHost, isDesktop,
  listHosts, pendingHostKeyRequests, pendingKeyboardInteractiveRequests, prepareHostNetwork, saveHost, saveHostPassword, saveTailscaleAuthKey, sessionState, disconnectHost, tailscaleRuntimeInfo,
  actionDelete, actionList, actionSave,
  closePane, herdrAgents, herdrAttach, herdrBridge, herdrBridgeStop, herdrDetect, openPane,
  projectDelete, projectList, projectSave, runAction, runList, sendInput,
  autostartStatus, checkForUpdates, installUpdate, sftpCancel, sftpDownload, sftpList, sftpPause, sftpResume, sftpUpload, setAutostart, subscribeSftp,
  snippetDelete, snippetList, snippetSave,
  tmuxKill, tmuxList, tmuxNew,
  yaziAttach,
  tunnelClose, tunnelList, tunnelOpen,
} from './api'
import { Icon } from './icons'
import { FilesPanel } from './files/FilesPanel'
import { LocalTerminalWorkspace } from './terminal/LocalTerminalWorkspace'
import { TerminalWorkspace } from './terminal/TerminalWorkspace'
import { RuntimePanel } from './runtime/RuntimePanel'
import { useResumeRecovery } from './runtime/useResumeRecovery'
import { SettingsPanel } from './settings/SettingsPanel'
import { HostEditor } from './settings/HostEditor'
import { useTheme } from './settings/useTheme'
import { WorkspaceHeader } from './workspace/WorkspaceHeader'

const inputEncoder = new TextEncoder()

const newHost = (): Host => ({
  id: crypto.randomUUID(),
  label: '新建工作站',
  username: '',
  port: 22,
  auth_ref: null,
  auth_mode: 'Password',
  private_key_path: null,
  default_remote_path: '/',
  jump: null,
  addresses: [{ id: crypto.randomUUID(), kind: 'Tailscale', hostname_or_ip: '', port: 22, priority: 10, enabled: true }],
  tailscale: { enabled: true, mode: 'EmbeddedUserspace', device_name: null, auth_key_ref: null, state_dir: null },
  default_runtime: 'Tmux',
})

const firstAddress = (host: Host) => host.addresses.find((a) => a.enabled) ?? host.addresses[0]

const sameTmuxSessions = (a: TmuxSession[], b: TmuxSession[]) =>
  a.length === b.length && a.every((item, index) => {
    const other = b[index]
    return item.name === other.name && item.windows === other.windows && item.attached === other.attached && item.created === other.created
  })

const sameAgents = (a: HerdrAgentInfo[], b: HerdrAgentInfo[]) =>
  a.length === b.length && a.every((item, index) => {
    const other = b[index]
    return item.name === other.name && item.kind === other.kind && item.status === other.status && item.workspace_id === other.workspace_id && item.pane_id === other.pane_id
  })

type ConnectPhase = 'idle' | 'connecting' | 'ready' | 'failed'

export default function App() {
  const [hosts, setHosts] = useState<Host[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [draft, setDraft] = useState<Host | null>(null)
  const [message, setMessageState] = useState('正在读取本地配置…')
  const [messageIsError, setMessageIsError] = useState(false)
  const messageTimer = useRef<number | null>(null)
  // Enhanced message: errors are highlighted and any message expires so a
  // later poll/success cannot permanently overwrite an important error.
  const setMessage = useCallback((text: string) => {
    const isError = /失败|错误|无法|拒绝|异常|已拒绝|fail|error|denied|refused|timeout|invalid/i.test(text)
    setMessageState(text)
    setMessageIsError(isError)
    if (messageTimer.current !== null) window.clearTimeout(messageTimer.current)
    messageTimer.current = window.setTimeout(() => {
      setMessageState((current) => (current === text ? '' : current))
      setMessageIsError(false)
    }, isError ? 30000 : 10000)
  }, [])
  const [phase, setPhase] = useState<ConnectPhase>('idle')
  const [stateLabel, setStateLabel] = useState('未连接')
  // Keep credentials out of React state/devtools snapshots. The value lives
  // only in the native input element until submit and is cleared immediately.
  const passwordInputRef = useRef<HTMLInputElement | null>(null)
  const privateKeyPassphraseRef = useRef<HTMLInputElement | null>(null)
  const tailscaleKeyInputRef = useRef<HTMLInputElement | null>(null)
  const [rememberPassword, setRememberPassword] = useState(false)
  const [promptPassword, setPromptPassword] = useState(false)
  const [hostKeyRequest, setHostKeyRequest] = useState<HostKeyRequest | null>(null)
  const [keyboardInteractiveRequest, setKeyboardInteractiveRequest] = useState<KeyboardInteractiveRequest | null>(null)
  const keyboardInteractiveInputs = useRef<HTMLInputElement[]>([])
  const [herdrVersion, setHerdrVersion] = useState<string | null>(null)
  const [herdrMissing, setHerdrMissing] = useState(false)
  const [herdrError, setHerdrError] = useState<string | null>(null)
  const [agents, setAgents] = useState<HerdrAgentInfo[]>([])
  const [tmuxSessions, setTmuxSessions] = useState<TmuxSession[]>([])
  const [newTmuxName, setNewTmuxName] = useState('')
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [autoStart, setAutoStart] = useState(false)
  const [updateCheck, setUpdateCheck] = useState<UpdateCheck | null>(null)
  const [updateBusy, setUpdateBusy] = useState(false)
  const [theme, onThemeChange] = useTheme()
  const [activeTab, setActiveTab] = useState<'terminal' | 'local' | 'files' | 'preview' | 'actions'>('terminal')
  const [panes, setPanes] = useState<{ id: number; channel: number }[]>([])
  const [splitDir, setSplitDir] = useState<'h' | 'v'>('h')
  const [runtimeOpen, setRuntimeOpen] = useState(false)
  const [tailscaleComponents, setTailscaleComponents] = useState<{ cli_available: boolean; daemon_available: boolean; bundled: boolean; bundled_version: string } | null>(null)
  const [terminalFocusMode, setTerminalFocusMode] = useState(false)
  const [filesPath, setFilesPath] = useState('/')
  const [files, setFiles] = useState<RemoteFileMeta[]>([])
  const [filesLoading, setFilesLoading] = useState(false)
  const [selectedRemote, setSelectedRemote] = useState<string | null>(null)
  const [transfers, setTransfers] = useState<Record<string, { status: string; transferred: number; total: number | null; speed: number; message?: string }>>({})
  const [tunnels, setTunnels] = useState<TunnelInfo[]>([])
  const [tunnelPanelOpen, setTunnelPanelOpen] = useState(false)
  const [tunnelLocal, setTunnelLocal] = useState('')
  const [tunnelRemoteHost, setTunnelRemoteHost] = useState('127.0.0.1')
  const [tunnelRemotePort, setTunnelRemotePort] = useState('3000')
  const [previewUrl, setPreviewUrl] = useState<string | null>(null)
  const [bridgeInfo, setBridgeInfo] = useState<{ local: string; socket: string; tunnelId: string; remotePort: number } | null>(null)
  const [snippets, setSnippets] = useState<Snippet[]>([])
  const [snippetsOpen, setSnippetsOpen] = useState(false)
  const [snippetDraft, setSnippetDraft] = useState<Snippet | null>(null)
  const [projects, setProjects] = useState<Project[]>([])
  const [actionsByProject, setActionsByProject] = useState<Record<string, Action[]>>({})
  const [projectDraft, setProjectDraft] = useState<Project | null>(null)
  const [actionDraft, setActionDraft] = useState<Action | null>(null)
  const [runResult, setRunResult] = useState<RunOutcome | null>(null)
  const [runs, setRuns] = useState<Run[]>([])
  const [actionBusy, setActionBusy] = useState(false)
  const [confirmAction, setConfirmAction] = useState<Action | null>(null)
  const [micListening, setMicListening] = useState(false)
  const recognitionRef = useRef<any | null>(null)
  const runtimePoll = useRef<number | null>(null)
  const runtimePollBusy = useRef(false)
  const herdrMissingPolls = useRef(0)
  const connectSeq = useRef(0)
  const lastHostRef = useRef<Host | null>(null)
  const reconnectAttemptsRef = useRef(0)
  const filesSeq = useRef(0)
  const selectedIdRef = useRef<string | null>(null)
  const pollRef = useRef<number | null>(null)
  const phaseRef = useRef<ConnectPhase>(phase)
  phaseRef.current = phase

  useEffect(() => () => {
    if (messageTimer.current !== null) window.clearTimeout(messageTimer.current)
  }, [])

  const selected = hosts.find((h) => h.id === selectedId) ?? null
  const address = selected ? firstAddress(selected) : undefined
  selectedIdRef.current = selectedId

  useEffect(() => {
    if (!draft || !isDesktop()) return
    let active = true
    void tailscaleRuntimeInfo()
      .then((info) => { if (active) setTailscaleComponents(info) })
      .catch(() => { if (active) setTailscaleComponents(null) })
    return () => { active = false }
  }, [draft])

  useEffect(() => {
    let active = true
    const load = async () => {
      if (!isDesktop()) {
        if (active) setMessage('Preview 模式：浏览器仅作界面预览，配置不会写入磁盘。')
        return
      }
      try {
        const loaded = await listHosts()
        if (!active) return
        setHosts(loaded)
        setSelectedId(loaded[0]?.id ?? null)
        setFilesPath(loaded[0]?.default_remote_path || '/')
        setMessage(loaded.length === 0 ? '尚未配置工作站。' : '已读取 ' + loaded.length + ' 台工作站。')
      } catch (error) {
        if (active) setMessage('读取配置失败：' + String(error))
      }
    }
    void load()
    return () => { active = false }
  }, [setMessage])

  // Warm the selected embedded Tailscale path once per host selection.  The
  // previous implementation also warmed the first host during initial load
  // and depended on the entire `hosts` array, which could queue duplicate
  // daemon startups whenever host state changed.
  const selectedNetworkFingerprint = selected?.tailscale?.enabled && selected.tailscale.mode === 'EmbeddedUserspace'
    ? `${selected.id}:${selected.tailscale.state_dir ?? ''}:${selected.tailscale.auth_key_ref?.opaque_id ?? ''}`
    : ''
  useEffect(() => {
    if (!isDesktop() || !selectedId || !selectedNetworkFingerprint) return
    let active = true
    const timer = window.setTimeout(() => {
      if (active) void prepareHostNetwork(selectedId).catch(() => {})
    }, 120)
    return () => { active = false; window.clearTimeout(timer) }
  }, [selectedId, selectedNetworkFingerprint])

  // Poll for pending host-key decisions while connecting.
  useEffect(() => {
    if (phase !== 'connecting') return
    let active = true
    let busy = false
    const poll = async () => {
      if (!active || busy) return
      busy = true
      try {
        const requests = await pendingHostKeyRequests()
        if (active && requests.length > 0) {
          setHostKeyRequest((current) => current ?? requests[0])
        }
        const keyboardRequests = await pendingKeyboardInteractiveRequests()
        if (active && keyboardRequests.length > 0) {
          setKeyboardInteractiveRequest((current) => current ?? keyboardRequests[0])
        }
      } catch { /* transient */ }
      finally { busy = false }
    }
    void poll()
    pollRef.current = window.setInterval(() => void poll(), 500)
    return () => {
      active = false
      if (pollRef.current !== null) window.clearInterval(pollRef.current)
      pollRef.current = null
    }
  }, [phase])

  const refreshState = useCallback(async (hostId: string) => {
    if (!isDesktop()) return
    try {
      const state = await sessionState(hostId)
      const labels: Record<string, string> = {
        Disconnected: '未连接', ResolvingAddress: '解析地址…', Connecting: '连接中…',
        VerifyingHostKey: '验证主机密钥…', Authenticating: '认证中…', Ready: '已连接',
        Reconnecting: '重连中…', Failed: '连接失败',
      }
      setStateLabel(labels[state] ?? state)
      setPhase(state === 'Ready' ? 'ready' : state === 'Failed' ? 'failed' : state === 'Disconnected' ? 'idle' : 'connecting')
    } catch { /* transient */ }
  }, [])

  useResumeRecovery(Boolean(selectedId && phase !== 'idle'), () => {
    if (selectedId) void refreshState(selectedId)
  })

  // Poll remote runtime state (tmux sessions, herdr agents) while ready
  // and the runtime panel is actually visible.
  useEffect(() => {
    if (!isDesktop() || phase !== 'ready' || !selectedId || activeTab !== 'terminal' || !runtimeOpen) return
    const poll = async () => {
      if (runtimePollBusy.current) return
      runtimePollBusy.current = true
      const id = selectedId
      try {
        const sessionsPromise = tmuxList(id)
        const herdrPromise = (herdrVersion === null || herdrMissing)
          ? herdrDetect(id)
          : Promise.resolve(herdrVersion)
        const [sessionsResult, herdrResult] = await Promise.allSettled([sessionsPromise, herdrPromise])
        if (sessionsResult.status === 'fulfilled') {
          setTmuxSessions((previous) => sameTmuxSessions(previous, sessionsResult.value) ? previous : sessionsResult.value)
        }
        try {
          if (herdrResult.status === 'rejected') throw herdrResult.reason
          const version = herdrResult.value
          if (herdrVersion === null || herdrMissing) {
            herdrMissingPolls.current = 0
            setHerdrVersion(version)
            setHerdrMissing(false)
            setHerdrError(null)
          }
        } catch (error) {
          const detail = String(error)
          const definitelyMissing = /not installed|command not found|not recognized|找不到/i.test(detail)
          herdrMissingPolls.current = definitelyMissing ? herdrMissingPolls.current + 1 : 0
          setHerdrVersion(null)
          setHerdrMissing(definitelyMissing && herdrMissingPolls.current >= 3)
          setHerdrError(detail)
          setAgents((previous) => previous.length === 0 ? previous : [])
          return
        }
        try {
          const found = await herdrAgents(id)
          setAgents((previous) => sameAgents(previous, found) ? previous : found)
          setHerdrError(null)
        } catch (error) {
          setAgents((previous) => previous.length === 0 ? previous : [])
          setHerdrError(String(error))
        }
      } finally {
        runtimePollBusy.current = false
      }
    }
    void poll()
    runtimePoll.current = window.setInterval(() => void poll(), 8000)
    return () => {
      if (runtimePoll.current !== null) window.clearInterval(runtimePoll.current)
      runtimePoll.current = null
    }

    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [phase, selectedId, activeTab, runtimeOpen, herdrMissing, herdrVersion])

  // Open the initial pane when a session becomes ready.
  useEffect(() => {
    if (phase !== 'ready' || !selectedId || !isDesktop() || panes.length > 0) return
    let active = true
    void openPane(selectedId, 80, 24).then(([id, channel]) => {
      if (!active) {
        // React StrictMode double-runs effects in dev: the first run's
        // pane must be closed again or it becomes an orphan PTY on the
        // host until disconnect.
        void closePane(selectedId, id).catch(() => {})
        return
      }
      setPanes([{ id, channel }])
    }).catch((error) => setMessage('打开终端失败：' + String(error)))
    return () => { active = false }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [phase, selectedId, activeTab])

  // Detect disconnects and recover: key-auth hosts reconnect
  // automatically (bounded attempts); password hosts are prompted.
  useEffect(() => {
    if (!isDesktop() || !selectedId) return
    let active = true
    let busy = false
    const poll = async () => {
      if (!active || busy || phaseRef.current === 'idle') return
      busy = true
      try {
        const state = await sessionState(selectedId)
        if (state === 'Reconnecting') {
          setStateLabel('重连中…')
          setPhase('connecting')
          // Panes belong to the dead transport: the backend cleared them
          // on attach, so drop the stale ids and let the ready-effect
          // reopen a fresh pane after the reconnect completes.
          setPanes([])
          const host = lastHostRef.current
          const canReconnectWithoutPassword = host && (
            host.auth_ref !== null ||
            host.auth_mode === 'SshAgent' ||
            host.auth_mode === 'KeyboardInteractive'
          )
          if (canReconnectWithoutPassword && host && reconnectAttemptsRef.current < 3) {
            const attempt = reconnectAttemptsRef.current
            reconnectAttemptsRef.current += 1
            setMessage('连接断开，正在自动重连…')
            // Bounded backoff so bursts of drops do not hammer the host.
            await new Promise((resolve) => window.setTimeout(resolve, attempt * 1200))
            if (active && connectSeq.current > 0 && lastHostRef.current?.id === host.id) {
              void runConnect(host)
            }
          } else if (host && host.auth_ref === null && (host.auth_mode === 'Password' || host.auth_mode === 'PublicKey')) {
            reconnectAttemptsRef.current += 1
            if (reconnectAttemptsRef.current === 1) {
              setMessage(host.auth_mode === 'PublicKey'
                ? '连接已断开：请确认私钥口令后重新连接。'
                : '连接已断开：请输入密码重新连接。')
              setPromptPassword(true)
            }
          }
        } else if (state === 'Failed') {
          setPhase('failed')
          setStateLabel('连接失败')
        } else if (state === 'Ready') {
          reconnectAttemptsRef.current = 0
          if (phase !== 'ready') {
            setPhase('ready')
            setStateLabel('已连接')
          }
        }
      } catch { /* transient */ }
      finally { busy = false }
    }
    void poll()
    const timer = window.setInterval(() => void poll(), 3000)
    return () => { active = false; window.clearInterval(timer) }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId])

  const onSplit = async (dir: 'h' | 'v') => {
    if (!selectedId) return
    try {
      const [id, channel] = await openPane(selectedId, 80, 24)
      setPanes((items) => [...items, { id, channel }])
      setSplitDir(dir)
    } catch (error) {
      setMessage('分屏失败：' + String(error))
    }
  }

  const onClosePane = (id: number) => {
    if (!selectedId) return
    void closePane(selectedId, id).catch(() => {})
    setPanes((items) => items.filter((pane) => pane.id !== id))
  }

  const runConnect = async (host: Host, passwordValue?: string) => {
    lastHostRef.current = host
    const seq = ++connectSeq.current
    // Any panes from a previous transport are invalid; they will be
    // reopened by the ready-effect once this connection establishes.
    setPanes([])
    setPromptPassword(false)
    setRememberPassword(false)
    setHostKeyRequest(null)
    setKeyboardInteractiveRequest(null)
    if (passwordInputRef.current) passwordInputRef.current.value = ''
    setPhase('connecting')
    setMessage('正在连接 ' + host.label + '…')
    try {
      const result = await connectHost(host, passwordValue)
      if (seq !== connectSeq.current) return
      setMessage(result)
      void refreshState(host.id)
    } catch (error) {
      if (seq !== connectSeq.current) return
      setPhase('failed')
      setStateLabel('连接失败')
      setMessage('连接失败：' + String(error))
    }
  }

  const onConnectClick = () => {
    if (!selected) return
    if ((selected.auth_mode === 'Password' || selected.auth_mode === 'PublicKey') && selected.auth_ref === null) {
      setPromptPassword(true)
    } else {
      void runConnect(selected)
    }
  }

  const onHostKeyDecision = async (decision: 'trust_once' | 'trust_and_save' | 'reject') => {
    if (!hostKeyRequest) return
    await answerHostKey(hostKeyRequest.request_id, decision)
    setHostKeyRequest(null)
  }
  const onKeyboardInteractiveAnswer = async () => {
    if (!keyboardInteractiveRequest) return
    const responses = keyboardInteractiveInputs.current.map((input) => input.value)
    keyboardInteractiveInputs.current.forEach((input) => { input.value = '' })
    const requestId = keyboardInteractiveRequest.request_id
    setKeyboardInteractiveRequest(null)
    try {
      const answered = await answerKeyboardInteractive(requestId, responses)
      if (!answered) setMessage('交互式认证请求已过期，请重新连接。')
    } catch (error) {
      setMessage('提交交互式认证失败：' + String(error))
    }
  }
  const saveDraft = async () => {
    if (!draft) return
    if (!draft.label.trim() || !draft.username.trim() || !firstAddress(draft)?.hostname_or_ip.trim()) {
      setMessage('请填写名称、用户名和至少一个地址。')
      return
    }
    if (isDesktop()) {
      try {
        const editingActiveHost = selectedId === draft.id && (phase === 'ready' || phase === 'connecting')
        if (editingActiveHost) {
          await disconnectHost(draft.id)
          setPanes([])
          setTransfers({})
          setFiles([])
          setPhase('idle')
          setStateLabel('未连接')
        }
        await saveHost(draft)
        let storedDraft = draft
        const privateKeyPassphrase = privateKeyPassphraseRef.current?.value ?? ''
        if (draft.auth_mode === 'PublicKey' && privateKeyPassphrase) {
          storedDraft = await saveHostPassword(draft, privateKeyPassphrase)
        }
        if (privateKeyPassphraseRef.current) privateKeyPassphraseRef.current.value = ''
        const tailscaleKey = tailscaleKeyInputRef.current?.value ?? ''
        if (tailscaleKey.trim()) {
          const storedHost = await saveTailscaleAuthKey(draft.id, tailscaleKey)
          if (tailscaleKeyInputRef.current) tailscaleKeyInputRef.current.value = ''
          setHosts((items) => [...items.filter((h) => h.id !== storedHost.id), storedHost].sort((a, b) => a.label.localeCompare(b.label)))
          setSelectedId(storedHost.id)
          setDraft(null)
          setMessage(editingActiveHost
            ? '工作站与 Tailscale 凭据已保存；连接已安全断开，请使用新配置重新连接。'
            : '工作站与 Tailscale 凭据已保存。')
          return
        }
        setHosts((items) => [...items.filter((h) => h.id !== storedDraft.id), storedDraft].sort((a, b) => a.label.localeCompare(b.label)))
        setSelectedId(storedDraft.id)
        setDraft(null)
        setMessage(editingActiveHost
          ? '工作站配置已保存；连接已安全断开，请使用新配置重新连接。'
          : '工作站配置已保存。')
        return
      } catch (error) {
        setMessage('保存失败：' + String(error))
        return
      }
    }
    if (tailscaleKeyInputRef.current) tailscaleKeyInputRef.current.value = ''
    if (privateKeyPassphraseRef.current) privateKeyPassphraseRef.current.value = ''
    setHosts((items) => [...items.filter((h) => h.id !== draft.id), draft].sort((a, b) => a.label.localeCompare(b.label)))
    setSelectedId(draft.id)
    setDraft(null)
    setMessage(isDesktop()
      ? '工作站配置已保存；如果刚才正在连接，请使用新配置重新连接。'
      : 'Preview：配置仅保留在当前页面。')
  }

  const deleteSelected = async () => {
    if (!selected) return
    if (isDesktop()) {
      try { await deleteHost(selected.id) } catch (error) { setMessage('删除失败：' + String(error)); return }
    }
    setHosts((items) => items.filter((h) => h.id !== selected.id))
    setSelectedId(null)
    setMessage(isDesktop() ? '工作站已删除。' : 'Preview：已从当前页面移除。')
  }

  /** Switching hosts must tear down the previous session's UI state
   *  and its backend connection; otherwise the new host reuses stale
   *  pane ids, keeps stale transfers/tunnels and leaks the old session. */
  const onSelectHost = (host: Host) => {
    if (selectedId && selectedId !== host.id && isDesktop()) {
      void disconnectHost(selectedId).catch(() => {})
    }
    setSelectedId(host.id)
    setFilesPath(host.default_remote_path || '/')
    setPhase('idle')
    setStateLabel('未连接')
    setPanes([])
    setTransfers({})
    setFiles([])
    setSelectedRemote(null)
    setBridgeInfo(null)
    setTmuxSessions([])
    setAgents([])
    setHerdrVersion(null)
    herdrMissingPolls.current = 0
    setNewTmuxName('')
    setHostKeyRequest(null)
    setKeyboardInteractiveRequest(null)
    if (passwordInputRef.current) passwordInputRef.current.value = ''
    setPromptPassword(false)
    setMessage('')
  }

  /** First live pane id, or null when no terminal is open. Features that
   *  target "the terminal" use this so they keep working after pane 0 is
   *  closed (split panes can have any lowest id). */
  const firstPaneId = (): number | null => panes[0]?.id ?? null

  const onDisconnect = async () => {
    if (!selected) return
    await disconnectHost(selected.id).catch(() => {})
    setBridgeInfo(null)
    setPanes([])
    setTransfers({})
    setFiles([])
    setPhase('idle')
    setStateLabel('未连接')
    setMessage('已断开；远端 tmux/Herdr 会话不受影响。')
  }

  const updateDraft = (update: (host: Host) => Host) => setDraft((value) => (value ? update(value) : value))

  const onTmuxAttach = (name: string) => {
    if (!selected) return
    // Session names are interpolated into a shell line; only accept the
    // same character set the server-side tmux_new whitelist enforces so
    // a malicious remote session name cannot inject shell metacharacters.
    if (!/^[A-Za-z0-9_.-]{1,64}$/.test(name)) {
      setMessage('tmux 会话名包含不安全字符，已拒绝附加。')
      return
    }
    const paneId = firstPaneId()
    if (paneId === null) {
      setMessage('没有打开的终端，无法附加 tmux 会话。')
      return
    }
    void sendInput(selected.id, paneId, inputEncoder.encode('tmux attach -t ' + name + '\r'))
  }

  const onTmuxCreate = async () => {
    if (!selected || !newTmuxName.trim()) return
    try {
      await tmuxNew(selected.id, newTmuxName.trim())
      setNewTmuxName('')
      setTmuxSessions(await tmuxList(selected.id))
    } catch (error) {
      setMessage('tmux 新建失败：' + String(error))
    }
  }

  const onTmuxKill = async (name: string) => {
    if (!selected) return
    try {
      await tmuxKill(selected.id, name)
      setTmuxSessions(await tmuxList(selected.id))
    } catch (error) {
      setMessage('tmux 删除失败：' + String(error))
    }
  }

  const onHerdrAttach = () => {
    if (!selected) return
    void herdrAttach(selected.id).catch((error) => setMessage('herdr 启动失败：' + String(error)))
  }

  const onHerdrBridge = async () => {
    if (!selected) return
    try {
      const info = await herdrBridge(selected.id, 0)
      setBridgeInfo({ local: info.tunnel.local_addr, socket: info.remote_socket, tunnelId: info.tunnel.id, remotePort: info.remote_port })
      setMessage('herdr socket 已桥接：' + info.tunnel.local_addr)
    } catch (error) {
      setMessage('桥接失败：' + String(error))
    }
  }

  const onHerdrBridgeStop = async () => {
    if (!selected || !bridgeInfo) return
    try {
      // The local tunnel must be closed explicitly; the remote socat is
      // killed with the real remote port (never the local loopback one).
      await tunnelClose(bridgeInfo.tunnelId).catch(() => {})
      await herdrBridgeStop(selected.id, bridgeInfo.remotePort).catch(() => {})
      setBridgeInfo(null)
      setTunnels(await tunnelList())
      setMessage('herdr 桥接已停止')
    } catch (error) {
      setMessage('停止桥接失败：' + String(error))
    }
  }

  // ---- snippets ----
  useEffect(() => {
    if (!isDesktop()) return
    void snippetList().then(setSnippets).catch(() => {})
  }, [snippetsOpen])

  const onSnippetRun = (snippet: Snippet) => {
    if (!selected) return
    const paneId = firstPaneId()
    if (paneId === null) {
      setMessage('没有打开的终端，无法运行片段。')
      return
    }
    void sendInput(selected.id, paneId, inputEncoder.encode(snippet.text + '\r')).catch(() => {})
    setSnippetsOpen(false)
  }

  const onSnippetSave = async () => {
    if (!snippetDraft) return
    try {
      await snippetSave(snippetDraft)
      setSnippets(await snippetList())
      setSnippetDraft(null)
    } catch (error) {
      setMessage('片段保存失败：' + String(error))
    }
  }

  const onSnippetDelete = async (id: string) => {
    try {
      await snippetDelete(id)
      setSnippets(await snippetList())
    } catch (error) {
      setMessage('片段删除失败：' + String(error))
    }
  }

  // ---- workspace controls ----
  useEffect(() => {
    if (!isDesktop() || !selectedId) return
    void projectList(selectedId).then(setProjects).catch(() => {})
    if (activeTab === 'actions') {
      void runList(undefined, 50, selectedId).then(setRuns).catch(() => {})
    }
  }, [selectedId, activeTab])

  const refreshActions = async (projectId: string) => {
    try {
      const actions = await actionList(projectId)
      setActionsByProject((map) => ({ ...map, [projectId]: actions }))
    } catch { /* transient */ }
  }

  useEffect(() => {
    for (const project of projects) void refreshActions(project.id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projects])

  const onProjectSave = async () => {
    if (!projectDraft || !selectedId) return
    try {
      await projectSave(projectDraft)
      setProjects(await projectList(selectedId))
      setProjectDraft(null)
    } catch (error) {
      setMessage('项目保存失败：' + String(error))
    }
  }

  const onActionSave = async () => {
    if (!actionDraft) return
    try {
      await actionSave(actionDraft)
      await refreshActions(actionDraft.project_id)
      setActionDraft(null)
    } catch (error) {
      setMessage('动作保存失败：' + String(error))
    }
  }

  const onRunAction = async (action: Action) => {
    if (!selected) return
    // Dangerous commands always require an explicit user confirmation.  The
    // backend independently recomputes the level, so this only keeps the UI
    // from presenting a misleading one-click path.
    if (action.danger_level === 'Dangerous') {
      setConfirmAction(action)
      return
    }
    void executeAction(action, false)
  }

  const executeAction = async (action: Action, confirmed: boolean) => {
    if (!selected) return
    setActionBusy(true)
    try {
      const outcome = await runAction(selected.id, action, confirmed)
      setRunResult(outcome)
      void runList(undefined, 50, selected.id).then(setRuns).catch(() => {})
      setMessage('动作完成：退出码 ' + String(outcome.exit_code ?? '交互式') + (outcome.output_bytes > 400 ? '（输出已截断）' : ''))
    } catch (error) {
      setMessage('动作失败：' + String(error))
    } finally {
      setActionBusy(false)
    }
  }

  const onMicToggle = () => {
    if (micListening) {
      recognitionRef.current?.stop?.()
      setMicListening(false)
      return
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const windowAny = window as any
    const Impl = windowAny.SpeechRecognition || windowAny.webkitSpeechRecognition
    if (!Impl) {
      setMessage('当前 WebView 不支持语音输入')
      return
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const recognition = new Impl()
    recognition.lang = 'zh-CN'
    recognition.interimResults = false
    recognition.continuous = false
    recognition.onresult = (event: any) => {
      const text = event.results?.[0]?.[0]?.transcript ?? ''
      if (text.trim()) {
        const paneId = firstPaneId()
        if (paneId === null) {
          setMessage('没有打开的终端，无法输入语音文本。')
          return
        }
        // Read the CURRENT host at delivery time: the recognition closure
        // must not send to a host the user switched away from mid-speech.
        const target = selectedIdRef.current
        if (!target) return
        void sendInput(target, paneId, inputEncoder.encode(text.trim())).catch(() => {})
        setMessage('语音已输入：' + text.trim())
      }
    }
    recognition.onend = () => setMicListening(false)
    recognition.onerror = () => setMicListening(false)
    recognitionRef.current = recognition
    recognition.start()
    setMicListening(true)
  }

  // ---- file panel ----
  const refreshFiles = useCallback(async () => {
    if (!selectedId || !isDesktop()) return
    // Sequence guard: a slow response for an older path must not
    // overwrite the listing the user is currently looking at.
    const seq = ++filesSeq.current
    setFilesLoading(true)
    try {
      let listing = await sftpList(selectedId, filesPath)
      // Some SSH servers finish SFTP subsystem negotiation just after the
      // first successful request and can transiently return an empty root.
      // A Linux root directory cannot be genuinely empty in a usable host,
      // so retry that one ambiguous result before telling the user it is.
      if (filesPath === '/' && listing.length === 0 && seq === filesSeq.current) {
        await new Promise((resolve) => window.setTimeout(resolve, 350))
        listing = await sftpList(selectedId, filesPath)
      }
      if (seq === filesSeq.current) setFiles(listing)
    } catch (error) {
      if (seq === filesSeq.current) {
        setFiles([])
        setMessage('文件列表失败：' + String(error))
      }
    } finally {
      if (seq === filesSeq.current) setFilesLoading(false)
    }
  }, [selectedId, filesPath, setMessage])

  useEffect(() => {
    if (activeTab !== 'files' || phase !== 'ready' || !selectedId) return () => {}
    void refreshFiles()
    // Subscribe per host: switching hosts must tear down the old
    // subscription, otherwise stale transfer events from the previous
    // host keep updating this panel and the new host gets no events.
    return subscribeSftp(selectedId, (event) => {
      if ('State' in event) {
        const { id, status } = event.State
        setTransfers((map) => ({ ...map, [id]: { ...(map[id] ?? { transferred: 0, total: null, speed: 0 }), status } }))
        // An upload that just finished changed the remote listing.
        if (status === 'Completed') {
          void refreshFiles()
        }
        // Terminal transfer entries auto-dismiss after a grace period so
        // long-lived sessions do not accumulate stale rows.
        if (['Completed', 'Failed', 'Cancelled'].includes(status)) {
          window.setTimeout(() => {
            setTransfers((map) => {
              const next = { ...map }
              if (next[id] && ['Completed', 'Failed', 'Cancelled'].includes(next[id].status)) delete next[id]
              return next
            })
          }, 15000)
        }
      } else if ('Progress' in event) {
        const { id, progress } = event.Progress
        setTransfers((map) => ({ ...map, [id]: { ...(map[id] ?? { status: 'Transferring' }), transferred: progress.transferred, total: progress.total, speed: progress.speed_bps } }))
      } else if ('Failed' in event) {
        const { id, message } = event.Failed
        setTransfers((map) => ({ ...map, [id]: { ...(map[id] ?? { transferred: 0, total: null, speed: 0 }), status: 'Failed', message } }))
      }
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab, phase, selectedId])

  const onOpenRemote = (entry: RemoteFileMeta) => {
    if (entry.is_dir) {
      const base = filesPath === '/' ? '' : filesPath
      setFilesPath(base + '/' + entry.name)
      setSelectedRemote(null)
    } else {
      setSelectedRemote(filesPath === '/' ? '/' + entry.name : filesPath + '/' + entry.name)
    }
  }

  const onUploadClick = async () => {
    if (!selected) return
    const { open } = await import('@tauri-apps/plugin-dialog')
    const picked = await open({ multiple: true, directory: false })
    const paths = Array.isArray(picked) ? picked : (picked ? [picked] : [])
    for (const localPath of paths) {
      const name = localPath.split(/[\\/]/).pop() ?? 'file'
      const remote = filesPath === '/' ? '/' + name : filesPath + '/' + name
      try {
        await sftpUpload(selected.id, localPath, remote)
      } catch (error) {
        setMessage('上传失败：' + String(error))
      }
    }
  }

  const pinCurrentFolder = async () => {
    if (!selected) return
    const updated = { ...selected, default_remote_path: filesPath }
    if (isDesktop()) {
      try {
        await saveHost(updated)
      } catch (error) {
        setMessage(`固定目录失败：${String(error)}`)
        return
      }
    }
    setHosts((items) => items.map((host) => host.id === updated.id ? updated : host))
    setMessage(`已将 ${filesPath} 固定为 ${selected.label} 的默认文件目录。`)
  }

  const onDownloadClick = async () => {
    if (!selected || !selectedRemote) return
    const { save } = await import('@tauri-apps/plugin-dialog')
    const name = selectedRemote.split('/').pop() ?? 'download'
    const target = await save({ defaultPath: name })
    if (!target) return
    try {
      await sftpDownload(selected.id, selectedRemote, target)
    } catch (error) {
      setMessage('下载失败：' + String(error))
    }
  }

  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return bytes + ' B'
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KiB'
    if (bytes < 1024 * 1024 * 1024) return (bytes / (1024 * 1024)).toFixed(1) + ' MiB'
    return (bytes / (1024 * 1024 * 1024)).toFixed(1) + ' GiB'
  }

  // ---- tunnels & preview ----
  useEffect(() => {
    if (!isDesktop() || !(phase === 'ready')) return
    const poll = () => {
      void tunnelList().then(setTunnels).catch(() => {})
    }
    poll()
    const timer = window.setInterval(poll, 5000)
    return () => window.clearInterval(timer)
  }, [phase, selectedId])

  const onCreateTunnel = async () => {
    if (!selected) return
    const remotePort = Number(tunnelRemotePort)
    if (!remotePort || remotePort < 1 || remotePort > 65535) {
      setMessage('远程端口需为 1–65535')
      return
    }
    try {
      const local = tunnelLocal.trim() === '' ? 0 : Number(tunnelLocal)
      if (Number.isNaN(local) || local < 0 || local > 65535) {
        setMessage('本地端口需为 0–65535（0 表示自动分配）')
        return
      }
      const info = await tunnelOpen(selected.id, local, tunnelRemoteHost.trim() || '127.0.0.1', remotePort)
      setTunnels(await tunnelList())
      setTunnelLocal(String(info.local_addr.split(':')[1]))
      setMessage('隧道已建立：' + info.local_addr + ' → ' + info.remote_host + ':' + info.remote_port)
    } catch (error) {
      setMessage('隧道建立失败：' + String(error))
    }
  }

  const onCloseTunnel = async (id: string) => {
    try {
      await tunnelClose(id)
      setTunnels(await tunnelList())
      setPreviewUrl((url) => {
        const closed = tunnels.find((tunnel) => tunnel.id === id)
        if (closed && url?.includes(closed.local_addr.split(':')[1])) return null
        return url
      })
    } catch (error) {
      setMessage('隧道关闭失败：' + String(error))
    }
  }

  const listeningTunnels = tunnels.filter((tunnel) => tunnel.state === 'Listening')

  return (
    <div className={'shell' + (terminalFocusMode ? ' terminal-focus-mode' : '')}>
      <aside className="sidebar">
        <div className="brand"><span className="brand-mark" /><span className="brand-name">kodework<em>.</em></span><span className="brand-meta">远程工作台</span></div>
        <div className="section-label">工作站 <button aria-label="添加工作站" onClick={() => setDraft(newHost())}><Icon name="plus" size={13} /></button></div>
        {hosts.length === 0 && <div className="empty-nav">还没有工作站配置</div>}
        {hosts.map((host) => (
          <button
            className={'nav-row ' + (selectedId === host.id ? 'selected' : '')}
            key={host.id}
            onClick={() => onSelectHost(host)}
          >
            <span className={'dot ' + (phase === 'ready' && selectedId === host.id ? 'online' : 'offline')} />
            <span>{host.label}</span>
            <small>{host.tailscale?.enabled ? 'Tailscale' : '手动地址'}</small>
          </button>
        ))}
        <div className="sidebar-footer">
          <div className="connection-pill">
            <span className={'dot ' + (phase === 'ready' ? 'online' : 'offline')} />
            {stateLabel} · {phase === 'ready' && selected ? selected.label : '无会话'}
          </div>
          <div className="sidebar-actions">
            <button className="settings" onClick={() => setSnippetsOpen(true)}><Icon name="zap" size={14} />片段</button>
            <button className="settings" onClick={() => { setSettingsOpen(true); void autostartStatus().then(setAutoStart).catch(() => {}) }}><Icon name="gear" size={14} />设置</button>
          </div>
        </div>
      </aside>

      <main className="main">
        <WorkspaceHeader selected={selected} address={address} phase={phase} onConnect={onConnectClick} onDisconnect={() => { void onDisconnect() }} onDelete={() => { void deleteSelected() }} onTunnel={() => setTunnelPanelOpen(true)} onEdit={() => { if (selected) setDraft(structuredClone(selected)) }} />

        <div className="workspace-tabs">
          <button className={activeTab === 'terminal' ? 'active' : ''} onClick={() => setActiveTab('terminal')}><Icon name="terminal" size={13} />终端</button>
          <button className={activeTab === 'local' ? 'active' : ''} onClick={() => setActiveTab('local')}><Icon name="computer" size={13} />本机</button>
          <button className={activeTab === 'files' ? 'active' : ''} onClick={() => setActiveTab('files')}><Icon name="folder" size={13} />文件</button>
          <button className={activeTab === 'preview' ? 'active' : ''} onClick={() => setActiveTab('preview')} disabled={listeningTunnels.length === 0 && !previewUrl}><Icon name="globe" size={13} />预览</button>
          <button className={activeTab === 'actions' ? 'active' : ''} onClick={() => setActiveTab('actions')}><Icon name="activity" size={13} />活动</button>
          <span className="tmux">
            {selected?.tailscale?.enabled ? 'Tailscale 已配置' : '地址候选待配置'} · {selected ? selected.default_runtime.toLowerCase() : '未连接'}
          </span>
        </div>

        <div className="main-area">
          {activeTab === 'actions' ? (
            <section className="terminal-card files-card">
              <div className="terminal-head">
                <span>◷ 项目与动作</span>
                <span className="terminal-tools">Quick · Interactive · Background</span>
              </div>
              <div className="files-body">
                {projects.length === 0 && <div className="runtime-empty">还没有项目；点击“新建项目”创建</div>}
                {projects.map((project) => (
                  <div key={project.id} style={{ marginBottom: 10 }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '6px 4px' }}>
                      <Icon name="folder" size={12} />
                      <span style={{ fontWeight: 600, fontSize: 13 }}>{project.name}</span>
                      <span className="tunnel-remote">{project.remote_cwd}</span>
                      <span style={{ marginLeft: 'auto', display: 'flex', gap: 6 }}>
                        <button className="mini" onClick={() => setProjectDraft({ ...project })}>编辑</button>
                        <button className="mini danger" onClick={() => void projectDelete(project.id).then(() => setProjects((items) => items.filter((p) => p.id !== project.id))).catch((e) => setMessage('删除失败：' + String(e)))}>删除</button>
                        <button className="mini" onClick={() => setActionDraft({ id: crypto.randomUUID(), project_id: project.id, name: '', command: '', mode: 'Quick', cwd: project.remote_cwd, timeout_ms: 60000, danger_level: 'Safe', confirmation: 'Never', env: {} })}>＋动作</button>
                      </span>
                    </div>
                    {(actionsByProject[project.id] ?? []).map((action) => (
                      <div className="tunnel-row" key={action.id} style={{ paddingLeft: 20 }}>
                        <span style={{ fontWeight: 600, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{action.name}</span>
                        <span className="tunnel-state" style={action.danger_level === 'Dangerous' ? { color: 'var(--danger)' } : {}}>{action.danger_level}</span>
                        <span className="tunnel-conns">{action.mode}</span>
                        <button className={'mini' + (action.danger_level === 'Dangerous' ? ' danger' : '')} disabled={actionBusy} onClick={() => void onRunAction(action)}>{actionBusy ? <Icon name="power" size={10} /> : <Icon name="play" size={10} />}{actionBusy ? '运行中' : '运行'}</button>
                        <button className="mini" onClick={() => setActionDraft({ ...action })}>编辑</button>
                        <button className="mini danger" onClick={() => void actionDelete(action.id).then(() => refreshActions(project.id)).catch((e) => setMessage('删除失败：' + String(e)))}><Icon name="trash" size={10} />删除</button>
                      </div>
                    ))}
                  </div>
                ))}
                {runResult && (
                  <div style={{ marginTop: 8, padding: 10, background: 'var(--bg-inset)', border: '1px solid var(--line)', borderRadius: 5, fontFamily: 'var(--mono)', fontSize: 11, whiteSpace: 'pre-wrap', maxHeight: 160, overflowY: 'auto' }}>
                    {runResult.stdout_preview || '(无输出)'}
                    {runResult.stderr_preview && '\n[stderr] ' + runResult.stderr_preview}
                  </div>
                )}
                <div style={{ marginTop: 14, borderTop: '1px solid var(--line)', paddingTop: 8 }}>
                  <div className="runtime-title"><span>运行历史</span><small>{runs.length} 条</small></div>
                  {runs.length === 0 ? <div className="runtime-empty">暂无运行记录</div> : runs.slice(0, 12).map((run) => (
                    <div className="tunnel-row" key={run.id} style={{ paddingLeft: 4 }}>
                      <span className="tunnel-state">{run.status}</span>
                      <span className="tunnel-conns">退出 {run.exit_code ?? '—'}</span>
                      <span className="tunnel-remote">{run.remote_session_ref ?? `${run.output_bytes} bytes`}</span>
                    </div>
                  ))}
                </div>
              </div>
              <div className="files-toolbar" style={{ borderTop: '1px solid var(--line)', borderBottom: 'none' }}>
                <button className="mini" onClick={() => setProjectDraft({ id: crypto.randomUUID(), host_id: selectedId ?? '', name: '', remote_cwd: '~', preferred_runtime: selected?.default_runtime ?? 'Tmux' })}><Icon name="plus" size={11} />新建项目</button>
                {runResult && <button className="mini" onClick={() => setRunResult(null)}>清除输出</button>}
              </div>
            </section>
          ) : activeTab === 'preview' ? (
            <section className="preview-card">
              <div className="preview-head">
                <Icon name="globe" size={12} />
                <span>Web Preview</span>
                {previewUrl && <span>{previewUrl}</span>}
                {listeningTunnels.length > 0 && (
                  <select
                    style={{ marginLeft: 'auto', background: 'var(--bg-inset)', border: '1px solid var(--line-strong)', borderRadius: 4, padding: '3px 6px', fontSize: 11, color: 'var(--text)' }}
                    value={previewUrl ?? ''}
                    onChange={(event) => setPreviewUrl(event.target.value || null)}
                  >
                    <option value="">选择隧道…</option>
                    {listeningTunnels.map((tunnel) => (
                      <option key={tunnel.id} value={"http://" + tunnel.local_addr}>
                        {tunnel.local_addr} → {tunnel.remote_host}:{tunnel.remote_port}
                      </option>
                    ))}
                  </select>
                )}
              </div>
              <div className="preview-body">
                {previewUrl ? (
                  <iframe title="web-preview" src={previewUrl} sandbox="allow-scripts allow-forms allow-same-origin" />
                ) : (
                  <div className="preview-empty">
                    <Icon name="eye" size={34} />
                    <div>先建立一条隧道，再在这里预览远程 Web 服务</div>
                  </div>
                )}
              </div>
            </section>
          ) : activeTab === 'files' && !(phase === 'ready' && selected) ? (
            <section className="terminal-card">
              <div className="terminal-empty">
                <div className="empty-symbol"><Icon name="folder" size={40} /></div>
                <h2>连接后可用</h2>
                <p>文件面板通过 SFTP 流式访问远程目录。</p>
              </div>
            </section>
          ) : activeTab === 'files' && phase === 'ready' && selected ? (
            <FilesPanel
              currentPath={filesPath}
              entries={files}
              loading={filesLoading}
              selectedRemote={selectedRemote}
              transfers={transfers}
              formatSize={formatSize}
              onPathChange={setFilesPath}
              onOpen={onOpenRemote}
              onRefresh={() => { void refreshFiles() }}
              onUpload={() => { void onUploadClick() }}
              onDownload={() => { void onDownloadClick() }}
              pinnedPath={selected.default_remote_path || '/'}
              pinned={selected.default_remote_path === filesPath}
              onPinCurrentPath={() => { void pinCurrentFolder() }}
              onGoPinnedPath={() => { setSelectedRemote(null); setFilesPath(selected.default_remote_path || '/') }}
              onYazi={() => { void yaziAttach(selected.id).catch((error) => setMessage(`yazi 启动失败：${String(error)}`)) }}
              onPauseTransfer={(id) => { void sftpPause(selected.id, id).catch(() => {}) }}
              onResumeTransfer={(id) => { void sftpResume(selected.id, id).catch(() => {}) }}
              onCancelTransfer={(id) => { void sftpCancel(selected.id, id).catch(() => {}) }}
              onDismissTransfer={(id) => setTransfers((map) => { const next = { ...map }; delete next[id]; return next })}
            />
          ) : null}
          <TerminalWorkspace
            visible={activeTab === 'terminal'} phase={phase} stateLabel={stateLabel} selected={selected}
            panes={panes} splitDir={splitDir} micListening={micListening} runtimeOpen={runtimeOpen}
            focusMode={terminalFocusMode} onMicToggle={onMicToggle} onSplit={(direction) => { void onSplit(direction) }}
            onToggleRuntime={() => setRuntimeOpen((open) => !open)} onToggleFocus={() => setTerminalFocusMode((active) => !active)}
            onClosePane={onClosePane} onPasteStatus={setMessage}
          />
          <LocalTerminalWorkspace visible={activeTab === 'local'} onStatus={setMessage} />
          {phase === 'ready' && selected && activeTab === 'terminal' && runtimeOpen && (
            <RuntimePanel
              herdrVersion={herdrVersion} herdrMissing={herdrMissing} herdrError={herdrError} agents={agents}
              bridgeInfo={bridgeInfo ? { local: bridgeInfo.local, socket: bridgeInfo.socket } : null}
              tmuxSessions={tmuxSessions} newTmuxName={newTmuxName} onNewTmuxName={setNewTmuxName}
              onHerdrBridge={() => { void onHerdrBridge() }} onHerdrBridgeStop={() => { void onHerdrBridgeStop() }} onHerdrAttach={onHerdrAttach}
              onTmuxAttach={onTmuxAttach} onTmuxKill={(name) => { void onTmuxKill(name) }} onTmuxCreate={() => { void onTmuxCreate() }}
            />
          )}
        </div>

        <footer className="statusbar">
          <span className="status-security"><Icon name="check" size={12} />本地元数据 · 凭据不进入 SQLite</span>
          <span className={messageIsError ? 'status-message error' : 'status-message'}>{message || '就绪'}</span>
          <span>{isDesktop() ? 'Desktop' : 'Preview'} · v{__APP_VERSION__}</span>
        </footer>
      </main>
      {draft && (
        <HostEditor
          draft={draft} tailscaleComponents={tailscaleComponents} tailscaleKeyInputRef={tailscaleKeyInputRef}
          privateKeyPassphraseRef={privateKeyPassphraseRef} updateDraft={updateDraft} onClose={() => setDraft(null)}
          onSave={() => { void saveDraft() }} onMessage={setMessage}
        />
      )}

      {promptPassword && selected && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <form className="host-modal" onSubmit={(event) => {
            event.preventDefault()
            const passwordValue = passwordInputRef.current?.value ?? ''
            if (passwordInputRef.current) passwordInputRef.current.value = ''
            if (rememberPassword) {
              void saveHostPassword(selected, passwordValue)
                .then((storedHost) => {
                  setHosts((items) => items.map((item) => item.id === storedHost.id ? storedHost : item))
                  return runConnect(storedHost)
                })
                .catch((error) => setMessage('保存凭据失败：' + String(error)))
            } else {
              void runConnect(selected, passwordValue)
            }
          }}>
            <div className="modal-head">
              <div><div className="eyebrow">AUTHENTICATION</div><h2>连接 {selected.label}</h2></div>
              <button type="button" className="ghost" onClick={() => {
                if (passwordInputRef.current) passwordInputRef.current.value = ''
                setPromptPassword(false)
                setRememberPassword(false)
                setPhase('idle')
                setStateLabel('未连接')
              }}>取消</button>
            </div>
            <label>{selected.auth_mode === 'PublicKey' ? '私钥口令（如无可留空）' : '密码'}
              <input ref={passwordInputRef} type="password" autoComplete="off" autoFocus />
            </label>
            <label className="toggle-row">
              <input type="checkbox" checked={rememberPassword} onChange={(event) => setRememberPassword(event.target.checked)} />
              使用当前操作系统的安全密钥环保存{selected.auth_mode === 'PublicKey' ? '口令' : '密码'}
            </label>
            <p className="modal-note">凭据仅经 IPC 传递到本地 Rust 进程，不写入数据库或日志。</p>
            <button className="primary modal-submit" type="submit">连接</button>
          </form>
        </div>
      )}

      {projectDraft && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">PROJECT</div><h2>编辑项目</h2></div>
              <button type="button" className="ghost" onClick={() => setProjectDraft(null)}>关闭</button>
            </div>
            <label>名称
              <input value={projectDraft.name} onChange={(e) => setProjectDraft({ ...projectDraft, name: e.target.value })} autoFocus />
            </label>
            <label>远程目录
              <input value={projectDraft.remote_cwd} onChange={(e) => setProjectDraft({ ...projectDraft, remote_cwd: e.target.value })} placeholder="~/projects/foo" />
            </label>
            <div className="modal-actions">
              <button className="ghost" onClick={() => setProjectDraft(null)}>取消</button>
              <button className="primary" onClick={() => void onProjectSave()}>保存</button>
            </div>
          </div>
        </div>
      )}

      {actionDraft && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">ACTION</div><h2>编辑动作</h2></div>
              <button type="button" className="ghost" onClick={() => setActionDraft(null)}>关闭</button>
            </div>
            <label>名称
              <input value={actionDraft.name} onChange={(e) => setActionDraft({ ...actionDraft, name: e.target.value })} autoFocus />
            </label>
            <label>命令
              <textarea
                rows={3}
                value={actionDraft.command}
                onChange={(e) => setActionDraft({ ...actionDraft, command: e.target.value })}
                style={{ background: 'var(--bg-inset)', border: '1px solid var(--line-strong)', borderRadius: 4, padding: '7px 9px', fontSize: 12, fontFamily: 'var(--mono)', color: 'var(--text)', resize: 'vertical' }}
              />
            </label>
            <div className="form-row">
              <label>模式
                <select value={actionDraft.mode} onChange={(e) => setActionDraft({ ...actionDraft, mode: e.target.value as Action['mode'] })}>
                  <option value="Quick">Quick（快速）</option>
                  <option value="Interactive">Interactive（终端）</option>
                  <option value="Background">Background（后台）</option>
                </select>
              </label>
              <label>危险级别
                <select value={actionDraft.danger_level} disabled>
                  <option value="Safe">安全</option>
                  <option value="Review">需复核</option>
                  <option value="Dangerous">危险</option>
                </select>
                <span className="modal-note">保存时按命令内容自动判定（服务端强制），不可手动设置。</span>
              </label>
            </div>
            <label>超时（毫秒，留空默认 30s）
              <input type="number" value={actionDraft.timeout_ms ?? ''} onChange={(e) => setActionDraft({ ...actionDraft, timeout_ms: e.target.value ? Number(e.target.value) : null })} />
            </label>
            <div className="modal-actions">
              <button className="ghost" onClick={() => setActionDraft(null)}>取消</button>
              <button className="primary" onClick={() => void onActionSave()}>保存</button>
            </div>
          </div>
        </div>
      )}

      {confirmAction && selected && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">CONFIRM</div><h2>确认运行危险动作</h2></div>
            </div>
            <p className="modal-note">动作 <strong>{confirmAction.name}</strong> 被标记为危险，将在远程执行：</p>
            <code className="fingerprint">{confirmAction.command}</code>
            <div className="modal-actions">
              <button className="ghost" onClick={() => setConfirmAction(null)}>取消</button>
              <button className="primary" style={{ background: 'var(--danger)' }} onClick={() => { const action = confirmAction; setConfirmAction(null); void executeAction(action, true) }}>确认执行</button>
            </div>
          </div>
        </div>
      )}

      {snippetsOpen && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">SNIPPETS</div><h2>命令片段</h2></div>
              <button type="button" className="ghost" onClick={() => setSnippetsOpen(false)}>关闭</button>
            </div>
            {snippetDraft ? (
              <>
              <label>名称
                <input value={snippetDraft.name} onChange={(e) => setSnippetDraft({ ...snippetDraft, name: e.target.value })} autoFocus onKeyDown={(e) => { if (e.key === 'Enter') void onSnippetSave() }} />
              </label>
              <label>命令内容
                <textarea
                  rows={4}
                  value={snippetDraft.text}
                  onChange={(e) => setSnippetDraft({ ...snippetDraft, text: e.target.value })}
                  style={{ background: 'var(--bg-inset)', border: '1px solid var(--line-strong)', borderRadius: 4, padding: '7px 9px', fontSize: 12, fontFamily: 'var(--mono)', color: 'var(--text)', resize: 'vertical' }}
                />
              </label>
              <div className="modal-actions">
                <button className="ghost" onClick={() => setSnippetDraft(null)}>取消</button>
                <button className="primary" onClick={() => void onSnippetSave()}>保存</button>
              </div>
              </>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6, maxHeight: 320, overflowY: 'auto' }}>
                {snippets.length === 0 && <div className="runtime-empty">还没有片段；点击下方“新建”添加</div>}
                {snippets.map((snippet) => (
                  <div className="tunnel-row" key={snippet.id}>
                    <span style={{ fontWeight: 600, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{snippet.name}</span>
                    <span className="tunnel-remote">{snippet.text.length} 字符</span>
                    <button className="mini" onClick={() => onSnippetRun(snippet)}><Icon name="play" size={10} />执行</button>
                    <button className="mini" onClick={() => setSnippetDraft({ ...snippet })}><Icon name="gear" size={10} />编辑</button>
                    <button className="mini danger" onClick={() => void onSnippetDelete(snippet.id)}><Icon name="trash" size={10} />删除</button>
                  </div>
                ))}
              </div>
            )}
            {!snippetDraft && (
              <button className="primary modal-submit" onClick={() => setSnippetDraft({ id: crypto.randomUUID(), name: '', text: '', sort_order: 0 })}><Icon name="plus" size={12} />新建片段</button>
            )}
          </div>
        </div>
      )}

      {tunnelPanelOpen && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">PORT FORWARDING</div><h2>SSH 隧道</h2></div>
              <button type="button" className="ghost" onClick={() => setTunnelPanelOpen(false)}>关闭</button>
            </div>
            <div className="tunnel-form">
              <label>本地端口<span className="modal-note">留空自动分配</span>
                <input value={tunnelLocal} onChange={(e) => setTunnelLocal(e.target.value)} placeholder="0" />
              </label>
              <label>远程主机
                <input value={tunnelRemoteHost} onChange={(e) => setTunnelRemoteHost(e.target.value)} placeholder="127.0.0.1" />
              </label>
              <label>远程端口
                <input value={tunnelRemotePort} onChange={(e) => setTunnelRemotePort(e.target.value)} placeholder="3000" />
              </label>
              <button className="primary" onClick={() => void onCreateTunnel()}><Icon name="plus" size={12} />建立</button>
            </div>
            <div className="modal-note" style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
              仅监听 127.0.0.1，不暴露到局域网；断开连接时隧道自动失效。
            </div>
            {tunnels.length === 0 ? (
              <div className="runtime-empty">还没有隧道</div>
            ) : tunnels.map((tunnel) => (
              <div className="tunnel-row" key={tunnel.id}>
                <span className={"tunnel-state " + tunnel.state.toLowerCase()}>{tunnel.state}</span>
                <span className="tunnel-local">{tunnel.local_addr}</span>
                <span className="tunnel-remote">→ {tunnel.remote_host}:{tunnel.remote_port}</span>
                <span className="tunnel-conns">{tunnel.active_connections} 连接</span>
                {tunnel.state === 'Listening' && (
                  <button className="mini" onClick={() => { setPreviewUrl("http://" + tunnel.local_addr); setActiveTab('preview'); setTunnelPanelOpen(false) }}><Icon name="eye" size={10} />预览</button>
                )}
                <button className="mini danger" onClick={() => void onCloseTunnel(tunnel.id)}><Icon name="close" size={10} />关闭</button>
              </div>
            ))}
          </div>
        </div>
      )}

      {settingsOpen && (
        <SettingsPanel
          autoStart={autoStart} updateCheck={updateCheck} updateBusy={updateBusy} version={__APP_VERSION__} theme={theme} onThemeChange={onThemeChange}
          onClose={() => setSettingsOpen(false)}
          onAutoStart={(enabled) => { setAutoStart(enabled); void setAutostart(enabled).then(setAutoStart).catch((error) => setMessage('自启设置失败：' + String(error))) }}
          onCheck={() => { setUpdateBusy(true); void checkForUpdates().then(setUpdateCheck).catch((error) => setUpdateCheck({ status: 'error', error: String(error) })).finally(() => setUpdateBusy(false)) }}
          onInstall={() => { setUpdateBusy(true); void installUpdate().then((result) => { if (result.ok) { setUpdateCheck({ status: 'up-to-date' }); setMessage('更新已安装，重启应用后生效。') } else { setUpdateCheck({ status: 'error', error: result.error ?? '安装失败' }) } }).finally(() => setUpdateBusy(false)) }}
        />
      )}


      {keyboardInteractiveRequest && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <form className="host-modal" onSubmit={(event) => { event.preventDefault(); void onKeyboardInteractiveAnswer() }}>
            <div className="modal-head">
              <div><div className="eyebrow">KEYBOARD-INTERACTIVE</div><h2>{keyboardInteractiveRequest.name || '服务器认证'}</h2></div>
            </div>
            {keyboardInteractiveRequest.instructions && <p className="modal-note">{keyboardInteractiveRequest.instructions}</p>}
            {keyboardInteractiveRequest.prompts.map((prompt, index) => (
              <label key={`${keyboardInteractiveRequest.request_id}-${index}`}>{prompt.prompt || `响应 ${index + 1}`}
                <input
                  ref={(element) => { if (element) keyboardInteractiveInputs.current[index] = element }}
                  type={prompt.echo ? 'text' : 'password'}
                  autoComplete="off"
                  spellCheck={false}
                  autoFocus={index === 0}
                />
              </label>
            ))}
            <p className="modal-note">响应不会写入 SQLite、日志或工作区快照；请求超过两分钟自动失效。</p>
            <div className="modal-actions">
              <button className="primary" type="submit">提交认证</button>
            </div>
          </form>
        </div>
      )}

      {hostKeyRequest && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">HOST KEY VERIFICATION</div><h2>确认主机密钥</h2></div>
            </div>
            <p className="fingerprint-label">服务器 {hostKeyRequest.info.hostname}:{hostKeyRequest.info.port} 的主机密钥指纹：</p>
            <code className="fingerprint">{hostKeyRequest.info.fingerprint}</code>
            <p className="modal-note">算法 {hostKeyRequest.info.algorithm}。请与服务器所有者核对该指纹；密钥变化将被硬性阻止。</p>
            <div className="modal-actions">
              <button className="ghost" onClick={() => void onHostKeyDecision('reject')}>拒绝</button>
              <button className="ghost" onClick={() => void onHostKeyDecision('trust_once')}>本次信任</button>
              <button className="primary" onClick={() => void onHostKeyDecision('trust_and_save')}>信任并保存</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
