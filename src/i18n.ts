export type Language = 'zh-CN' | 'en-US'

export const LANGUAGE_STORAGE_KEY = 'kodework.language.v1'

export const translationCatalogs = {
  'zh-CN': {
    settings: '设置', close: '关闭', language: '界面语言', chinese: '简体中文', english: 'English',
    appearance: '外观主题', localOnly: '仅保存在本机界面偏好中', followSystem: '跟随系统', followSystemHint: '根据系统外观自动切换', dark: '黑色', darkHint: '低亮度深色工作台', light: '白色', lightHint: '明亮高对比界面',
    accent: '强调色', autoStart: '登录时自动启动 KodeWork（托盘常驻）', updates: '自动更新', currentVersion: (v: string) => `当前版本 v${v} · 签名更新（tauri-plugin-updater）`, check: '检查更新', checking: '检查中…', install: '下载并安装', latest: '已是最新版本', unsupported: '浏览器预览模式不支持自动更新', found: (v: string) => `发现新版本 v${v}`, checkFailed: (e: string) => `检查失败：${e}`,
    security: '密码仅在连接时一次性输入并即时清零，不写入本机 SQLite 或日志。', updateSecurity: '更新包必须通过 Tauri updater 签名验证；Windows 正式分发还需要 Authenticode。',
    chooseLanguage: '选择界面语言', chooseLanguageHint: '安装完成后首次启动会显示此选择；你也可以稍后在设置中更改。', continue: '继续',
    terminal: '终端', newTerminal: '新建终端', pasteAssets: '粘贴图片/PDF', voice: '语音', splitRight: '左右分屏', splitBelow: '上下分屏', runtime: '运行时', focus: '专注', closeTerminal: '关闭终端', remoteTerminal: '远程终端', localTerminal: '本机终端', refreshLocal: '重新检测本机终端', powershell: 'PowerShell', commandPrompt: '命令提示符', wslDistribution: 'WSL 发行版', chooseWsl: '选择 WSL…', openWsl: '打开 WSL', chooseLocalTerminal: '选择一个本机终端', localTerminalHint: 'PowerShell、CMD 和 WSL 与远程 SSH 会话相互独立，可同时打开多个标签。', loadingLocalRenderer: '正在加载本机终端渲染器…', noWsl: '没有可用的 WSL 发行版。请先安装并初始化 WSL。', localCapabilitiesFailed: (e: string) => `读取本机终端能力失败：${e}`, localOpened: (label: string) => `已打开 ${label}。`, localOpenFailed: (e: string) => `打开本机终端失败：${e}`, localCloseFailed: (e: string) => `关闭本机终端失败：${e}`, closeLabel: (label: string) => `关闭 ${label}`, workspace: '工作区', unconfigured: '未配置', addWorkstation: '先添加一台远程工作站', disconnect: '断开', connect: '连接', connecting: '连接中…', delete: '删除', tunnel: '隧道', edit: '编辑', files: '文件', preview: '预览', activity: '活动', local: '本机', noSession: '无会话', tailscaleConfigured: 'Tailscale 已配置', addressCandidatesPending: '地址候选待配置', notConnected: '未连接', localMetadataSafe: '本地元数据 · 凭据不进入 SQLite；动作环境变量按普通工作区文本保存', ready: '就绪', stateDisconnected: '未连接', stateResolving: '解析地址…', stateConnecting: '连接中…', stateVerifyingHostKey: '验证主机密钥…', stateAuthenticating: '认证中…', stateWaitingForCredential: '等待凭据…', stateReady: '已连接', stateReconnecting: '重连中…', stateFailed: '连接失败', credentialRequired: '连接需要凭据，请输入密码或私钥口令后重试。', connectFailed: (e: string) => `连接失败：${e}`,
    sftpStreamingResume: 'SFTP · 流式传输 · 断点续传', parentDirectory: '上级目录', refresh: '刷新', upload: '上传', download: '下载', updatePinnedDirectory: '更新此工作站打开文件页时的默认目录', pinned: '已固定', returnToPinnedDirectory: (path: string) => `回到固定目录：${path}`, returnToPinned: '回到固定目录', pinCurrentDirectoryHint: '以后打开这台工作站的文件页时自动进入当前目录', pinCurrent: '固定当前', openYazi: '在终端中打开 yazi', loading: '读取中…', emptyDirectory: '空目录', clear: '清除', pause: '暂停', resume: '继续', cancel: '取消', remoteDirectoryItems: (path: string, count: string) => `远程目录 ${path}，${count} 项`,
    notDetected: '未检测到', temporarilyUnavailable: '暂不可用', detecting: '检测中…', bridgeHerdrHint: '把远程 herdr socket 桥接到本地端口（需远程 socat）', bridge: '桥接', startHerdrHint: '在终端中启动 herdr TUI', start: '启动', stop: '停止', remoteHerdrMissing: '远程未检测到 Herdr', herdrAgentsUnavailable: 'Herdr 已检测到，但 agent 列表暂不可用', noRunningAgents: '没有运行中的 agent', sessionCount: (count: string) => `${count} 会话`, noTmuxSessions: '暂无会话；可在下方新建', confirmKillTmux: (name: string) => `确定删除 tmux 会话 ${name}？会话中的进程将终止。`, newSessionName: '新会话名', create: '新建',
    previewMode: 'Preview 模式：浏览器仅作界面预览，配置不会写入磁盘。',
    noWorkstationConfigured: '尚未配置工作站。', loadedWorkstations: (count: string) => `已读取 ${count} 台工作站。`, loadConfigFailed: (error: string) => `读取配置失败：${error}`,
    openTerminalFailed: (error: string) => `打开终端失败：${error}`, splitFailed: (error: string) => `分屏失败：${error}`,
    connectingTo: (label: string) => `正在连接 ${label}…`,
    reconnectAttempting: '连接断开，正在自动重连…', reconnectNeedsPassword: '连接已断开：请输入密码重新连接。', reconnectNeedsCredential: '自动重连需要凭据，请重新输入。',
    keyboardInteractiveExpired: '交互式认证请求已过期，请重新连接。', keyboardInteractiveSubmitFailed: (error: string) => `提交交互式认证失败：${error}`,
    fillRequiredFields: '请填写名称、用户名和至少一个地址。',
    hostAndTailscaleSavedDisconnected: '工作站与 Tailscale 凭据已保存；连接已安全断开，请使用新配置重新连接。', hostAndTailscaleSaved: '工作站与 Tailscale 凭据已保存。',
    hostSavedDisconnected: '工作站配置已保存；连接已安全断开，请使用新配置重新连接。', hostSaved: '工作站配置已保存。',
    hostSavedReconnectHint: '工作站配置已保存；如果刚才正在连接，请使用新配置重新连接。', previewConfigKeptInPage: 'Preview：配置仅保留在当前页面。',
    saveFailed: (error: string) => `保存失败：${error}`,
    confirmDeleteWorkstation: (label: string) => `确定删除工作站 ${label}？项目与动作会删除；已有运行历史会被保留，因此若存在运行历史，删除会被阻止。`,
    deleteFailed: (error: string) => `删除失败：${error}`, workstationDeleted: '工作站已删除。', previewRemovedFromPage: 'Preview：已从当前页面移除。',
    disconnectedRemoteSessionsSurvive: '已断开；远端 tmux/Herdr 会话不受影响。',
    tmuxUnsafeName: 'tmux 会话名包含不安全字符，已拒绝附加。', tmuxNeedsTerminal: '没有打开的终端，无法附加 tmux 会话。',
    tmuxCreateFailed: (error: string) => `tmux 新建失败：${error}`, tmuxKillFailed: (error: string) => `tmux 删除失败：${error}`,
    herdrLaunchFailed: (error: string) => `herdr 启动失败：${error}`,
    herdrBridged: (addr: string) => `herdr socket 已桥接：${addr}`, bridgeFailed: (error: string) => `桥接失败：${error}`,
    herdrBridgeStopped: 'herdr 桥接已停止', bridgeStopFailed: (error: string) => `停止桥接失败：${error}`,
    snippetNeedsTerminal: '没有打开的终端，无法运行片段。',
    snippetSaveFailed: (error: string) => `片段保存失败：${error}`, snippetDeleteFailed: (error: string) => `片段删除失败：${error}`,
    projectSaveFailed: (error: string) => `项目保存失败：${error}`, actionSaveFailed: (error: string) => `动作保存失败：${error}`,
    actionCompleted: '动作已完成', actionFailedExitCode: (code: string) => `动作失败：退出码 ${code}`, backgroundTaskStarted: '后台任务已启动，等待远端完成状态', commandSentToTerminal: '命令已发送到终端', outputTruncatedSuffix: '（输出已截断）', actionFailed: (error: string) => `动作失败：${error}`,
    speechUnsupported: '当前 WebView 不支持语音输入', speechNeedsTerminal: '没有打开的终端，无法输入语音文本。', speechInput: (text: string) => `语音已输入：${text}`,
    fileListFailed: (error: string) => `文件列表失败：${error}`,
    uploadFailed: (error: string) => `上传失败：${error}`, pinFailed: (error: string) => `固定目录失败：${error}`, pinnedDefaultDir: (path: string, label: string) => `已将 ${path} 固定为 ${label} 的默认文件目录。`, downloadFailed: (error: string) => `下载失败：${error}`,
    yaziLaunchFailed: (error: string) => `yazi 启动失败：${error}`,
    remotePortRange: '远程端口需为 1–65535', localPortRange: '本地端口需为 0–65535（0 表示自动分配）', tunnelEstablished: (local: string, host: string, port: number) => `隧道已建立：${local} → ${host}:${port}`, tunnelCreateFailed: (error: string) => `隧道建立失败：${error}`, tunnelCloseFailed: (error: string) => `隧道关闭失败：${error}`,
    remoteWorkbench: '远程工作台', workstations: '工作站', addWorkstationShort: '添加工作站', noWorkstationsConfigured: '还没有工作站配置', snippets: '片段', manualAddress: '手动地址',
    previewNeedsTunnel: '建立隧道后可预览远程 Web 服务', projectsAndActions: '◷ 项目与动作', noProjectsYet: '还没有项目；点击“新建项目”创建', addAction: '＋动作', runAction: '运行', runActionInProgress: '运行中', noOutput: '(无输出)', runHistory: '运行历史', runCount: (count: string) => `${count} 条`, noRunHistory: '暂无运行记录', exitLabel: (code: string) => `退出 ${code}`, newProject: '新建项目', clearOutput: '清除输出',
    selectTunnel: '选择隧道…', previewEmptyHint: '先建立一条隧道，再在这里预览远程 Web 服务', filesNeedConnection: '连接后可用', filesSftpHint: '文件面板通过 SFTP 流式访问远程目录。',
    saveCredentialFailed: (error: string) => `保存凭据失败：${error}`, connectTo: (label: string) => `连接 ${label}`, passphraseLabel: '私钥口令（如无可留空）', passwordLabel: '密码', saveInKeyring: (kind: string) => `使用当前操作系统的安全密钥环保存${kind}`, passphraseWord: '口令', passwordWord: '密码', credentialIpcNote: '凭据仅经 IPC 传递到本地 Rust 进程，不写入数据库或日志。',
    editProject: '编辑项目', nameLabel: '名称', remoteDirectory: '远程目录', save: '保存',
    editAction: '编辑动作', commandLabel: '命令', modeLabel: '模式', modeQuick: 'Quick（快速）', modeInteractive: 'Interactive（终端）', modeBackground: 'Background（后台）', dangerLevel: '危险级别', dangerSafe: '安全', dangerReview: '需复核', dangerDangerous: '危险', dangerAutoNote: '保存时按命令内容自动判定（服务端强制），不可手动设置。', timeoutLabel: '超时（毫秒，仅 Quick；留空默认 30s）', noLocalTimeoutNote: 'Interactive/Background 没有本地可观测的超时边界。',
    confirmRunAction: '确认运行动作', actionWillRun: (name: string, level: string) => `动作 ${name}（${level}）将在远程执行：`, confirmExecute: '确认执行',
    commandSnippets: '命令片段', commandContent: '命令内容', noSnippetsYet: '还没有片段；点击下方“新建”添加', charCount: (count: string) => `${count} 字符`, execute: '执行', newSnippet: '新建片段',
    sshTunnels: 'SSH 隧道', localPort: '本地端口', autoAssignWhenEmpty: '留空自动分配', remoteHost: '远程主机', remotePort: '远程端口', establish: '建立', tunnelLoopbackNote: '仅监听 127.0.0.1，不暴露到局域网；断开连接时隧道自动失效。', noTunnels: '还没有隧道', connectionCount: (count: string) => `${count} 连接`, tunnelPreview: '预览',
    autostartFailed: (error: string) => `自启设置失败：${error}`, updateInstalled: '更新已安装，重启应用后生效。', installFailed: '安装失败',
    serverAuth: '服务器认证', responseLabel: (index: string) => `响应 ${index}`, keyboardInteractiveNote: '响应不会写入 SQLite、日志或工作区快照；请求超过两分钟自动失效。', submitAuth: '提交认证',
    confirmHostKey: '确认主机密钥', hostKeyFingerprint: (host: string, port: number) => `服务器 ${host}:${port} 的主机密钥指纹：`, hostKeyAlgorithmNote: (algorithm: string) => `算法 ${algorithm}。请与服务器所有者核对该指纹；密钥变化将被硬性阻止。`, rejectKey: '拒绝', trustOnce: '本次信任', trustAndSave: '信任并保存',
  },
  'en-US': {
    settings: 'Settings', close: 'Close', language: 'Interface language', chinese: '简体中文', english: 'English',
    appearance: 'Appearance', localOnly: 'Stored only in this device’s UI preferences', followSystem: 'System', followSystemHint: 'Follow the operating system appearance', dark: 'Dark', darkHint: 'Low-glare dark workbench', light: 'Light', lightHint: 'Bright, high-contrast interface',
    accent: 'Accent color', autoStart: 'Start KodeWork on sign-in (keep it in the tray)', updates: 'Updates', currentVersion: (v: string) => `Current version v${v} · signed updates (tauri-plugin-updater)`, check: 'Check for updates', checking: 'Checking…', install: 'Download and install', latest: 'You are up to date', unsupported: 'Automatic updates are unavailable in browser preview mode', found: (v: string) => `New version v${v} is available`, checkFailed: (e: string) => `Check failed: ${e}`,
    security: 'Passwords are entered once during connection and cleared immediately; they are not written to SQLite or logs.', updateSecurity: 'Update packages must pass Tauri updater signature verification; Windows production distribution also needs Authenticode.',
    chooseLanguage: 'Choose interface language', chooseLanguageHint: 'This appears on first launch after installation. You can change it later in Settings.', continue: 'Continue',
    terminal: 'Terminal', newTerminal: 'New terminal', pasteAssets: 'Paste image/PDF', voice: 'Voice', splitRight: 'Split right', splitBelow: 'Split below', runtime: 'Runtime', focus: 'Focus', closeTerminal: 'Close terminal', remoteTerminal: 'Remote terminal', localTerminal: 'Local terminal', refreshLocal: 'Refresh local terminals', powershell: 'PowerShell', commandPrompt: 'Command Prompt', wslDistribution: 'WSL distribution', chooseWsl: 'Choose WSL…', openWsl: 'Open WSL', chooseLocalTerminal: 'Choose a local terminal', localTerminalHint: 'PowerShell, Command Prompt, and WSL sessions are independent from remote SSH sessions and can run in parallel.', loadingLocalRenderer: 'Loading local terminal renderer…', noWsl: 'No WSL distribution is available. Install and initialize WSL first.', localCapabilitiesFailed: (e: string) => `Failed to read local terminal capabilities: ${e}`, localOpened: (label: string) => `Opened ${label}.`, localOpenFailed: (e: string) => `Failed to open local terminal: ${e}`, localCloseFailed: (e: string) => `Failed to close local terminal: ${e}`, closeLabel: (label: string) => `Close ${label}`, workspace: 'Workspace', unconfigured: 'Not configured', addWorkstation: 'Add a remote workstation first', disconnect: 'Disconnect', connect: 'Connect', connecting: 'Connecting…', delete: 'Delete', tunnel: 'Tunnel', edit: 'Edit', files: 'Files', preview: 'Preview', activity: 'Activity', local: 'Local', noSession: 'No session', tailscaleConfigured: 'Tailscale configured', addressCandidatesPending: 'Address candidates pending', notConnected: 'Not connected', localMetadataSafe: 'Local metadata · credentials are not stored in SQLite; action environment values are ordinary workspace text', ready: 'Ready', stateDisconnected: 'Not connected', stateResolving: 'Resolving address…', stateConnecting: 'Connecting…', stateVerifyingHostKey: 'Verifying host key…', stateAuthenticating: 'Authenticating…', stateWaitingForCredential: 'Waiting for credentials…', stateReady: 'Connected', stateReconnecting: 'Reconnecting…', stateFailed: 'Connection failed', credentialRequired: 'Credentials are required. Enter a password or private-key passphrase and try again.', connectFailed: (e: string) => `Connection failed: ${e}`,
    sftpStreamingResume: 'SFTP · streaming · resumable transfers', parentDirectory: 'Parent directory', refresh: 'Refresh', upload: 'Upload', download: 'Download', updatePinnedDirectory: 'Update this workstation’s default Files directory', pinned: 'Pinned', returnToPinnedDirectory: (path: string) => `Return to pinned directory: ${path}`, returnToPinned: 'Pinned directory', pinCurrentDirectoryHint: 'Open this directory automatically when Files is opened for this workstation', pinCurrent: 'Pin current', openYazi: 'Open yazi in the terminal', loading: 'Loading…', emptyDirectory: 'Empty directory', clear: 'Clear', pause: 'Pause', resume: 'Resume', cancel: 'Cancel', remoteDirectoryItems: (path: string, count: string) => `Remote directory ${path}, ${count} items`,
    notDetected: 'Not detected', temporarilyUnavailable: 'Temporarily unavailable', detecting: 'Detecting…', bridgeHerdrHint: 'Bridge the remote herdr socket to a local port (remote socat required)', bridge: 'Bridge', startHerdrHint: 'Start the herdr TUI in the terminal', start: 'Start', stop: 'Stop', remoteHerdrMissing: 'Herdr was not detected remotely', herdrAgentsUnavailable: 'Herdr is installed, but the agent list is temporarily unavailable', noRunningAgents: 'No agents are running', sessionCount: (count: string) => `${count} sessions`, noTmuxSessions: 'No sessions; create one below', confirmKillTmux: (name: string) => `Delete tmux session ${name}? Processes in the session will be terminated.`, newSessionName: 'New session name', create: 'Create',
    previewMode: 'Preview mode: the browser only shows the interface; configuration is not written to disk.',
    noWorkstationConfigured: 'No workstation configured yet.', loadedWorkstations: (count: string) => `Loaded ${count} workstation(s).`, loadConfigFailed: (error: string) => `Failed to load configuration: ${error}`,
    openTerminalFailed: (error: string) => `Failed to open terminal: ${error}`, splitFailed: (error: string) => `Failed to split pane: ${error}`,
    connectingTo: (label: string) => `Connecting to ${label}…`,
    reconnectAttempting: 'Connection lost; reconnecting automatically…', reconnectNeedsPassword: 'Connection lost: enter the password to reconnect.', reconnectNeedsCredential: 'Automatic reconnect needs credentials; please enter them again.',
    keyboardInteractiveExpired: 'The keyboard-interactive request expired; please reconnect.', keyboardInteractiveSubmitFailed: (error: string) => `Failed to submit keyboard-interactive response: ${error}`,
    fillRequiredFields: 'Please fill in the name, username, and at least one address.',
    hostAndTailscaleSavedDisconnected: 'Workstation and Tailscale credentials saved; the connection was closed safely. Reconnect with the new configuration.', hostAndTailscaleSaved: 'Workstation and Tailscale credentials saved.',
    hostSavedDisconnected: 'Workstation configuration saved; the connection was closed safely. Reconnect with the new configuration.', hostSaved: 'Workstation configuration saved.',
    hostSavedReconnectHint: 'Workstation configuration saved; if a connection was in progress, reconnect with the new configuration.', previewConfigKeptInPage: 'Preview: the configuration is kept only in this page.',
    saveFailed: (error: string) => `Save failed: ${error}`,
    confirmDeleteWorkstation: (label: string) => `Delete workstation ${label}? Projects and actions will be deleted. Existing run history is kept, so deletion is blocked while run history exists.`,
    deleteFailed: (error: string) => `Delete failed: ${error}`, workstationDeleted: 'Workstation deleted.', previewRemovedFromPage: 'Preview: removed from this page.',
    disconnectedRemoteSessionsSurvive: 'Disconnected; remote tmux/Herdr sessions are unaffected.',
    tmuxUnsafeName: 'The tmux session name contains unsafe characters; attach was rejected.', tmuxNeedsTerminal: 'No open terminal; cannot attach to a tmux session.',
    tmuxCreateFailed: (error: string) => `Failed to create tmux session: ${error}`, tmuxKillFailed: (error: string) => `Failed to delete tmux session: ${error}`,
    herdrLaunchFailed: (error: string) => `Failed to launch herdr: ${error}`,
    herdrBridged: (addr: string) => `Herdr socket bridged: ${addr}`, bridgeFailed: (error: string) => `Bridge failed: ${error}`,
    herdrBridgeStopped: 'Herdr bridge stopped', bridgeStopFailed: (error: string) => `Failed to stop bridge: ${error}`,
    snippetNeedsTerminal: 'No open terminal; cannot run a snippet.',
    snippetSaveFailed: (error: string) => `Failed to save snippet: ${error}`, snippetDeleteFailed: (error: string) => `Failed to delete snippet: ${error}`,
    projectSaveFailed: (error: string) => `Failed to save project: ${error}`, actionSaveFailed: (error: string) => `Failed to save action: ${error}`,
    actionCompleted: 'Action completed', actionFailedExitCode: (code: string) => `Action failed: exit code ${code}`, backgroundTaskStarted: 'Background task started; waiting for the remote completion state', commandSentToTerminal: 'Command sent to the terminal', outputTruncatedSuffix: ' (output truncated)', actionFailed: (error: string) => `Action failed: ${error}`,
    speechUnsupported: 'Speech input is not supported in this WebView', speechNeedsTerminal: 'No open terminal; cannot type the speech text.', speechInput: (text: string) => `Speech input: ${text}`,
    fileListFailed: (error: string) => `Failed to list files: ${error}`,
    uploadFailed: (error: string) => `Upload failed: ${error}`, pinFailed: (error: string) => `Failed to pin directory: ${error}`, pinnedDefaultDir: (path: string, label: string) => `Pinned ${path} as the default files directory for ${label}.`, downloadFailed: (error: string) => `Download failed: ${error}`,
    yaziLaunchFailed: (error: string) => `Failed to launch yazi: ${error}`,
    remotePortRange: 'Remote port must be 1–65535', localPortRange: 'Local port must be 0–65535 (0 auto-assigns)', tunnelEstablished: (local: string, host: string, port: number) => `Tunnel established: ${local} → ${host}:${port}`, tunnelCreateFailed: (error: string) => `Failed to establish tunnel: ${error}`, tunnelCloseFailed: (error: string) => `Failed to close tunnel: ${error}`,
    remoteWorkbench: 'Remote workbench', workstations: 'Workstations', addWorkstationShort: 'Add workstation', noWorkstationsConfigured: 'No workstations configured', snippets: 'Snippets', manualAddress: 'Manual address',
    previewNeedsTunnel: 'Establish a tunnel to preview a remote web service', projectsAndActions: '◷ Projects & Actions', noProjectsYet: 'No projects yet; click “New project” to create one', addAction: '＋Action', runAction: 'Run', runActionInProgress: 'Running', noOutput: '(no output)', runHistory: 'Run history', runCount: (count: string) => `${count} entries`, noRunHistory: 'No run history yet', exitLabel: (code: string) => `exit ${code}`, newProject: 'New project', clearOutput: 'Clear output',
    selectTunnel: 'Select tunnel…', previewEmptyHint: 'Establish a tunnel first, then preview the remote web service here', filesNeedConnection: 'Available after connecting', filesSftpHint: 'The Files panel streams remote directories over SFTP.',
    saveCredentialFailed: (error: string) => `Failed to save credentials: ${error}`, connectTo: (label: string) => `Connect ${label}`, passphraseLabel: 'Private-key passphrase (leave empty if none)', passwordLabel: 'Password', saveInKeyring: (kind: string) => `Save the ${kind} in the operating system's secure keyring`, passphraseWord: 'passphrase', passwordWord: 'password', credentialIpcNote: 'Credentials travel only over IPC to the local Rust process; they are not written to the database or logs.',
    editProject: 'Edit project', nameLabel: 'Name', remoteDirectory: 'Remote directory', save: 'Save',
    editAction: 'Edit action', commandLabel: 'Command', modeLabel: 'Mode', modeQuick: 'Quick', modeInteractive: 'Interactive (terminal)', modeBackground: 'Background', dangerLevel: 'Danger level', dangerSafe: 'Safe', dangerReview: 'Review', dangerDangerous: 'Dangerous', dangerAutoNote: 'Determined automatically from the command content when saving (server-enforced); it cannot be set manually.', timeoutLabel: 'Timeout (milliseconds; Quick only; empty uses the 30s default)', noLocalTimeoutNote: 'Interactive/Background actions have no locally observable timeout boundary.',
    confirmRunAction: 'Confirm running action', actionWillRun: (name: string, level: string) => `Action ${name} (${level}) will run on the remote host:`, confirmExecute: 'Confirm & run',
    commandSnippets: 'Command snippets', commandContent: 'Command content', noSnippetsYet: 'No snippets yet; click “New” below to add one', charCount: (count: string) => `${count} characters`, execute: 'Execute', newSnippet: 'New snippet',
    sshTunnels: 'SSH tunnels', localPort: 'Local port', autoAssignWhenEmpty: 'Auto-assigned when empty', remoteHost: 'Remote host', remotePort: 'Remote port', establish: 'Establish', tunnelLoopbackNote: 'Listens on 127.0.0.1 only and is never exposed to the LAN; tunnels expire automatically on disconnect.', noTunnels: 'No tunnels yet', connectionCount: (count: string) => `${count} connections`, tunnelPreview: 'Preview',
    autostartFailed: (error: string) => `Failed to change autostart: ${error}`, updateInstalled: 'Update installed; it takes effect after restarting the app.', installFailed: 'Install failed',
    serverAuth: 'Server authentication', responseLabel: (index: string) => `Response ${index}`, keyboardInteractiveNote: 'Responses are not written to SQLite, logs, or workspace snapshots; requests expire after two minutes.', submitAuth: 'Submit authentication',
    confirmHostKey: 'Confirm host key', hostKeyFingerprint: (host: string, port: number) => `Host-key fingerprint of server ${host}:${port}:`, hostKeyAlgorithmNote: (algorithm: string) => `Algorithm ${algorithm}. Verify this fingerprint with the server owner; a changed key is blocked as a hard failure.`, rejectKey: 'Reject', trustOnce: 'Trust this time', trustAndSave: 'Trust & save',
  },
} as const

export type TranslationKey = keyof typeof translationCatalogs['zh-CN']
export type Translator = (key: TranslationKey, ...args: string[]) => string

export function translate(language: Language, key: TranslationKey, ...args: string[]): string {
  const value = (translationCatalogs[language] as Record<string, unknown>)[key] ?? key
  return typeof value === 'function' ? (value as (...values: string[]) => string)(...args) : String(value)
}

export function detectInitialLanguage(): Language {
  try {
    const stored = window.localStorage.getItem(LANGUAGE_STORAGE_KEY)
    if (stored === 'zh-CN' || stored === 'en-US') return stored
  } catch { /* use browser locale fallback */ }
  return navigator.language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US'
}

export function hasSavedLanguage(): boolean {
  try { return window.localStorage.getItem(LANGUAGE_STORAGE_KEY) === 'zh-CN' || window.localStorage.getItem(LANGUAGE_STORAGE_KEY) === 'en-US' } catch { return false }
}

export function saveLanguage(language: Language): void {
  try { window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language) } catch { /* current session still works */ }
}
