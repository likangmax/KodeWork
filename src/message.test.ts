import { describe, expect, it } from 'vitest'
import { isErrorMessage } from './message'

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
})
