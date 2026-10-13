/// <reference types="node" />
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'
import { translationCatalogs } from './i18n'

const sourceRoot = fileURLToPath(new URL('.', import.meta.url))

function sourceFiles(directory: string): string[] {
  const files: string[] = []
  for (const entry of readdirSync(directory)) {
    const full = join(directory, entry)
    if (statSync(full).isDirectory()) {
      files.push(...sourceFiles(full))
      continue
    }
    if (/\.tsx?$/.test(entry)) files.push(full)
  }
  return files
}

const catalogueKeys = Object.keys(translationCatalogs['zh-CN'])
const sources = sourceFiles(sourceRoot)
  .filter((file) => !file.endsWith(`${sep}i18n.ts`))
  .map((file) => ({ path: relative(sourceRoot, file).split(sep).join('/'), text: readFileSync(file, 'utf8') }))

describe('i18n catalogue usage', () => {
  it('has a catalogue of a realistic size', () => {
    expect(catalogueKeys.length).toBeGreaterThan(100)
  })

  // A key that no source mentions can never be shown. It is dead weight in both
  // languages and a sign that a rename left the catalogue behind. A whole-word
  // match is used rather than requiring `t('key')`, because two call sites are
  // deliberately dynamic (`t(decoded.reason)` maps OSC 52 rejection codes, which
  // do appear as literals in the terminal module).
  it('does not carry keys that no source references', () => {
    const dead = catalogueKeys.filter((key) => {
      const pattern = new RegExp(`\\b${key}\\b`)
      return !sources.some(({ text }) => pattern.test(text))
    })
    expect(dead, `remove or re-reference these catalogue keys: ${dead.join(', ')}`).toEqual([])
  })

  // The reverse direction is enforced by the type system (`TranslationKey`), so
  // this only guards against a key being spelled as a string without t(...).
  it('keeps the two catalogues key-identical', () => {
    expect(Object.keys(translationCatalogs['en-US']).sort()).toEqual([...catalogueKeys].sort())
  })
})
