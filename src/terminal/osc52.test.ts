import { describe, expect, it } from 'vitest'
import { decodeOsc52 } from './osc52'

const encode = (text: string) => btoa(String.fromCodePoint(...new TextEncoder().encode(text)))

describe('decodeOsc52', () => {
  it('decodes Herdr-style UTF-8 clipboard writes', () => {
    expect(decodeOsc52(`c;${encode('中文 clipboard ✓')}`)).toEqual({ kind: 'write', text: '中文 clipboard ✓' })
  })

  it('recognizes clipboard queries without exposing local clipboard data', () => {
    expect(decodeOsc52('c;?')).toEqual({ kind: 'query' })
  })

  it('rejects malformed selection and base64 values', () => {
    expect(decodeOsc52('bad-selection;YQ==').kind).toBe('invalid')
    expect(decodeOsc52('c;%%%').kind).toBe('invalid')
  })

  it('accepts an empty clipboard write', () => {
    expect(decodeOsc52('c;')).toEqual({ kind: 'write', text: '' })
  })

  it('rejects decoded content larger than the native clipboard limit', () => {
    const oversized = btoa('a'.repeat(4 * 1024 * 1024 + 1))
    expect(decodeOsc52(`c;${oversized}`).kind).toBe('invalid')
  })
})
