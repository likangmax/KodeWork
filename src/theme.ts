export type ThemeMode = 'dark' | 'light' | 'system'
export type AccentColor = 'amber' | 'blue' | 'green' | 'purple' | 'rose' | 'cyan'

export type ThemePreference = {
  mode: ThemeMode
  accent: AccentColor
}

export const THEME_STORAGE_KEY = 'kodework.theme.v1'
export const THEME_CHANGE_EVENT = 'kodework-theme-change'

const DEFAULT_PREFERENCE: ThemePreference = { mode: 'dark', accent: 'blue' }
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
  const cursor = ({ amber: '#e3b341', blue: '#4aa3ff', green: '#3fb96f', purple: '#a371f7', rose: '#f778ba', cyan: '#22b8cf' } as Record<string, string>)[accent] ?? '#4aa3ff'
  return light ? {
    background: '#f8fafc', foreground: '#1c2430', cursor, selectionBackground: '#cfe3f7',
    black: '#1c2430', red: '#c23c32', green: '#1f8a50', yellow: '#a2670a', blue: '#2b6cb0', magenta: '#8a4fae', cyan: '#0f8296', white: '#f8fafc',
    brightBlack: '#5a6a7d', brightRed: '#dd5045', brightGreen: '#2ba35f', brightYellow: '#c07f14', brightBlue: '#3f8fe0', brightMagenta: '#a56ad0', brightCyan: '#18a3b8', brightWhite: '#ffffff',
  } : {
    background: '#0d1117', foreground: '#dbe4ee', cursor, selectionBackground: '#25354a',
    black: '#0d1117', red: '#e5534b', green: '#3fb96f', yellow: '#d9a52a', blue: '#4aa3ff', magenta: '#a371f7', cyan: '#39c5cf', white: '#b1bac4',
    brightBlack: '#6e7681', brightRed: '#f47067', brightGreen: '#5cc98a', brightYellow: '#e3b341', brightBlue: '#6db8ff', brightMagenta: '#b78cfa', brightCyan: '#5ad4dd', brightWhite: '#ffffff',
  }
}
