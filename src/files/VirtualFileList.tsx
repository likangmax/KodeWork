import { memo, useCallback, useLayoutEffect, useMemo, useRef, useState } from 'react'
import type { RemoteFileMeta } from '../api'
import { Icon } from '../icons'
import type { Translator } from '../i18n'
import { calculateVirtualWindow } from './virtualization'

const ROW_HEIGHT = 34
const OVERSCAN = 10

type Props = {
  entries: RemoteFileMeta[]
  currentPath: string
  selectedRemote: string | null
  formatSize: (bytes: number) => string
  onOpen: (entry: RemoteFileMeta) => void
  t: Translator
}

const remotePathFor = (currentPath: string, name: string) =>
  currentPath === '/' ? `/${name}` : `${currentPath}/${name}`

export const VirtualFileList = memo(function VirtualFileList({
  entries,
  currentPath,
  selectedRemote,
  formatSize,
  onOpen,
  t,
}: Props) {
  const viewportRef = useRef<HTMLDivElement | null>(null)
  const [viewport, setViewport] = useState({ scrollTop: 0, height: 0 })

  const measure = useCallback(() => {
    const element = viewportRef.current
    if (!element) return
    setViewport((previous) => {
      const next = { scrollTop: element.scrollTop, height: element.clientHeight }
      return previous.scrollTop === next.scrollTop && previous.height === next.height ? previous : next
    })
  }, [])

  useLayoutEffect(() => {
    const element = viewportRef.current
    if (!element) return
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [measure])

  useLayoutEffect(() => {
    const element = viewportRef.current
    if (!element) return
    element.scrollTop = 0
    measure()
  }, [currentPath, measure])

  const windowRange = useMemo(
    () => calculateVirtualWindow(entries.length, viewport.scrollTop, viewport.height, ROW_HEIGHT, OVERSCAN),
    [entries.length, viewport],
  )
  const visible = entries.slice(windowRange.start, windowRange.end)

  return (
    <div
      ref={viewportRef}
      className="files-body virtual-file-viewport"
      onScroll={measure}
      role="listbox"
      aria-label={t('remoteDirectoryItems', currentPath, String(entries.length))}
    >
      <div className="virtual-file-space" style={{ height: windowRange.totalHeight }}>
        <div className="virtual-file-window" style={{ transform: `translateY(${windowRange.offsetTop}px)` }}>
          {visible.map((entry, visibleIndex) => {
            const absoluteIndex = windowRange.start + visibleIndex
            const remotePath = remotePathFor(currentPath, entry.name)
            return (
              <div
                key={entry.name}
                className={`file-row ${selectedRemote === remotePath ? 'selected' : ''}`}
                style={{ height: ROW_HEIGHT }}
                onClick={() => onOpen(entry)}
                onDoubleClick={() => { if (entry.is_dir) onOpen(entry) }}
                role="option"
                aria-selected={selectedRemote === remotePath}
                aria-setsize={entries.length}
                aria-posinset={absoluteIndex + 1}
                title={entry.name}
              >
                <span className="file-icon"><Icon name={entry.is_dir ? 'chevron' : 'arrow_down'} size={11} /></span>
                <span className="file-name">{entry.name}</span>
                <span className="file-size">{entry.is_dir ? '—' : formatSize(entry.size)}</span>
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
})
