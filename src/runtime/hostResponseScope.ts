/**
 * Tracks the selected workstation across asynchronous native API calls.
 * A -> B -> A is a *new* selection: a response from the first A session
 * must not apply merely because the selected host id matches again.
 */
export class HostResponseScope {
  private hostId: string | null = null
  private revision = 0

  select(hostId: string | null): void {
    if (this.hostId === hostId) return
    this.hostId = hostId
    this.revision += 1
  }

  /**
   * Snapshot the current host selection. A promise may update the UI only
   * while the original generation is still selected.
   */
  accepts(hostId: string): () => boolean {
    const revision = this.revision
    return () => this.hostId === hostId && this.revision === revision
  }
}
