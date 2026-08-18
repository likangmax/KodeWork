import { useEffect, useRef } from 'react'

/**
 * Browser timers are throttled while Windows sleeps. On resume/focus/network
 * recovery, run one coalesced state probe immediately instead of waiting for
 * the regular three-second session poll.
 */
export const useResumeRecovery = (enabled: boolean, recover: () => void) => {
  const recoverRef = useRef(recover)
  recoverRef.current = recover
  useEffect(() => {
    if (!enabled) return
    let timer: number | null = null
    const schedule = () => {
      if (document.visibilityState === 'hidden') return
      if (timer !== null) window.clearTimeout(timer)
      timer = window.setTimeout(() => recoverRef.current(), 150)
    }
    window.addEventListener('focus', schedule)
    window.addEventListener('online', schedule)
    document.addEventListener('visibilitychange', schedule)
    return () => {
      if (timer !== null) window.clearTimeout(timer)
      window.removeEventListener('focus', schedule)
      window.removeEventListener('online', schedule)
      document.removeEventListener('visibilitychange', schedule)
    }
  }, [enabled])
}
