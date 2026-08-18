// xterm.js terminal component: mounts the terminal, streams session
// events into it, forwards input and debounced resize back to Rust.

import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Terminal } from 'xterm'
import 'xterm/css/xterm.css'
import { memo, useCallback, useEffect, useRef, useState } from 'react'
import { clipboardCopyText, clipboardPaste, resizePty, sendInput, subscribeSession } from './api'
import { THEME_CHANGE_EVENT, terminalTheme } from './theme'
import { decodeOsc52 } from './terminal/osc52'

const inputEncoder = new TextEncoder()

type Props = {
  hostId: string
  connected: boolean
  /** Split-pane id; each pane owns one PTY channel on the host. */
  paneId: number
  /** Only events for this SSH channel are written to this terminal. */
  channelId: number
  pasteRequest: number
  onPasteStatus?: (message: string) => void
}

export const TerminalPane = memo(function TerminalPane({ hostId, connected, paneId, channelId, pasteRequest, onPasteStatus }: Props) {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const termRef = useRef<Terminal | null>(null)
  const fitRef = useRef<FitAddon | null>(null)
  const resizeTimer = useRef<number | null>(null)
  const pasteBusyRef = useRef(false)
  const [pasteBusy, setPasteBusy] = useState(false)

  const pasteFromClipboard = useCallback(async () => {
    const term = termRef.current
    if (!connected || !term || pasteBusyRef.current) return
    pasteBusyRef.current = true
    setPasteBusy(true)
    onPasteStatus?.('正在读取剪贴板；图片或 PDF 会先上传到远程主机…')
    try {
      const payload = await clipboardPaste(hostId)
      if (payload.kind === 'text') {
        term.paste(payload.text)
        onPasteStatus?.('已粘贴文本。')
      } else if (payload.kind === 'assets') {
        term.paste(payload.remote_paths.map((path) => `'${path.replaceAll("'", "'\\''")}'`).join(' '))
        onPasteStatus?.(`已上传 ${payload.remote_paths.length} 个图片/PDF，并粘贴远端路径。`)
      } else {
        onPasteStatus?.('剪贴板中没有可粘贴的文本、图片或 PDF。')
      }
    } catch (error) {
      onPasteStatus?.(`剪贴板上传失败：${String(error)}`)
    } finally {
      pasteBusyRef.current = false
      setPasteBusy(false)
      term.focus()
    }
  }, [connected, hostId, onPasteStatus])

  useEffect(() => {
    if (pasteRequest > 0) void pasteFromClipboard()
  }, [pasteRequest, pasteFromClipboard])

  useEffect(() => {
    if (!connected || !containerRef.current) return
    const container = containerRef.current
    const term = new Terminal({
      fontFamily: 'Cascadia Code, Consolas, "Courier New", monospace',
      fontSize: 13,
      lineHeight: 1.15,
      cursorBlink: true,
      // Keep long-running terminals useful without allowing many hidden
      // panes to grow the WebView heap without bound.
      scrollback: 20_000,
      theme: terminalTheme(),
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.loadAddon(new WebLinksAddon())
    term.open(container)
    term.focus()
    termRef.current = term
    fitRef.current = fit

    const onThemeChange = () => {
      term.options.theme = terminalTheme()
      if (term.rows > 0) term.refresh(0, term.rows - 1)
    }
    window.addEventListener(THEME_CHANGE_EVENT, onThemeChange)

    // Herdr, tmux, Vim and other remote TUIs use OSC 52 to copy from inside
    // their own panes. xterm.js deliberately requires the host application to
    // decide whether that remote clipboard write is allowed.
    const osc52Disposable = term.parser.registerOscHandler(52, async (data) => {
      const decoded = decodeOsc52(data)
      if (decoded.kind === 'query') return true // Clipboard reads stay disabled.
      if (decoded.kind === 'invalid') {
        onPasteStatus?.(`远端复制已阻止：${decoded.reason}`)
        return true
      }
      try {
        await clipboardCopyText(decoded.text)
        onPasteStatus?.('Herdr 选区已复制到系统剪贴板。')
      } catch (error) {
        onPasteStatus?.(`写入系统剪贴板失败：${String(error)}`)
      }
      return true
    })

    // The PTY was opened by the pane manager (App) at mount time;

    let pendingInput: Uint8Array[] = []
    let inputFlushQueued = false
    let inputChain = Promise.resolve()
    const flushInput = () => {
      inputFlushQueued = false
      if (pendingInput.length === 0) return
      const chunks = pendingInput
      pendingInput = []
      const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0)
      const merged = new Uint8Array(total)
      let offset = 0
      for (const chunk of chunks) {
        merged.set(chunk, offset)
        offset += chunk.length
      }
      // Preserve keystroke order even when the IPC bridge is busy, and keep
      // each native IPC payload comfortably below the command boundary cap.
      const chunkSize = 64 * 1024
      for (let start = 0; start < merged.length; start += chunkSize) {
        const chunk = merged.slice(start, Math.min(start + chunkSize, merged.length))
        inputChain = inputChain.then(() => sendInput(hostId, paneId, chunk)).catch(() => {})
      }
    }
    const onData = (data: string) => {
      pendingInput.push(inputEncoder.encode(data))
      if (!inputFlushQueued) {
        inputFlushQueued = true
        requestAnimationFrame(flushInput)
      }
    }
    const fitAndResize = () => {
      if (container.clientWidth < 2 || container.clientHeight < 2) return
      try { fit.fit() } catch { return }
      if (term.cols >= 2 && term.rows >= 2) {
        void resizePty(hostId, paneId, term.cols, term.rows).catch(() => {})
      }
    }
    const onResize = () => {
      if (resizeTimer.current !== null) window.clearTimeout(resizeTimer.current)
      // Debounce window drags: at most one remote resize per 150 ms.
      resizeTimer.current = window.setTimeout(fitAndResize, 100)
    }
    // Wait for both WebView layout and terminal fonts. A synchronous fit can
    // leave the remote PTY smaller than the visible surface and clip output.
    const initialFrame = requestAnimationFrame(() => requestAnimationFrame(fitAndResize))
    void document.fonts?.ready.then(fitAndResize)

    const onNativePaste = (event: KeyboardEvent) => {
      const isPaste = (event.ctrlKey && !event.altKey && event.key.toLowerCase() === 'v')
        || (event.shiftKey && event.key === 'Insert')
      if (!isPaste) return
      event.preventDefault()
      event.stopImmediatePropagation()
      void pasteFromClipboard()
    }

    let selectionTimer: number | null = null
    let lastCopiedSelection = ''
    const selectionDisposable = term.onSelectionChange(() => {
      if (selectionTimer !== null) window.clearTimeout(selectionTimer)
      selectionTimer = window.setTimeout(() => {
        const selection = term.getSelection()
        if (!selection || selection === lastCopiedSelection) return
        lastCopiedSelection = selection
        void clipboardCopyText(selection)
          .then(() => onPasteStatus?.('已复制终端选区。'))
          .catch((error) => onPasteStatus?.(`复制失败：${String(error)}`))
      }, 120)
    })

    // Batch writes per animation frame: many small Data events (each
    // JSON-array inflated over IPC) otherwise cause one xterm parse per
    // event. Merging keeps the terminal smooth under high output.
    let pending: Uint8Array[] = []
    let flushQueued = false
    let firstOutputRendered = false
    const flush = () => {
      flushQueued = false
      if (pending.length === 0) return
      const chunks = pending
      pending = []
      const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0)
      const merged = new Uint8Array(total)
      let offset = 0
      for (const chunk of chunks) {
        merged.set(chunk, offset)
        offset += chunk.length
      }
      const forceInitialRefresh = !firstOutputRendered
      term.write(merged, () => {
        if (forceInitialRefresh && term.rows > 0) {
          // WebView2 can occasionally keep the first terminal frame blank
          // until a later resize invalidates the canvas. Force one bounded
          // redraw after the first write so the login banner/prompt appears
          // immediately even when the layout is otherwise stable.
          term.refresh(0, term.rows - 1)
          firstOutputRendered = true
        }
      })
    }
    const queue = (bytes: Uint8Array) => {
      pending.push(bytes)
      if (!flushQueued) {
        flushQueued = true
        requestAnimationFrame(flush)
      }
    }
    const dispose = subscribeSession(hostId, channelId, (event) => {
      if ('Data' in event) {
        queue(new Uint8Array(event.Data.bytes))
      } else if ('ExtendedData' in event) {
        queue(new Uint8Array(event.ExtendedData.bytes))
      }
    })

    const unlistenResize = () => window.removeEventListener('resize', onResize)
    window.addEventListener('resize', onResize)
    const observer = new ResizeObserver(() => onResize())
    observer.observe(container)
    container.addEventListener('keydown', onNativePaste, true)
    term.onData(onData)

    return () => {
      dispose()
      flushInput()
      flush() // deliver anything still queued before the terminal dies
      unlistenResize()
      window.removeEventListener(THEME_CHANGE_EVENT, onThemeChange)
      osc52Disposable.dispose()
      observer.disconnect()
      cancelAnimationFrame(initialFrame)
      container.removeEventListener('keydown', onNativePaste, true)
      selectionDisposable.dispose()
      if (selectionTimer !== null) window.clearTimeout(selectionTimer)
      if (resizeTimer.current !== null) window.clearTimeout(resizeTimer.current)
      term.dispose()
      termRef.current = null
      fitRef.current = null
    }
  }, [hostId, connected, paneId, channelId, onPasteStatus, pasteFromClipboard])

  return (
    <div className="terminal-host" ref={containerRef}>
      {pasteBusy && <div className="terminal-paste-progress">正在上传剪贴板文件…</div>}
    </div>
  )
})
