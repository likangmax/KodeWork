/**
 * Ensures the previous native disconnect for one workstation settles before
 * that workstation can be reconnected. Operations on distinct workstations
 * are never serialized behind each other.
 */
export class HostDisconnectGate {
  private readonly pending = new Map<string, Promise<void>>()

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

  awaitPending(hostId: string): Promise<void> {
    return this.pending.get(hostId) ?? Promise.resolve()
  }
}
