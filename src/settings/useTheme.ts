import { useCallback, useEffect, useState } from 'react'
import { applyTheme, loadThemePreference, saveThemePreference, type ThemePreference } from '../theme'

export function useTheme(): [ThemePreference, (next: ThemePreference) => void] {
  const [preference, setPreference] = useState<ThemePreference>(() => loadThemePreference())

  useEffect(() => {
    saveThemePreference(preference)
    const media = window.matchMedia?.('(prefers-color-scheme: light)')
    if (!media || preference.mode !== 'system') return
    const onChange = () => applyTheme(preference)
    media.addEventListener?.('change', onChange)
    return () => media.removeEventListener?.('change', onChange)
  }, [preference])

  const update = useCallback((next: ThemePreference) => {
    setPreference(next)
  }, [])
  return [preference, update]
}
