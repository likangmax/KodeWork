import { describe, expect, it } from 'vitest'
import { translationCatalogs } from './i18n'

describe('translation catalogs', () => {
  it('keeps English and Simplified Chinese keys aligned', () => {
    expect(Object.keys(translationCatalogs['en-US']).sort()).toEqual(
      Object.keys(translationCatalogs['zh-CN']).sort(),
    )
  })

  it('keeps the English connection failure path localized', () => {
    expect(translationCatalogs['en-US'].credentialRequired).not.toMatch(/[\u3400-\u9fff]/u)
    expect(translationCatalogs['en-US'].connectFailed('diagnostic')).not.toMatch(/[\u3400-\u9fff]/u)
  })
})
