/** Keep browser speech recognition bound to its original SSH workstation. */
export type VoiceDestination = { hostId: string; paneId: number }

export function voiceDestination(
  originalHostId: string,
  stillSelected: boolean,
  ready: boolean,
  currentPaneId: number | null,
): VoiceDestination | null {
  if (!originalHostId || !stillSelected || !ready || currentPaneId === null) return null
  return { hostId: originalHostId, paneId: currentPaneId }
}
