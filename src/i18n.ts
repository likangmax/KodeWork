export type Language = 'zh-CN' | 'en-US'

export const LANGUAGE_STORAGE_KEY = 'kodework.language.v1'

const translations = {
  'zh-CN': {
    settings: '设置', close: '关闭', language: '界面语言', chinese: '简体中文', english: 'English',
    appearance: '外观主题', localOnly: '仅保存在本机界面偏好中', followSystem: '跟随系统', followSystemHint: '根据系统外观自动切换', dark: '黑色', darkHint: '低亮度深色工作台', light: '白色', lightHint: '明亮高对比界面',
    accent: '强调色', autoStart: '登录时自动启动 KodeWork（托盘常驻）', updates: '自动更新', currentVersion: (v: string) => `当前版本 v${v} · 签名更新（tauri-plugin-updater）`, check: '检查更新', checking: '检查中…', install: '下载并安装', latest: '已是最新版本', unsupported: '浏览器预览模式不支持自动更新', found: (v: string) => `发现新版本 v${v}`, checkFailed: (e: string) => `检查失败：${e}`,
    security: '密码仅在连接时一次性输入并即时清零，不写入本机 SQLite 或日志。', updateSecurity: '更新包必须通过 Tauri updater 签名验证；Windows 正式分发还需要 Authenticode。',
    chooseLanguage: '选择界面语言', chooseLanguageHint: '安装完成后首次启动会显示此选择；你也可以稍后在设置中更改。', continue: '继续',
    terminal: '终端', newTerminal: '新建终端', pasteAssets: '粘贴图片/PDF', voice: '语音', splitRight: '左右分屏', splitBelow: '上下分屏', runtime: '运行时', focus: '专注', closeTerminal: '关闭终端', remoteTerminal: '远程终端', localTerminal: '本机终端', refreshLocal: '重新检测本机终端', powershell: 'PowerShell', commandPrompt: '命令提示符', wslDistribution: 'WSL 发行版', chooseWsl: '选择 WSL…', openWsl: '打开 WSL', chooseLocalTerminal: '选择一个本机终端', localTerminalHint: 'PowerShell、CMD 和 WSL 与远程 SSH 会话相互独立，可同时打开多个标签。', loadingLocalRenderer: '正在加载本机终端渲染器…', noWsl: '没有可用的 WSL 发行版。请先安装并初始化 WSL。', localCapabilitiesFailed: (e: string) => `读取本机终端能力失败：${e}`, localOpened: (label: string) => `已打开 ${label}。`, localOpenFailed: (e: string) => `打开本机终端失败：${e}`, localCloseFailed: (e: string) => `关闭本机终端失败：${e}`, closeLabel: (label: string) => `关闭 ${label}`, workspace: '工作区', unconfigured: '未配置', addWorkstation: '先添加一台远程工作站', disconnect: '断开', connect: '连接', connecting: '连接中…', delete: '删除', tunnel: '隧道', edit: '编辑', files: '文件', preview: '预览', activity: '活动', local: '本机', noSession: '无会话', tailscaleConfigured: 'Tailscale 已配置', addressCandidatesPending: '地址候选待配置', notConnected: '未连接', localMetadataSafe: '本地元数据 · 凭据不进入 SQLite；动作环境变量按普通工作区文本保存', ready: '就绪', stateDisconnected: '未连接', stateResolving: '解析地址…', stateConnecting: '连接中…', stateVerifyingHostKey: '验证主机密钥…', stateAuthenticating: '认证中…', stateReady: '已连接', stateReconnecting: '重连中…', stateFailed: '连接失败',
  },
  'en-US': {
    settings: 'Settings', close: 'Close', language: 'Interface language', chinese: '简体中文', english: 'English',
    appearance: 'Appearance', localOnly: 'Stored only in this device’s UI preferences', followSystem: 'System', followSystemHint: 'Follow the operating system appearance', dark: 'Dark', darkHint: 'Low-glare dark workbench', light: 'Light', lightHint: 'Bright, high-contrast interface',
    accent: 'Accent color', autoStart: 'Start KodeWork on sign-in (keep it in the tray)', updates: 'Updates', currentVersion: (v: string) => `Current version v${v} · signed updates (tauri-plugin-updater)`, check: 'Check for updates', checking: 'Checking…', install: 'Download and install', latest: 'You are up to date', unsupported: 'Automatic updates are unavailable in browser preview mode', found: (v: string) => `New version v${v} is available`, checkFailed: (e: string) => `Check failed: ${e}`,
    security: 'Passwords are entered once during connection and cleared immediately; they are not written to SQLite or logs.', updateSecurity: 'Update packages must pass Tauri updater signature verification; Windows production distribution also needs Authenticode.',
    chooseLanguage: 'Choose interface language', chooseLanguageHint: 'This appears on first launch after installation. You can change it later in Settings.', continue: 'Continue',
    terminal: 'Terminal', newTerminal: 'New terminal', pasteAssets: 'Paste image/PDF', voice: 'Voice', splitRight: 'Split right', splitBelow: 'Split below', runtime: 'Runtime', focus: 'Focus', closeTerminal: 'Close terminal', remoteTerminal: 'Remote terminal', localTerminal: 'Local terminal', refreshLocal: 'Refresh local terminals', powershell: 'PowerShell', commandPrompt: 'Command Prompt', wslDistribution: 'WSL distribution', chooseWsl: 'Choose WSL…', openWsl: 'Open WSL', chooseLocalTerminal: 'Choose a local terminal', localTerminalHint: 'PowerShell, Command Prompt, and WSL sessions are independent from remote SSH sessions and can run in parallel.', loadingLocalRenderer: 'Loading local terminal renderer…', noWsl: 'No WSL distribution is available. Install and initialize WSL first.', localCapabilitiesFailed: (e: string) => `Failed to read local terminal capabilities: ${e}`, localOpened: (label: string) => `Opened ${label}.`, localOpenFailed: (e: string) => `Failed to open local terminal: ${e}`, localCloseFailed: (e: string) => `Failed to close local terminal: ${e}`, closeLabel: (label: string) => `Close ${label}`, workspace: 'Workspace', unconfigured: 'Not configured', addWorkstation: 'Add a remote workstation first', disconnect: 'Disconnect', connect: 'Connect', connecting: 'Connecting…', delete: 'Delete', tunnel: 'Tunnel', edit: 'Edit', files: 'Files', preview: 'Preview', activity: 'Activity', local: 'Local', noSession: 'No session', tailscaleConfigured: 'Tailscale configured', addressCandidatesPending: 'Address candidates pending', notConnected: 'Not connected', localMetadataSafe: 'Local metadata · credentials are not stored in SQLite; action environment values are ordinary workspace text', ready: 'Ready', stateDisconnected: 'Not connected', stateResolving: 'Resolving address…', stateConnecting: 'Connecting…', stateVerifyingHostKey: 'Verifying host key…', stateAuthenticating: 'Authenticating…', stateReady: 'Connected', stateReconnecting: 'Reconnecting…', stateFailed: 'Connection failed',
  },
} as const

export type TranslationKey = keyof typeof translations['zh-CN']
export type Translator = (key: TranslationKey, ...args: string[]) => string

export function translate(language: Language, key: TranslationKey, ...args: string[]): string {
  const value = translations[language][key]
  return typeof value === 'function' ? (value as (...values: string[]) => string)(...args) : value
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
