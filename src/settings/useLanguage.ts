import { useCallback, useState } from 'react'
import { detectInitialLanguage, hasSavedLanguage, saveLanguage, type Language } from '../i18n'

export function useLanguage(): [Language, (next: Language) => void, boolean] {
  const [language, setLanguage] = useState<Language>(() => detectInitialLanguage())
  const [needsPrompt, setNeedsPrompt] = useState(() => !hasSavedLanguage())
  const update = useCallback((next: Language) => {
    setLanguage(next)
    saveLanguage(next)
    setNeedsPrompt(false)
  }, [])
  return [language, update, needsPrompt]
}