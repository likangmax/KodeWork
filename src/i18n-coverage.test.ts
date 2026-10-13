/// <reference types="node" />
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const sourceRoot = fileURLToPath(new URL('.', import.meta.url))
const han = /\p{Script=Han}/u

/** Every .ts/.tsx file under src/, except the catalogue itself and tests. */
function sourceFiles(directory: string): string[] {
  const files: string[] = []
  for (const entry of readdirSync(directory)) {
    const full = join(directory, entry)
    if (statSync(full).isDirectory()) {
      files.push(...sourceFiles(full))
      continue
    }
    if (!/\.tsx?$/.test(entry)) continue
    const relativePath = relative(sourceRoot, full).split(sep).join('/')
    if (relativePath === 'i18n.ts' || /\.test\.tsx?$/.test(relativePath)) continue
    files.push(full)
  }
  return files
}

describe('i18n coverage contract', () => {
  // The two languages are complete catalogues, so user-visible text has no
  // business being written inline. Regression this prevents: Chinese status
  // messages rendered in the English interface (and English eyebrows rendered
  // in the Chinese interface) because a component hardcoded the string.
  it('keeps user-visible text in the catalogue instead of inline literals', () => {
    const violations: string[] = []
    for (const file of sourceFiles(sourceRoot)) {
      const relativePath = relative(sourceRoot, file).split(sep).join('/')
      readFileSync(file, 'utf8').split(/\r?\n/).forEach((line, index) => {
        if (!han.test(line)) return
        // Pattern matching against remote/heterogeneous output, or a place that
        // deliberately shows both languages, opts out with an inline marker.
        if (line.includes('i18n-exempt')) return
        violations.push(`${relativePath}:${index + 1}: ${line.trim().slice(0, 120)}`)
      })
    }
    expect(violations, `move these strings into src/i18n.ts:\n${violations.join('\n')}`).toEqual([])
  })

  // The section labels above each dialog used to be literal English, so the
  // Chinese interface showed AUTHENTICATION / PROJECT / CONFIRM and friends.
  it('renders every dialog eyebrow through the translator', () => {
    const violations: string[] = []
    for (const file of sourceFiles(sourceRoot)) {
      const relativePath = relative(sourceRoot, file).split(sep).join('/')
      for (const line of readFileSync(file, 'utf8').split(/\r?\n/)) {
        if (line.includes('i18n-exempt')) continue
        // At least one character must follow the tag, so `>{t('...')}` (a JSX
        // expression) is fine while `>SETTINGS` is not.
        const literal = line.match(/className="eyebrow">([^<{]{1,40})/)?.[1]
        if (literal) violations.push(`${relativePath}: literal eyebrow text ${JSON.stringify(literal)}`)
      }
    }
    expect(violations, `use t(...) for eyebrow labels:\n${violations.join('\n')}`).toEqual([])
  })

  it('scans a meaningful number of files', () => {
    expect(sourceFiles(sourceRoot).length).toBeGreaterThan(10)
  })
})
