// Kodework main shell: host rail, connect flow with host-key
// confirmation, xterm terminal pane, status bar. No fabricated success.

import { useCallback, useEffect, useRef, useState } from 'react'
import './App.css'
import type { Action, HerdrAgentInfo, Host, HostKeyRequest, KeyboardInteractiveRequest, Project, RemoteFileMeta, Run, RunOutcome, Snippet, TmuxSession, TunnelInfo, UpdateCheck } from './api'
import {
  answerHostKey, answerKeyboardInteractive, asConnectError, connectHost, deleteHost, isDesktop,
  listHosts, pendingHostKeyRequests, pendingKeyboardInteractiveRequests, prepareHostNetwork, saveHost, saveHostPassword, saveTailscaleAuthKey, sessionState, subscribeSessionRuntime, disconnectHost, tailscaleRuntimeInfo,
  actionDelete, actionList, actionSave,
  closePane, herdrAgents, herdrAttach, herdrBridge, herdrBridgeStopById, herdrDetect, openPane,
  projectDelete, projectList, projectSave, runAction, runList, runReconcile, sendInput,
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
import { useLanguage } from './settings/useLanguage'
import { LanguagePrompt } from './settings/LanguagePrompt'
import { translate } from './i18n'
import { WorkspaceHeader } from './workspace/WorkspaceHeader'

const inputEncoder = new TextEncoder()

const newHost = (): Host => ({
  id: crypto.randomUUID(),
  label: 'New workstation',
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
  const [message, setMessageState] = useState('')
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
  // Empty initial value: language is resolved below; render sites fall
  // back to the translated "disconnected" label until the first refresh.
  const [stateLabel, setStateLabel] = useState('')
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
  const [language, onLanguageChange, needsLanguagePrompt] = useLanguage()
  const t = useCallback((key: Parameters<typeof translate>[1], ...args: string[]) => translate(language, key, ...args), [language])
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
  const [bridgeInfo, setBridgeInfo] = useState<{ local: string; socket: string; tunnelId: string; bridgeId: string } | null>(null)
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
        if (active) setMessage(t('previewMode'))
        return
      }
      try {
        const loaded = await listHosts()
        if (!active) return
        setHosts(loaded)
        setSelectedId(loaded[0]?.id ?? null)
        setFilesPath(loaded[0]?.default_remote_path || '/')
        setMessage(loaded.length === 0 ? t('noWorkstationConfigured') : t('loadedWorkstations', String(loaded.length)))
      } catch (error) {
        if (active) setMessage(t('loadConfigFailed', String(error)))
      }
    }
    void load()
    return () => { active = false }
  }, [setMessage, t])

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
        Disconnected: t('stateDisconnected'), ResolvingAddress: t('stateResolving'), Connecting: t('stateConnecting'),
        VerifyingHostKey: t('stateVerifyingHostKey'), Authenticating: t('stateAuthenticating'), Ready: t('stateReady'),
        Reconnecting: t('stateReconnecting'), WaitingForCredential: t('stateWaitingForCredential'), Failed: t('stateFailed'),
      }
      setStateLabel(labels[state] ?? state)
      setPhase(state === 'Ready' ? 'ready' : state === 'Failed' ? 'failed' : state === 'Disconnected' ? 'idle' : 'connecting')
    } catch { /* transient */ }
  }, [t])

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
    }).catch((error) => setMessage(t('openTerminalFailed', String(error))))
    return () => { active = false }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [phase, selectedId, activeTab])

  // Observe native connection supervision. The renderer updates its view and
  // prompts for credentials, but never owns reconnect timing or retry loops.
  useEffect(() => {
    if (!isDesktop() || !selectedId) return
    const unsubscribe = subscribeSessionRuntime(selectedId, (snapshot) => {
        const state = snapshot.state
        if (state === 'Reconnecting') {
          setStateLabel(t('stateReconnecting'))
          setPhase('connecting')
          // Panes belong to the dead transport: the backend cleared them
          // on attach, so drop the stale ids and let the ready-effect
          // reopen a fresh pane after the reconnect completes.
          setPanes([])
          const host = lastHostRef.current
          const canReconnectWithoutPassword = host && (
            host.auth_ref !== null ||
            host.auth_mode === 'PublicKey' ||
            host.auth_mode === 'SshAgent' ||
            host.auth_mode === 'KeyboardInteractive'
          )
          if (canReconnectWithoutPassword && host) {
            setMessage(t('reconnectAttempting'))
          } else if (host && host.auth_ref === null && host.auth_mode === 'Password') {
            setMessage(t('reconnectNeedsPassword'))
            setPromptPassword(true)
          }
        } else if (state === 'WaitingForCredential') {
          setPhase('connecting')
          setStateLabel(t('stateWaitingForCredential'))
          if (lastHostRef.current?.auth_mode === 'Password') {
            setMessage(t('reconnectNeedsCredential'))
            setPromptPassword(true)
          }
        } else if (state === 'Failed') {
          setPhase('failed')
          setStateLabel(t('stateFailed'))
        } else if (state === 'Ready') {
          if (phaseRef.current !== 'ready') {
            setPhase('ready')
            setStateLabel(t('stateReady'))
          }
        }
    })
    return unsubscribe
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedId])

  const onSplit = async (dir: 'h' | 'v') => {
    if (!selectedId) return
    try {
      const [id, channel] = await openPane(selectedId, 80, 24)
      setPanes((items) => [...items, { id, channel }])
      setSplitDir(dir)
    } catch (error) {
      setMessage(t('splitFailed', String(error)))
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
    setMessage(t('connectingTo', host.label))
    try {
      const result = await connectHost(host, passwordValue)
      if (seq !== connectSeq.current) return
      setMessage(result)
      void refreshState(host.id)
    } catch (error) {
      if (seq !== connectSeq.current) return
      const connectError = asConnectError(error)
      if (connectError?.kind === 'CredentialRequired') {
        setPhase('failed')
        setStateLabel(t('stateFailed'))
        setMessage(t('credentialRequired'))
        setPromptPassword(true)
        return
      }
      setPhase('failed')
      setStateLabel(t('stateFailed'))
      setMessage(t('connectFailed', connectError?.detail ?? String(error)))
    }
  }

  const onConnectClick = () => {
    if (!selected) return
    if (selected.auth_mode === 'Password' && selected.auth_ref === null) {
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
      if (!answered) setMessage(t('keyboardInteractiveExpired'))
    } catch (error) {
      setMessage(t('keyboardInteractiveSubmitFailed', String(error)))
    }
  }
  const saveDraft = async () => {
    if (!draft) return
    if (!draft.label.trim() || !draft.username.trim() || !firstAddress(draft)?.hostname_or_ip.trim()) {
      setMessage(t('fillRequiredFields'))
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
          setStateLabel(t('stateDisconnected'))
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
            ? t('hostAndTailscaleSavedDisconnected')
            : t('hostAndTailscaleSaved'))
          return
        }
        setHosts((items) => [...items.filter((h) => h.id !== storedDraft.id), storedDraft].sort((a, b) => a.label.localeCompare(b.label)))
        setSelectedId(storedDraft.id)
        setDraft(null)
        setMessage(editingActiveHost
          ? t('hostSavedDisconnected')
          : t('hostSaved'))
        return
      } catch (error) {
        setMessage(t('saveFailed', String(error)))
        return
      }
    }
    if (tailscaleKeyInputRef.current) tailscaleKeyInputRef.current.value = ''
    if (privateKeyPassphraseRef.current) privateKeyPassphraseRef.current.value = ''
    setHosts((items) => [...items.filter((h) => h.id !== draft.id), draft].sort((a, b) => a.label.localeCompare(b.label)))
    setSelectedId(draft.id)
    setDraft(null)
    setMessage(isDesktop()
      ? t('hostSavedReconnectHint')
      : t('previewConfigKeptInPage'))
  }

  const deleteSelected = async () => {
    if (!selected) return
    // Deleting cascades projects/actions/sessions/tunnels: confirm first.
    if (!window.confirm(t('confirmDeleteWorkstation', selected.label))) return
    if (isDesktop()) {
      try { await deleteHost(selected.id) } catch (error) { setMessage(t('deleteFailed', String(error))); return }
    }
    setHosts((items) => items.filter((h) => h.id !== selected.id))
    setSelectedId(null)
    setMessage(isDesktop() ? t('workstationDeleted') : t('previewRemovedFromPage'))
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
    setStateLabel(t('stateDisconnected'))
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
    setStateLabel(t('stateDisconnected'))
    setMessage(t('disconnectedRemoteSessionsSurvive'))
  }

  const updateDraft = (update: (host: Host) => Host) => setDraft((value) => (value ? update(value) : value))

  const onTmuxAttach = (name: string) => {
    if (!selected) return
    // Session names are interpolated into a shell line; only accept the
    // same character set the server-side tmux_new whitelist enforces so
    // a malicious remote session name cannot inject shell metacharacters.
    if (!/^[A-Za-z0-9_.-]{1,64}$/.test(name)) {
      setMessage(t('tmuxUnsafeName'))
      return
    }
    const paneId = firstPaneId()
    if (paneId === null) {
      setMessage(t('tmuxNeedsTerminal'))
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
      setMessage(t('tmuxCreateFailed', String(error)))
    }
  }

  const onTmuxKill = async (name: string) => {
    if (!selected) return
    try {
      await tmuxKill(selected.id, name)
      setTmuxSessions(await tmuxList(selected.id))
    } catch (error) {
      setMessage(t('tmuxKillFailed', String(error)))
    }
  }

  const onHerdrAttach = () => {
    if (!selected) return
    void herdrAttach(selected.id).catch((error) => setMessage(t('herdrLaunchFailed', String(error))))
  }

  const onHerdrBridge = async () => {
    if (!selected) return
    try {
      const info = await herdrBridge(selected.id, 0)
      setBridgeInfo({ local: info.tunnel.local_addr, socket: info.remote_socket, tunnelId: info.tunnel.id, bridgeId: info.bridge_id })
      setMessage(t('herdrBridged', info.tunnel.local_addr))
    } catch (error) {
      setMessage(t('bridgeFailed', String(error)))
    }
  }

  const onHerdrBridgeStop = async () => {
    if (!selected || !bridgeInfo) return
    try {
      // Close the local tunnel and then the exact SSH-owned BridgeId. The
      // remote port/PID are compatibility metadata, never ownership keys.
      await tunnelClose(bridgeInfo.tunnelId).catch(() => {})
      await herdrBridgeStopById(selected.id, bridgeInfo.bridgeId).catch(() => {})
      setBridgeInfo(null)
      setTunnels(await tunnelList())
      setMessage(t('herdrBridgeStopped'))
    } catch (error) {
      setMessage(t('bridgeStopFailed', String(error)))
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
      setMessage(t('snippetNeedsTerminal'))
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
      setMessage(t('snippetSaveFailed', String(error)))
    }
  }

  const onSnippetDelete = async (id: string) => {
    try {
      await snippetDelete(id)
      setSnippets(await snippetList())
    } catch (error) {
      setMessage(t('snippetDeleteFailed', String(error)))
    }
  }

  // ---- workspace controls ----
  useEffect(() => {
    if (!isDesktop() || !selectedId || phase !== 'ready') return
    void projectList(selectedId).then(setProjects).catch(() => {})
    void runReconcile(selectedId).then(() => runList(undefined, 50, selectedId).then(setRuns)).catch(() => {})
    if (activeTab === 'actions') {
      void runList(undefined, 50, selectedId).then(setRuns).catch(() => {})
    }
  }, [selectedId, activeTab, phase])

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
      setMessage(t('projectSaveFailed', String(error)))
    }
  }

  const onActionSave = async () => {
    if (!actionDraft) return
    try {
      const action = actionDraft.mode === 'Quick'
        ? actionDraft
        : { ...actionDraft, timeout_ms: null }
      await actionSave(action)
      await refreshActions(action.project_id)
      setActionDraft(null)
    } catch (error) {
      setMessage(t('actionSaveFailed', String(error)))
    }
  }

  const onRunAction = async (action: Action) => {
    if (!selected) return
    // The backend independently recomputes the level. Review and Always
    // actions need the same explicit confirmation path as Dangerous actions.
    if (action.confirmation === 'Always' || action.danger_level !== 'Safe') {
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
      const resultMessage = outcome.disposition === 'Completed'
        ? (outcome.exit_code === 0
          ? t('actionCompleted')
          : t('actionFailedExitCode', String(outcome.exit_code ?? 'unknown')))
        : outcome.disposition === 'BackgroundStarted'
          ? t('backgroundTaskStarted')
          : t('commandSentToTerminal')
      setMessage(resultMessage + (outcome.output_bytes > 400 ? t('outputTruncatedSuffix') : ''))
    } catch (error) {
      setMessage(t('actionFailed', String(error)))
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
      setMessage(t('speechUnsupported'))
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
          setMessage(t('speechNeedsTerminal'))
          return
        }
        // Read the CURRENT host at delivery time: the recognition closure
        // must not send to a host the user switched away from mid-speech.
        const target = selectedIdRef.current
        if (!target) return
        void sendInput(target, paneId, inputEncoder.encode(text.trim())).catch(() => {})
        setMessage(t('speechInput', text.trim()))
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
        setMessage(t('fileListFailed', String(error)))
      }
    } finally {
      if (seq === filesSeq.current) setFilesLoading(false)
    }
  }, [selectedId, filesPath, setMessage, t])

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
        setMessage(t('uploadFailed', String(error)))
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
        setMessage(t('pinFailed', String(error)))
        return
      }
    }
    setHosts((items) => items.map((host) => host.id === updated.id ? updated : host))
    setMessage(t('pinnedDefaultDir', filesPath, selected.label))
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
      setMessage(t('downloadFailed', String(error)))
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
      setMessage(t('remotePortRange'))
      return
    }
    try {
      const local = tunnelLocal.trim() === '' ? 0 : Number(tunnelLocal)
      if (Number.isNaN(local) || local < 0 || local > 65535) {
        setMessage(t('localPortRange'))
        return
      }
      const info = await tunnelOpen(selected.id, local, tunnelRemoteHost.trim() || '127.0.0.1', remotePort)
      setTunnels(await tunnelList())
      setTunnelLocal(String(info.local_addr.split(':')[1]))
      setMessage(t('tunnelEstablished', info.local_addr, info.remote_host, String(info.remote_port)))
    } catch (error) {
      setMessage(t('tunnelCreateFailed', String(error)))
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
      setMessage(t('tunnelCloseFailed', String(error)))
    }
  }

  const listeningTunnels = tunnels.filter((tunnel) => tunnel.state === 'Listening')

  return (
    <div className={'shell' + (terminalFocusMode ? ' terminal-focus-mode' : '')}>
      <aside className="sidebar">
        <div className="brand"><span className="brand-mark" /><span className="brand-name">kodework<em>.</em></span><span className="brand-meta">{t('remoteWorkbench')}</span></div>
        <div className="section-label">{t('workstations')} <button aria-label={t('addWorkstationShort')} onClick={() => setDraft(newHost())}><Icon name="plus" size={13} /></button></div>
        {hosts.length === 0 && <div className="empty-nav">{t('noWorkstationsConfigured')}</div>}
        {hosts.map((host) => (
          <button
            className={'nav-row ' + (selectedId === host.id ? 'selected' : '')}
            key={host.id}
            onClick={() => onSelectHost(host)}
          >
            <span className={'dot ' + (phase === 'ready' && selectedId === host.id ? 'online' : 'offline')} />
            <span>{host.label}</span>
            <small>{host.tailscale?.enabled ? 'Tailscale' : t('manualAddress')}</small>
          </button>
        ))}
        <div className="sidebar-footer">
          <div className="connection-pill">
            <span className={'dot ' + (phase === 'ready' ? 'online' : 'offline')} />
            {stateLabel || t('stateDisconnected')} · {phase === 'ready' && selected ? selected.label : t('noSession')}
          </div>
          <div className="sidebar-actions">
            <button className="settings" onClick={() => setSnippetsOpen(true)}><Icon name="zap" size={14} />{t('snippets')}</button>
            <button className="settings" onClick={() => { setSettingsOpen(true); void autostartStatus().then(setAutoStart).catch(() => {}) }}><Icon name="gear" size={14} />{t('settings')}</button>
          </div>
        </div>
      </aside>

      <main className="main">
        <WorkspaceHeader language={language} selected={selected} address={address} phase={phase} onConnect={onConnectClick} onDisconnect={() => { void onDisconnect() }} onDelete={() => { void deleteSelected() }} onTunnel={() => setTunnelPanelOpen(true)} stateLabel={stateLabel || t('stateDisconnected')} onEdit={() => { if (selected) setDraft(structuredClone(selected)) }} />

        <div className="workspace-tabs">
          <button className={activeTab === 'terminal' ? 'active' : ''} onClick={() => setActiveTab('terminal')}><Icon name="terminal" size={13} />{t('terminal')}</button>
          <button className={activeTab === 'local' ? 'active' : ''} onClick={() => setActiveTab('local')}><Icon name="computer" size={13} />{t('local')}</button>
          <button className={activeTab === 'files' ? 'active' : ''} onClick={() => setActiveTab('files')}><Icon name="folder" size={13} />{t('files')}</button>
          <button className={activeTab === 'preview' ? 'active' : ''} onClick={() => setActiveTab('preview')} disabled={listeningTunnels.length === 0 && !previewUrl} title={listeningTunnels.length === 0 && !previewUrl ? t('previewNeedsTunnel') : undefined}><Icon name="globe" size={13} />{t('preview')}</button>
          <button className={activeTab === 'actions' ? 'active' : ''} onClick={() => setActiveTab('actions')}><Icon name="activity" size={13} />{t('activity')}</button>
          <span className="tmux">
            {selected?.tailscale?.enabled ? t('tailscaleConfigured') : t('addressCandidatesPending')} · {selected ? selected.default_runtime.toLowerCase() : t('notConnected')}
          </span>
        </div>

        <div className="main-area">
          {activeTab === 'actions' ? (
            <section className="terminal-card files-card">
              <div className="terminal-head">
                <span>{t('projectsAndActions')}</span>
                <span className="terminal-tools">Quick · Interactive · Background</span>
              </div>
              <div className="files-body">
                {projects.length === 0 && <div className="runtime-empty">{t('noProjectsYet')}</div>}
                {projects.map((project) => (
                  <div key={project.id} style={{ marginBottom: 10 }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '6px 4px' }}>
                      <Icon name="folder" size={12} />
                      <span style={{ fontWeight: 600, fontSize: 13 }}>{project.name}</span>
                      <span className="tunnel-remote">{project.remote_cwd}</span>
                      <span style={{ marginLeft: 'auto', display: 'flex', gap: 6 }}>
                        <button className="mini" onClick={() => setProjectDraft({ ...project })}>{t('edit')}</button>
                        <button className="mini danger" onClick={() => void projectDelete(project.id).then(() => setProjects((items) => items.filter((p) => p.id !== project.id))).catch((e) => setMessage(t('deleteFailed', String(e))))}>{t('delete')}</button>
                        <button className="mini" onClick={() => setActionDraft({ id: crypto.randomUUID(), project_id: project.id, name: '', command: '', mode: 'Quick', cwd: project.remote_cwd, timeout_ms: 60000, danger_level: 'Safe', confirmation: 'OnDangerous', env: {} })}>{t('addAction')}</button>
                      </span>
                    </div>
                    {(actionsByProject[project.id] ?? []).map((action) => (
                      <div className="tunnel-row" key={action.id} style={{ paddingLeft: 20 }}>
                        <span style={{ fontWeight: 600, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{action.name}</span>
                        <span className="tunnel-state" style={action.danger_level === 'Dangerous' ? { color: 'var(--danger)' } : {}}>{action.danger_level}</span>
                        <span className="tunnel-conns">{action.mode}</span>
                        <button className={'mini' + (action.danger_level === 'Dangerous' ? ' danger' : '')} disabled={actionBusy} onClick={() => void onRunAction(action)}>{actionBusy ? <Icon name="power" size={10} /> : <Icon name="play" size={10} />}{actionBusy ? t('runActionInProgress') : t('runAction')}</button>
                        <button className="mini" onClick={() => setActionDraft({ ...action })}>{t('edit')}</button>
                        <button className="mini danger" onClick={() => void actionDelete(action.id).then(() => refreshActions(project.id)).catch((e) => setMessage(t('deleteFailed', String(e))))}><Icon name="trash" size={10} />{t('delete')}</button>
                      </div>
                    ))}
                  </div>
                ))}
                {runResult && (
                  <div style={{ marginTop: 8, padding: 10, background: 'var(--bg-inset)', border: '1px solid var(--line)', borderRadius: 5, fontFamily: 'var(--mono)', fontSize: 11, whiteSpace: 'pre-wrap', maxHeight: 160, overflowY: 'auto' }}>
                    {runResult.stdout_preview || t('noOutput')}
                    {runResult.stderr_preview && '\n[stderr] ' + runResult.stderr_preview}
                  </div>
                )}
                <div style={{ marginTop: 14, borderTop: '1px solid var(--line)', paddingTop: 8 }}>
                  <div className="runtime-title"><span>{t('runHistory')}</span><small>{t('runCount', String(runs.length))}</small></div>
                  {runs.length === 0 ? <div className="runtime-empty">{t('noRunHistory')}</div> : runs.slice(0, 12).map((run) => (
                    <div className="tunnel-row" key={run.id} style={{ paddingLeft: 4 }}>
                      <span className="tunnel-state">{run.status}</span>
                      <span className="tunnel-conns">{t('exitLabel', run.exit_code === null || run.exit_code === undefined ? '—' : String(run.exit_code))}</span>
                      <span className="tunnel-remote">{run.remote_session_ref ?? `${run.output_bytes} bytes`}</span>
                    </div>
                  ))}
                </div>
              </div>
              <div className="files-toolbar" style={{ borderTop: '1px solid var(--line)', borderBottom: 'none' }}>
                <button className="mini" onClick={() => setProjectDraft({ id: crypto.randomUUID(), host_id: selectedId ?? '', name: '', remote_cwd: '~', preferred_runtime: selected?.default_runtime ?? 'Tmux' })}><Icon name="plus" size={11} />{t('newProject')}</button>
                {runResult && <button className="mini" onClick={() => setRunResult(null)}>{t('clearOutput')}</button>}
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
                    <option value="">{t('selectTunnel')}</option>
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
                    <div>{t('previewEmptyHint')}</div>
                  </div>
                )}
              </div>
            </section>
          ) : activeTab === 'files' && !(phase === 'ready' && selected) ? (
            <section className="terminal-card">
              <div className="terminal-empty">
                <div className="empty-symbol"><Icon name="folder" size={40} /></div>
                <h2>{t('filesNeedConnection')}</h2>
                <p>{t('filesSftpHint')}</p>
              </div>
            </section>
          ) : activeTab === 'files' && phase === 'ready' && selected ? (
            <FilesPanel
              t={t}
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
              onYazi={() => { void yaziAttach(selected.id).catch((error) => setMessage(t('yaziLaunchFailed', String(error)))) }}
              onPauseTransfer={(id) => { void sftpPause(selected.id, id).catch(() => {}) }}
              onResumeTransfer={(id) => { void sftpResume(selected.id, id).catch(() => {}) }}
              onCancelTransfer={(id) => { void sftpCancel(selected.id, id).catch(() => {}) }}
              onDismissTransfer={(id) => setTransfers((map) => { const next = { ...map }; delete next[id]; return next })}
            />
          ) : null}
          <TerminalWorkspace
            visible={activeTab === 'terminal'} phase={phase} stateLabel={stateLabel || t('stateDisconnected')} selected={selected}
            panes={panes} splitDir={splitDir} micListening={micListening} runtimeOpen={runtimeOpen}
            focusMode={terminalFocusMode} onMicToggle={onMicToggle} onSplit={(direction) => { void onSplit(direction) }}
            onToggleRuntime={() => setRuntimeOpen((open) => !open)} onToggleFocus={() => setTerminalFocusMode((active) => !active)}
            onClosePane={onClosePane} onPasteStatus={setMessage} language={language}
          />
          <LocalTerminalWorkspace language={language} visible={activeTab === 'local'} onStatus={setMessage} />
          {phase === 'ready' && selected && activeTab === 'terminal' && runtimeOpen && (
            <RuntimePanel
              t={t}
              herdrVersion={herdrVersion} herdrMissing={herdrMissing} herdrError={herdrError} agents={agents}
              bridgeInfo={bridgeInfo ? { local: bridgeInfo.local, socket: bridgeInfo.socket } : null}
              tmuxSessions={tmuxSessions} newTmuxName={newTmuxName} onNewTmuxName={setNewTmuxName}
              onHerdrBridge={() => { void onHerdrBridge() }} onHerdrBridgeStop={() => { void onHerdrBridgeStop() }} onHerdrAttach={onHerdrAttach}
              onTmuxAttach={onTmuxAttach} onTmuxKill={(name) => { void onTmuxKill(name) }} onTmuxCreate={() => { void onTmuxCreate() }}
            />
          )}
        </div>

        <footer className="statusbar">
          <span className="status-security"><Icon name="check" size={12} />{t('localMetadataSafe')}</span>
          <span className={messageIsError ? 'status-message error' : 'status-message'}>{message || t('ready')}</span>
          <span>{isDesktop() ? 'Desktop' : 'Preview'} · v{__APP_VERSION__}</span>
        </footer>
      </main>
      {draft && (
        <HostEditor
          draft={draft} language={language} tailscaleComponents={tailscaleComponents} tailscaleKeyInputRef={tailscaleKeyInputRef}
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
                .catch((error) => setMessage(t('saveCredentialFailed', String(error))))
            } else {
              void runConnect(selected, passwordValue)
            }
          }}>
            <div className="modal-head">
              <div><div className="eyebrow">AUTHENTICATION</div><h2>{t('connectTo', selected.label)}</h2></div>
              <button type="button" className="ghost" onClick={() => {
                if (passwordInputRef.current) passwordInputRef.current.value = ''
                setPromptPassword(false)
                setRememberPassword(false)
                setPhase('idle')
                setStateLabel(t('stateDisconnected'))
              }}>{t('cancel')}</button>
            </div>
            <label>{selected.auth_mode === 'PublicKey' ? t('passphraseLabel') : t('passwordLabel')}
              <input ref={passwordInputRef} type="password" autoComplete="off" autoFocus />
            </label>
            <label className="toggle-row">
              <input type="checkbox" checked={rememberPassword} onChange={(event) => setRememberPassword(event.target.checked)} />
              {t('saveInKeyring', selected.auth_mode === 'PublicKey' ? t('passphraseWord') : t('passwordWord'))}
            </label>
            <p className="modal-note">{t('credentialIpcNote')}</p>
            <button className="primary modal-submit" type="submit">{t('connect')}</button>
          </form>
        </div>
      )}

      {projectDraft && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">PROJECT</div><h2>{t('editProject')}</h2></div>
              <button type="button" className="ghost" onClick={() => setProjectDraft(null)}>{t('close')}</button>
            </div>
            <label>{t('nameLabel')}
              <input value={projectDraft.name} onChange={(e) => setProjectDraft({ ...projectDraft, name: e.target.value })} autoFocus />
            </label>
            <label>{t('remoteDirectory')}
              <input value={projectDraft.remote_cwd} onChange={(e) => setProjectDraft({ ...projectDraft, remote_cwd: e.target.value })} placeholder="~/projects/foo" />
            </label>
            <div className="modal-actions">
              <button className="ghost" onClick={() => setProjectDraft(null)}>{t('cancel')}</button>
              <button className="primary" onClick={() => void onProjectSave()}>{t('save')}</button>
            </div>
          </div>
        </div>
      )}

      {actionDraft && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">ACTION</div><h2>{t('editAction')}</h2></div>
              <button type="button" className="ghost" onClick={() => setActionDraft(null)}>{t('close')}</button>
            </div>
            <label>{t('nameLabel')}
              <input value={actionDraft.name} onChange={(e) => setActionDraft({ ...actionDraft, name: e.target.value })} autoFocus />
            </label>
            <label>{t('commandLabel')}
              <textarea
                rows={3}
                value={actionDraft.command}
                onChange={(e) => setActionDraft({ ...actionDraft, command: e.target.value })}
                style={{ background: 'var(--bg-inset)', border: '1px solid var(--line-strong)', borderRadius: 4, padding: '7px 9px', fontSize: 12, fontFamily: 'var(--mono)', color: 'var(--text)', resize: 'vertical' }}
              />
            </label>
            <div className="form-row">
              <label>{t('modeLabel')}
                <select value={actionDraft.mode} onChange={(e) => {
                  const mode = e.target.value as Action['mode']
                  setActionDraft({ ...actionDraft, mode, timeout_ms: mode === 'Quick' ? actionDraft.timeout_ms : null })
                }}>
                  <option value="Quick">{t('modeQuick')}</option>
                  <option value="Interactive">{t('modeInteractive')}</option>
                  <option value="Background">{t('modeBackground')}</option>
                </select>
              </label>
              <label>{t('dangerLevel')}
                <select value={actionDraft.danger_level} disabled>
                  <option value="Safe">{t('dangerSafe')}</option>
                  <option value="Review">{t('dangerReview')}</option>
                  <option value="Dangerous">{t('dangerDangerous')}</option>
                </select>
                <span className="modal-note">{t('dangerAutoNote')}</span>
              </label>
            </div>
            <label>{t('timeoutLabel')}
              <input
                type="number"
                disabled={actionDraft.mode !== 'Quick'}
                value={actionDraft.mode === 'Quick' ? actionDraft.timeout_ms ?? '' : ''}
                onChange={(e) => setActionDraft({ ...actionDraft, timeout_ms: e.target.value ? Number(e.target.value) : null })}
              />
              {actionDraft.mode !== 'Quick' && <span className="modal-note">{t('noLocalTimeoutNote')}</span>}
            </label>
            <div className="modal-actions">
              <button className="ghost" onClick={() => setActionDraft(null)}>{t('cancel')}</button>
              <button className="primary" onClick={() => void onActionSave()}>{t('save')}</button>
            </div>
          </div>
        </div>
      )}

      {confirmAction && selected && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">CONFIRM</div><h2>{t('confirmRunAction')}</h2></div>
            </div>
            <p className="modal-note">{t('actionWillRun', confirmAction.name, confirmAction.danger_level)}</p>
            <code className="fingerprint">{confirmAction.command}</code>
            <div className="modal-actions">
              <button className="ghost" onClick={() => setConfirmAction(null)}>{t('cancel')}</button>
              <button className="primary" style={{ background: 'var(--danger)' }} onClick={() => { const action = confirmAction; setConfirmAction(null); void executeAction(action, true) }}>{t('confirmExecute')}</button>
            </div>
          </div>
        </div>
      )}

      {snippetsOpen && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">SNIPPETS</div><h2>{t('commandSnippets')}</h2></div>
              <button type="button" className="ghost" onClick={() => setSnippetsOpen(false)}>{t('close')}</button>
            </div>
            {snippetDraft ? (
              <>
              <label>{t('nameLabel')}
                <input value={snippetDraft.name} onChange={(e) => setSnippetDraft({ ...snippetDraft, name: e.target.value })} autoFocus onKeyDown={(e) => { if (e.key === 'Enter') void onSnippetSave() }} />
              </label>
              <label>{t('commandContent')}
                <textarea
                  rows={4}
                  value={snippetDraft.text}
                  onChange={(e) => setSnippetDraft({ ...snippetDraft, text: e.target.value })}
                  style={{ background: 'var(--bg-inset)', border: '1px solid var(--line-strong)', borderRadius: 4, padding: '7px 9px', fontSize: 12, fontFamily: 'var(--mono)', color: 'var(--text)', resize: 'vertical' }}
                />
              </label>
              <div className="modal-actions">
                <button className="ghost" onClick={() => setSnippetDraft(null)}>{t('cancel')}</button>
                <button className="primary" onClick={() => void onSnippetSave()}>{t('save')}</button>
              </div>
              </>
            ) : (
              <div style={{ display: 'flex', flexDirection: 'column', gap: 6, maxHeight: 320, overflowY: 'auto' }}>
                {snippets.length === 0 && <div className="runtime-empty">{t('noSnippetsYet')}</div>}
                {snippets.map((snippet) => (
                  <div className="tunnel-row" key={snippet.id}>
                    <span style={{ fontWeight: 600, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{snippet.name}</span>
                    <span className="tunnel-remote">{t('charCount', String(snippet.text.length))}</span>
                    <button className="mini" onClick={() => onSnippetRun(snippet)}><Icon name="play" size={10} />{t('execute')}</button>
                    <button className="mini" onClick={() => setSnippetDraft({ ...snippet })}><Icon name="gear" size={10} />{t('edit')}</button>
                    <button className="mini danger" onClick={() => void onSnippetDelete(snippet.id)}><Icon name="trash" size={10} />{t('delete')}</button>
                  </div>
                ))}
              </div>
            )}
            {!snippetDraft && (
              <button className="primary modal-submit" onClick={() => setSnippetDraft({ id: crypto.randomUUID(), name: '', text: '', sort_order: 0 })}><Icon name="plus" size={12} />{t('newSnippet')}</button>
            )}
          </div>
        </div>
      )}

      {tunnelPanelOpen && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">PORT FORWARDING</div><h2>{t('sshTunnels')}</h2></div>
              <button type="button" className="ghost" onClick={() => setTunnelPanelOpen(false)}>{t('close')}</button>
            </div>
            <div className="tunnel-form">
              <label>{t('localPort')}<span className="modal-note">{t('autoAssignWhenEmpty')}</span>
                <input value={tunnelLocal} onChange={(e) => setTunnelLocal(e.target.value)} placeholder="0" />
              </label>
              <label>{t('remoteHost')}
                <input value={tunnelRemoteHost} onChange={(e) => setTunnelRemoteHost(e.target.value)} placeholder="127.0.0.1" />
              </label>
              <label>{t('remotePort')}
                <input value={tunnelRemotePort} onChange={(e) => setTunnelRemotePort(e.target.value)} placeholder="3000" />
              </label>
              <button className="primary" onClick={() => void onCreateTunnel()}><Icon name="plus" size={12} />{t('establish')}</button>
            </div>
            <div className="modal-note" style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
              {t('tunnelLoopbackNote')}
            </div>
            {tunnels.length === 0 ? (
              <div className="runtime-empty">{t('noTunnels')}</div>
            ) : tunnels.map((tunnel) => (
              <div className="tunnel-row" key={tunnel.id}>
                <span className={"tunnel-state " + tunnel.state.toLowerCase()}>{tunnel.state}</span>
                <span className="tunnel-local">{tunnel.local_addr}</span>
                <span className="tunnel-remote">→ {tunnel.remote_host}:{tunnel.remote_port}</span>
                <span className="tunnel-conns">{t('connectionCount', String(tunnel.active_connections))}</span>
                {tunnel.state === 'Listening' && (
                  <button className="mini" onClick={() => { setPreviewUrl("http://" + tunnel.local_addr); setActiveTab('preview'); setTunnelPanelOpen(false) }}><Icon name="eye" size={10} />{t('tunnelPreview')}</button>
                )}
                <button className="mini danger" onClick={() => void onCloseTunnel(tunnel.id)}><Icon name="close" size={10} />{t('close')}</button>
              </div>
            ))}
          </div>
        </div>
      )}

      {settingsOpen && (
        <SettingsPanel
          autoStart={autoStart} updateCheck={updateCheck} updateBusy={updateBusy} version={__APP_VERSION__} theme={theme} onThemeChange={onThemeChange} language={language} onLanguageChange={onLanguageChange}
          onClose={() => setSettingsOpen(false)}
          onAutoStart={(enabled) => { setAutoStart(enabled); void setAutostart(enabled).then(setAutoStart).catch((error) => setMessage(t('autostartFailed', String(error)))) }}
          onCheck={() => { setUpdateBusy(true); void checkForUpdates().then(setUpdateCheck).catch((error) => setUpdateCheck({ status: 'error', error: String(error) })).finally(() => setUpdateBusy(false)) }}
          onInstall={() => { setUpdateBusy(true); void installUpdate().then((result) => { if (result.ok) { setUpdateCheck({ status: 'up-to-date' }); setMessage(t('updateInstalled')) } else { setUpdateCheck({ status: 'error', error: result.error ?? t('installFailed') }) } }).finally(() => setUpdateBusy(false)) }}
        />
      )}

      {needsLanguagePrompt && <LanguagePrompt language={language} onChoose={onLanguageChange} />}


      {keyboardInteractiveRequest && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <form className="host-modal" onSubmit={(event) => { event.preventDefault(); void onKeyboardInteractiveAnswer() }}>
            <div className="modal-head">
              <div><div className="eyebrow">KEYBOARD-INTERACTIVE</div><h2>{keyboardInteractiveRequest.name || t('serverAuth')}</h2></div>
            </div>
            {keyboardInteractiveRequest.instructions && <p className="modal-note">{keyboardInteractiveRequest.instructions}</p>}
            {keyboardInteractiveRequest.prompts.map((prompt, index) => (
              <label key={`${keyboardInteractiveRequest.request_id}-${index}`}>{prompt.prompt || t('responseLabel', String(index + 1))}
                <input
                  ref={(element) => { if (element) keyboardInteractiveInputs.current[index] = element }}
                  type={prompt.echo ? 'text' : 'password'}
                  autoComplete="off"
                  spellCheck={false}
                  autoFocus={index === 0}
                />
              </label>
            ))}
            <p className="modal-note">{t('keyboardInteractiveNote')}</p>
            <div className="modal-actions">
              <button className="primary" type="submit">{t('submitAuth')}</button>
            </div>
          </form>
        </div>
      )}

      {hostKeyRequest && (
        <div className="modal-backdrop" role="dialog" aria-modal="true">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">HOST KEY VERIFICATION</div><h2>{t('confirmHostKey')}</h2></div>
            </div>
            <p className="fingerprint-label">{t('hostKeyFingerprint', hostKeyRequest.info.hostname, String(hostKeyRequest.info.port))}</p>
            <code className="fingerprint">{hostKeyRequest.info.fingerprint}</code>
            <p className="modal-note">{t('hostKeyAlgorithmNote', hostKeyRequest.info.algorithm)}</p>
            <div className="modal-actions">
              <button className="ghost" onClick={() => void onHostKeyDecision('reject')}>{t('rejectKey')}</button>
              <button className="ghost" onClick={() => void onHostKeyDecision('trust_once')}>{t('trustOnce')}</button>
              <button className="primary" onClick={() => void onHostKeyDecision('trust_and_save')}>{t('trustAndSave')}</button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
