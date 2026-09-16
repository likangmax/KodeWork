/// <reference types="node" />
import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

const sources = [
  './App.tsx',
  './settings/HostEditor.tsx',
  './settings/SettingsPanel.tsx',
  './settings/LanguagePrompt.tsx',
]

describe('dialog accessibility contract', () => {
  it('gives every modal dialog an accessible name', () => {
    let dialogCount = 0
    for (const relativePath of sources) {
      const source = readFileSync(new URL(relativePath, import.meta.url), 'utf8')
      const dialogs = source.match(/<div[^>]*role="dialog"[^>]*>/g) ?? []
      dialogCount += dialogs.length
      for (const dialog of dialogs) {
        const labelledBy = dialog.match(/aria-labelledby="([^"]+)"/)
        const directLabel = dialog.match(/aria-label="([^"]+)"/)
        expect(labelledBy || directLabel, `${relativePath}: ${dialog}`).toBeTruthy()
        if (labelledBy) expect(source).toContain(`id="${labelledBy[1]}"`)
      }
    }
    expect(dialogCount).toBeGreaterThan(0)
  })
})
