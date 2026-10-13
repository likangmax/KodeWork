import { describe, expect, it } from 'vitest'
import { translationCatalogs } from './i18n'
import { isErrorMessage } from './message'

/** Catalogue entries that reach setMessage() and describe a failure. Each one
 *  must be highlighted; a new failure message with no matching keyword fails
 *  here instead of silently losing its error styling in the UI. */
const failureKeys = [
  'credentialRequired',
  'reconnectNeedsCredential',
  'reconnectNeedsPassword',
  'speechUnsupported',
  'fillRequiredFields',
  'remotePortRange',
  'localPortRange',
  'connectFailed',
  'saveFailed',
  'loadConfigFailed',
  'speechInputFailed',
] as const

/** Catalogue entries that reach setMessage() and report success or plain
 *  information. Highlighting these as failures would be a regression. */
const informationalKeys = [
  'connectingTo',
  'loadedWorkstations',
  'updateInstalled',
  'tunnelEstablished',
  'pinnedDefaultDir',
  'disconnectedRemoteSessionsSurvive',
] as const

const render = (language: 'zh-CN' | 'en-US', key: string): string => {
  const value = (translationCatalogs[language] as Record<string, unknown>)[key]
  return typeof value === 'function' ? (value as (...args: string[]) => string)('X', 'Y') : String(value ?? '')
}

describe('isErrorMessage', () => {
  it('classifies the Chinese remote-result timeout warning as an error', () => {
    expect(isErrorMessage('等待远端结果超时；远端进程可能仍在运行。')).toBe(true)
  })

  it.each(['Connection timeout', 'CONNECTION TIMEOUT'])(
    'preserves case-insensitive English timeout detection: %s',
    (message) => expect(isErrorMessage(message)).toBe(true),
  )

  it.each([
    '保存失败',
    '配置错误',
    '无法连接',
    '连接已拒绝',
    '运行异常',
    'Operation failed',
    'Connection error',
    'Permission denied',
    'Connection refused',
    'Invalid configuration',
  ])(
    'preserves existing error detection: %s',
    (message) => expect(isErrorMessage(message)).toBe(true),
  )

  it.each(['', '已保存工作站', 'Settings saved'])(
    'does not classify an informational message as an error: %s',
    (message) => expect(isErrorMessage(message)).toBe(false),
  )

  it.each(['zh-CN', 'en-US'] as const)('highlights every failure message in %s', (language) => {
    const missed = failureKeys.filter((key) => !isErrorMessage(render(language, key)))
    expect(missed, `these failure messages would render without error styling: ${missed.join(', ')}`).toEqual([])
  })

  it.each(['zh-CN', 'en-US'] as const)('leaves success and status messages unhighlighted in %s', (language) => {
    const wronglyFlagged = informationalKeys.filter((key) => isErrorMessage(render(language, key)))
    expect(wronglyFlagged, `these messages would render with error styling: ${wronglyFlagged.join(', ')}`).toEqual([])
  })
})
