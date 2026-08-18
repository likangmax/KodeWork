const MAX_CLIPBOARD_BYTES = 4 * 1024 * 1024
const MAX_BASE64_LENGTH = Math.ceil(MAX_CLIPBOARD_BYTES / 3) * 4
const selectionPattern = /^[cps0-7]*$/

export type Osc52Result =
  | { kind: 'write'; text: string }
  | { kind: 'query' }
  | { kind: 'invalid'; reason: string }

/** Decode the payload after `OSC 52;` without ever allowing clipboard reads. */
export function decodeOsc52(data: string): Osc52Result {
  const separator = data.indexOf(';')
  if (separator < 0) return { kind: 'invalid', reason: '缺少剪贴板选择参数' }
  const selection = data.slice(0, separator)
  const payload = data.slice(separator + 1)
  if (!selectionPattern.test(selection)) return { kind: 'invalid', reason: '剪贴板选择参数无效' }
  if (payload === '?') return { kind: 'query' }
  if (payload.length === 0) return { kind: 'write', text: '' }
  if (payload.length > MAX_BASE64_LENGTH) return { kind: 'invalid', reason: '远端剪贴板内容超过 4 MiB' }
  if (!/^[A-Za-z0-9+/]*={0,2}$/.test(payload) || payload.length % 4 === 1) {
    return { kind: 'invalid', reason: '远端剪贴板编码无效' }
  }
  try {
    const binary = globalThis.atob(payload)
    if (binary.length > MAX_CLIPBOARD_BYTES) return { kind: 'invalid', reason: '远端剪贴板内容超过 4 MiB' }
    const bytes = Uint8Array.from(binary, (value) => value.charCodeAt(0))
    const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes)
    return { kind: 'write', text }
  } catch {
    return { kind: 'invalid', reason: '远端剪贴板不是有效 UTF-8 文本' }
  }
}
