/// <reference types="node" />
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { describe, expect, it } from 'vitest'

const sourceRoot = fileURLToPath(new URL('.', import.meta.url))

function sourceFiles(directory: string): string[] {
  const files: string[] = []
  for (const entry of readdirSync(directory)) {
    const full = join(directory, entry)
    if (statSync(full).isDirectory()) {
      files.push(...sourceFiles(full))
      continue
    }
    if (/\.tsx?$/.test(entry) && !/\.test\./.test(entry)) files.push(full)
  }
  return files
}

/** Body text of every useEffect callback in a source file. */
function useEffectBodies(source: string): { line: number; body: string }[] {
  const bodies: { line: number; body: string }[] = []
  let index = 0
  while ((index = source.indexOf('useEffect(', index)) !== -1) {
    let cursor = index + 'useEffect('.length
    while (cursor < source.length && source[cursor] !== '{') cursor += 1
    let depth = 0
    let end = cursor
    for (; end < source.length; end += 1) {
      if (source[end] === '{') depth += 1
      else if (source[end] === '}') {
        depth -= 1
        if (depth === 0) break
      }
    }
    bodies.push({ line: source.slice(0, index).split('\n').length, body: source.slice(cursor, end + 1) })
    index = end + 1
  }
  return bodies
}

/** A registration and the disposal that must accompany it. `return` is enough
 *  for subscribe-style APIs, which hand back their own unsubscribe function. */
const PAIRS: { registered: RegExp; disposed: RegExp; label: string }[] = [
  { registered: /\baddEventListener\s*\(/, disposed: /\bremoveEventListener\s*\(/, label: 'addEventListener' },
  { registered: /\bsetInterval\s*\(/, disposed: /\bclearInterval\s*\(/, label: 'setInterval' },
  { registered: /\bnew ResizeObserver\s*\(/, disposed: /\.disconnect\s*\(\)/, label: 'ResizeObserver' },
  { registered: /\bnew IntersectionObserver\s*\(/, disposed: /\.disconnect\s*\(\)/, label: 'IntersectionObserver' },
  { registered: /\brequestAnimationFrame\s*\(/, disposed: /\bcancelAnimationFrame\s*\(/, label: 'requestAnimationFrame' },
  { registered: /\bsubscribe[A-Za-z]*\s*\(/, disposed: /\breturn\b/, label: 'subscribe*' },
]

describe('useEffect cleanup contract', () => {
  // A leaked timer, listener or observer keeps running after the component is
  // gone: it either touches disposed state or holds memory for the session.
  it('disposes every timer, listener, observer and subscription it registers', () => {
    const violations: string[] = []
    let scanned = 0
    for (const file of sourceFiles(sourceRoot)) {
      const relativePath = relative(sourceRoot, file).split(sep).join('/')
      for (const { line, body } of useEffectBodies(readFileSync(file, 'utf8'))) {
        scanned += 1
        for (const pair of PAIRS) {
          if (pair.registered.test(body) && !pair.disposed.test(body)) {
            violations.push(`${relativePath}:${line} registers ${pair.label} without ${pair.label === 'subscribe*' ? 'returning the unsubscribe' : `a matching dispose`}`)
          }
        }
      }
    }
    expect(scanned).toBeGreaterThan(15)
    expect(violations, `clean up these effects:\n${violations.join('\n')}`).toEqual([])
  })
})
