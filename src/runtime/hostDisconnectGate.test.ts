import { describe, expect, it } from 'vitest'
import { HostDisconnectGate } from './hostDisconnectGate'

describe('HostDisconnectGate', () => {
  it('blocks rapid A -> B -> A reconnect until old A disconnect settles, without blocking B', async () => {
    const gate = new HostDisconnectGate()
    const trace: string[] = []
    let finishA: (() => void) | null = null
    let signalAStart: (() => void) | undefined
    const aStarted = new Promise<void>((resolve) => { signalAStart = resolve })
    const outgoingA = gate.enqueue('a', () => new Promise<void>((resolve) => {
      trace.push('disconnect-a-started')
      finishA = resolve
      signalAStart?.()
    }))
    await aStarted
    const reconnectA = gate.awaitPending('a').then(() => { trace.push('connect-a') })
    await gate.awaitPending('b').then(() => { trace.push('connect-b') })
    expect(trace).toEqual(['disconnect-a-started', 'connect-b'])
    const unblock = finishA as (() => void) | null
    if (!unblock) throw new Error('A disconnect did not start')
    unblock()
    await Promise.all([outgoingA, reconnectA])
    expect(trace).toEqual(['disconnect-a-started', 'connect-b', 'connect-a'])
  })

  it('serializes multiple disconnects for the same host', async () => {
    const gate = new HostDisconnectGate()
    const events: string[] = []
    let release: (() => void) | null = null
    let signalFirstStart: (() => void) | undefined
    const firstStarted = new Promise<void>((resolve) => { signalFirstStart = resolve })
    const first = gate.enqueue('a', () => new Promise<void>((resolve) => {
      events.push('first')
      release = resolve
      signalFirstStart?.()
    }))
    const second = gate.enqueue('a', async () => { events.push('second') })
    await firstStarted
    expect(events).toEqual(['first'])
    const unblock = release as (() => void) | null
    if (!unblock) throw new Error('first disconnect did not start')
    unblock()
    await Promise.all([first, second])
    expect(events).toEqual(['first', 'second'])
  })

  it('preserves a failed disconnect result and permits later cleanup attempts', async () => {
    const gate = new HostDisconnectGate()
    const error = new Error('native disconnect failed')
    const failed = gate.enqueue('a', async () => { throw error })
    await expect(failed).rejects.toBe(error)
    const success = gate.enqueue('a', async () => {})
    await expect(success).resolves.toBeUndefined()
    await expect(gate.awaitPending('a')).resolves.toBeUndefined()
  })

  // The native side creates its session entry only after resolving credentials
  // and Tailscale, so a disconnect that arrives while the connect is still in
  // flight fails with "no session for host" and the connect then completes with
  // nothing tracking it. The teardown has to wait for that connect.
  it('waits for an in-flight connect before tearing the same host down', async () => {
    const gate = new HostDisconnectGate()
    const events: string[] = []
    let finishConnect: (() => void) | null = null
    let signalConnectStart: (() => void) | undefined
    const connectStarted = new Promise<void>((resolve) => { signalConnectStart = resolve })
    const connect = new Promise<void>((resolve) => {
      events.push('connect-started')
      finishConnect = resolve
      signalConnectStart?.()
    })
    gate.trackConnect('a', connect)
    await connectStarted

    const teardown = gate.enqueueAfterConnect('a', async () => { events.push('disconnect') })
    await Promise.resolve()
    await Promise.resolve()
    expect(events).toEqual(['connect-started'])
    expect(gate.isConnecting('a')).toBe(true)

    const unblock = finishConnect as (() => void) | null
    if (!unblock) throw new Error('connect did not start')
    unblock()
    await teardown
    expect(events).toEqual(['connect-started', 'disconnect'])
    expect(gate.isConnecting('a')).toBe(false)
  })

  it('does not let a failed connect block the teardown', async () => {
    const gate = new HostDisconnectGate()
    const events: string[] = []
    gate.trackConnect('a', Promise.reject(new Error('connect failed')))
    await gate.enqueueAfterConnect('a', async () => { events.push('disconnect') })
    expect(events).toEqual(['disconnect'])
    expect(gate.isConnecting('a')).toBe(false)
  })

  it('keeps a pending connect for another host out of the way', async () => {
    const gate = new HostDisconnectGate()
    const events: string[] = []
    let releaseConnect: (() => void) | null = null
    gate.trackConnect('a', new Promise<void>((resolve) => { releaseConnect = resolve }))
    await gate.enqueueAfterConnect('b', async () => { events.push('disconnect-b') })
    expect(events).toEqual(['disconnect-b'])
    const unblock = releaseConnect as (() => void) | null
    if (unblock) unblock()
  })
})
