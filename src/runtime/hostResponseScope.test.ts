import { describe, expect, it } from 'vitest'
import { HostResponseScope } from './hostResponseScope'

describe('HostResponseScope', () => {
  it('accepts results from the currently selected host only', () => {
    const scope = new HostResponseScope()
    scope.select('host-a')
    const fromA = scope.accepts('host-a')
    expect(fromA()).toBe(true)
    expect(scope.accepts('host-b')()).toBe(false)
    scope.select('host-b')
    expect(fromA()).toBe(false)
    expect(scope.accepts('host-b')()).toBe(true)
  })

  it('rejects stale A results even after switching A -> B -> A', () => {
    const scope = new HostResponseScope()
    scope.select('host-a')
    const staleA = scope.accepts('host-a')
    scope.select('host-b')
    scope.select('host-a')
    expect(staleA()).toBe(false)
    expect(scope.accepts('host-a')()).toBe(true)
  })

  it('keeps the existing request generation on redundant host selection', () => {
    const scope = new HostResponseScope()
    scope.select('host-a')
    const sameA = scope.accepts('host-a')
    scope.select('host-a')
    expect(sameA()).toBe(true)
    scope.select(null)
    expect(sameA()).toBe(false)
  })

  it('rejects an old asynchronous response after a host switch', async () => {
    const scope = new HostResponseScope()
    scope.select('host-a')
    const validA = scope.accepts('host-a')
    const values: string[] = []
    const pending = Promise.resolve('stale-a').then((value) => {
      if (validA()) values.push(value)
    })
    scope.select('host-b')
    await pending
    expect(values).toEqual([])
  })
})
