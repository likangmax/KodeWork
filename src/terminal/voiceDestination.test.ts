import { describe, expect, it } from 'vitest'
import { HostResponseScope } from '../runtime/hostResponseScope'
import { voiceDestination } from './voiceDestination'

describe('voiceDestination', () => {
  it('keeps a speech result on the originating workstation and currently open pane', () => {
    const scope = new HostResponseScope()
    scope.select('a')
    const originalLease = scope.accepts('a')
    expect(voiceDestination('a', originalLease(), true, 7)).toEqual({ hostId: 'a', paneId: 7 })
  })

  it('drops A speech after A to B to A instead of routing it to a newer session', () => {
    const scope = new HostResponseScope()
    scope.select('a')
    const originalLease = scope.accepts('a')
    scope.select('b')
    expect(voiceDestination('a', originalLease(), true, 0)).toBeNull()
    scope.select('a')
    expect(voiceDestination('a', originalLease(), true, 0)).toBeNull()
  })

  it('does not inject into a disconnected session or closed pane', () => {
    expect(voiceDestination('a', true, false, 1)).toBeNull()
    expect(voiceDestination('a', true, true, null)).toBeNull()
  })
})
