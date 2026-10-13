/// <reference types="node" />
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const sourceRoot = fileURLToPath(new URL('.', import.meta.url))

/** Every .tsx file under src/, so a dialog added in a new file is covered
 *  instead of silently escaping a hand-maintained list. */
function componentFiles(directory: string): string[] {
  const files: string[] = []
  for (const entry of readdirSync(directory)) {
    const full = join(directory, entry)
    if (statSync(full).isDirectory()) {
      files.push(...componentFiles(full))
      continue
    }
    if (entry.endsWith('.tsx')) files.push(full)
  }
  return files
}

describe('dialog accessibility contract', () => {
  it('gives every modal dialog an accessible name', () => {
    let dialogCount = 0
    for (const file of componentFiles(sourceRoot)) {
      const relativePath = relative(sourceRoot, file).split(sep).join('/')
      const source = readFileSync(file, 'utf8')
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
