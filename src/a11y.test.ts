import { describe, expect, it } from 'vitest'

describe('modal dialog accessibility', () => {
  it('ensures every role="dialog" element has an aria-labelledby referencing its heading id', () => {
    // Use Vite's raw glob import so the test stays purely within the client TypeScript environment
    const rawFiles = import.meta.glob('./**/*.tsx', {
      query: '?raw',
      import: 'default',
      eager: true,
    }) as Record<string, string>

    const foundDialogs: { file: string; id: string }[] = []

    for (const [filePath, content] of Object.entries(rawFiles)) {
      const matches = [...content.matchAll(/<[a-zA-Z0-9]+[^>]*role="dialog"[^>]*>/g)]
      for (const match of matches) {
        const tag = match[0]
        expect(tag).toContain('aria-labelledby=')
        const labelledByMatch = tag.match(/aria-labelledby="([^"]+)"/)
        expect(labelledByMatch).not.toBeNull()
        const labelId = labelledByMatch![1]
        expect(content).toContain(`id="${labelId}"`)
        foundDialogs.push({ file: filePath, id: labelId })
      }
    }

    // App.tsx (8) + HostEditor.tsx (1) + LanguagePrompt.tsx (1) + SettingsPanel.tsx (1) = 11
    expect(foundDialogs.length).toBe(11)
  })
})
