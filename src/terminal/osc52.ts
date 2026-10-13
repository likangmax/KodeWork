const MAX_CLIPBOARD_BYTES = 4 * 1024 * 1024
const MAX_BASE64_LENGTH = Math.ceil(MAX_CLIPBOARD_BYTES / 3) * 4
const selectionPattern = /^[cps0-7]*$/

/**
 * Stable, non-localized rejection codes. The renderer maps them to catalog keys
 * so a refusal is reported in the interface language instead of only Chinese.
 */
export type Osc52InvalidReason =
  | 'osc52MissingSelection'
  | 'osc52InvalidSelection'
  | 'osc52TooLarge'
  | 'osc52InvalidEncoding'
  | 'osc52InvalidUtf8'

export type Osc52Result =
  | { kind: 'write'; text: string }
  | { kind: 'query' }
  | { kind: 'invalid'; reason: Osc52InvalidReason }

/** Decode the payload after `OSC 52;` without ever allowing clipboard reads. */
export function decodeOsc52(data: string): Osc52Result {
  const separator = data.indexOf(';')
  if (separator < 0) return { kind: 'invalid', reason: 'osc52MissingSelection' }
  const selection = data.slice(0, separator)
  const payload = data.slice(separator + 1)
  if (!selectionPattern.test(selection)) return { kind: 'invalid', reason: 'osc52InvalidSelection' }
  if (payload === '?') return { kind: 'query' }
  if (payload.length === 0) return { kind: 'write', text: '' }
  if (payload.length > MAX_BASE64_LENGTH) return { kind: 'invalid', reason: 'osc52TooLarge' }
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(payload) || payload.length % 4 === 1) {
    return { kind: 'invalid', reason: 'osc52InvalidEncoding' }
  }
  try {
    const binary = globalThis.atob(payload)
    if (binary.length > MAX_CLIPBOARD_BYTES) return { kind: 'invalid', reason: 'osc52TooLarge' }
    const bytes = Uint8Array.from(binary, (value) => value.charCodeAt(0))
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes)
    return { kind: 'write', text }
  } catch {
    return { kind: 'invalid', reason: 'osc52InvalidUtf8' }
  }
}
