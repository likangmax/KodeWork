export type ThemeMode = 'dark' | 'light' | 'system'
export type AccentColor = 'amber' | 'blue' | 'green' | 'purple' | 'rose' | 'cyan'

export type ThemePreference = {
  mode: ThemeMode
  accent: AccentColor
}

export const THEME_STORAGE_KEY = 'kodework.theme.v1'
export const THEME_CHANGE_EVENT = 'kodework-theme-change'

const DEFAULT_PREFERENCE: ThemePreference = { mode: 'dark', accent: 'amber' }
const validModes: ThemeMode[] = ['dark', 'light', 'system']
const validAccents: AccentColor[] = ['amber', 'blue', 'green', 'purple', 'rose', 'cyan']

export function loadThemePreference(): ThemePreference {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(THEME_STORAGE_KEY) ?? '') as Partial<ThemePreference>
    return {
      mode: validModes.includes(parsed.mode as ThemeMode) ? parsed.mode as ThemeMode : DEFAULT_PREFERENCE.mode,
      accent: validAccents.includes(parsed.accent as AccentColor) ? parsed.accent as AccentColor : DEFAULT_PREFERENCE.accent,
    }
  } catch {
    return DEFAULT_PREFERENCE
  }
}

export function systemTheme(): Exclude<ThemeMode, 'system'> {
  return window.matchMedia?.('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
}

export function resolvedTheme(preference: ThemePreference): Exclude<ThemeMode, 'system'> {
  return preference.mode === 'system' ? systemTheme() : preference.mode
}

export function applyTheme(preference: ThemePreference): void {
  const root = document.documentElement
  const resolved = resolvedTheme(preference)
  root.dataset.theme = resolved
  root.dataset.accent = preference.accent
  root.style.colorScheme = resolved
  window.dispatchEvent(new CustomEvent(THEME_CHANGE_EVENT, { detail: { preference, resolved } }))
}

export function saveThemePreference(preference: ThemePreference): void {
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, JSON.stringify(preference))
  } catch {
    // A locked-down WebView can deny persistent storage. Theme switching
    // must remain usable for the current process even in that environment.
  }
  applyTheme(preference)
}

export function terminalTheme(): {
  background: string; foreground: string; cursor: string; selectionBackground: string
  black: string; red: string; green: string; yellow: string; blue: string; magenta: string; cyan: string; white: string
  brightBlack: string; brightRed: string; brightGreen: string; brightYellow: string; brightBlue: string; brightMagenta: string; brightCyan: string; brightWhite: string
} {
  const light = document.documentElement.dataset.theme === 'light'
  const accent = document.documentElement.dataset.accent ?? 'amber'
  const cursor = ({ amber: '#d58b16', blue: '#2878c8', green: '#238a58', purple: '#8254c7', rose: '#c34d76', cyan: '#168fa3' } as Record<string, string>)[accent] ?? '#d58b16'
  return light ? {
    background: '#f7f8fa', foreground: '#20252b', cursor, selectionBackground: '#c7ddf4',
    black: '#20252b', red: '#b42318', green: '#137333', yellow: '#8a5a00', blue: '#155eef', magenta: '#7a3e9d', cyan: '#087f8c', white: '#f7f8fa',
    brightBlack: '#59636e', brightRed: '#d92d20', brightGreen: '#1a9b52', brightYellow: '#a66a00', brightBlue: '#2f80ed', brightMagenta: '#9b51e0', brightCyan: '#149eca', brightWhite: '#ffffff',
  } : {
    background: '#0c0e10', foreground: '#d6dee3', cursor, selectionBackground: '#2c3a40',
    black: '#0c0e10', red: '#e06c75', green: '#98c379', yellow: '#e5c07b', blue: '#61afef', magenta: '#c678dd', cyan: '#56b6c2', white: '#abb2bf',
    brightBlack: '#5c6370', brightRed: '#e06c75', brightGreen: '#98c379', brightYellow: '#e5c07b', brightBlue: '#61afef', brightMagenta: '#c678dd', brightCyan: '#56b6c2', brightWhite: '#ffffff',
  }
}
