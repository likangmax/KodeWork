(() => {
  try {
    const raw = localStorage.getItem('kodework.theme.v1')
    const value = raw ? JSON.parse(raw) : {}
    const mode = ['dark', 'light', 'system'].includes(value.mode) ? value.mode : 'dark'
    const accent = ['amber', 'blue', 'green', 'purple', 'rose', 'cyan'].includes(value.accent) ? value.accent : 'amber'
    const resolved = mode === 'system' ? (matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark') : mode
    document.documentElement.dataset.theme = resolved
    document.documentElement.dataset.accent = accent
    document.documentElement.style.colorScheme = resolved
  } catch {
    document.documentElement.dataset.theme = 'dark'
    document.documentElement.dataset.accent = 'amber'
  }
})()
