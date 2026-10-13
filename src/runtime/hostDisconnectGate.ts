/**
 * Serializes per-workstation teardown.
 *
 * Two ordering problems are handled here, both observed as real user-visible
 * defects:
 *
 * 1. A switch A -> B -> A must not let the *old* A disconnect run after the new
 *    A connection was created, which would tear down the fresh transport.
 * 2. A disconnect issued while a connect for the same workstation is still in
 *    flight must wait for that connect: the native side creates its session
 *    entry only after resolving credentials and Tailscale, so an early
 *    disconnect fails with "no session for host" and the connect then completes
 *    with no renderer tracking it.
 *
 * Operations on distinct workstations are never serialized behind each other.
 */
export class HostDisconnectGate {
  private readonly pending = new Map<string, Promise<void>>()
  private readonly connects = new Map<string, Promise<unknown>>()

  enqueue(hostId: string, operation: () => Promise<void>): Promise<void> {
    const earlier = this.pending.get(hostId) ?? Promise.resolve()
    const current = earlier.catch(() => {}).then(operation)
    this.pending.set(hostId, current)
    const release = () => {
      if (this.pending.get(hostId) === current) this.pending.delete(hostId)
    }
    void current.then(release, release)
    return current
  }

  /**
   * Records an in-flight connect so a later teardown can wait for it. Returns
   * the same promise, so callers can use it inline.
   */
  trackConnect<T>(hostId: string, connect: Promise<T>): Promise<T> {
    this.connects.set(hostId, connect)
    const release = () => {
      if (this.connects.get(hostId) === connect) this.connects.delete(hostId)
    }
    void connect.then(release, release)
    return connect
  }

  /**
   * Enqueues a teardown that first waits for any in-flight connect for the same
   * workstation. A failed connect must not block the teardown, so the connect is
   * only awaited, never propagated.
   */
  enqueueAfterConnect(hostId: string, operation: () => Promise<void>): Promise<void> {
    const connect = this.connects.get(hostId)
    if (!connect) return this.enqueue(hostId, operation)
    return this.enqueue(hostId, () => connect.then(
      () => operation(),
      () => operation(),
    ))
  }

  awaitPending(hostId: string): Promise<void> {
    return this.pending.get(hostId) ?? Promise.resolve()
  }

  /** Test/diagnostics hook: is a connect currently tracked for this host? */
  isConnecting(hostId: string): boolean {
    return this.connects.has(hostId)
  }
}
