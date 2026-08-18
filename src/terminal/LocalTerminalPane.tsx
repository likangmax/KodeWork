import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Terminal } from 'xterm'
import { memo, useEffect, useRef } from 'react'
import { clipboardCopyText, localTerminalResize, localTerminalWrite, subscribeLocalTerminal } from '../api'
import { THEME_CHANGE_EVENT, terminalTheme } from '../theme'

const encoder = new TextEncoder()

export const LocalTerminalPane = memo(function LocalTerminalPane({ id, active, onStatus }: { id: number; active: boolean; onStatus: (message: string) => void }) {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const termRef = useRef<Terminal | null>(null)
  const fitRef = useRef<FitAddon | null>(null)

  useEffect(() => {
    const container = containerRef.current
    if (!container) return
    const term = new Terminal({
      fontFamily: 'Cascadia Code, Consolas, "Courier New", monospace',
      fontSize: 13, lineHeight: 1.15, cursorBlink: true, scrollback: 20_000, theme: terminalTheme(),
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.loadAddon(new WebLinksAddon())
    term.open(container)
    termRef.current = term
    fitRef.current = fit

    let inputChain = Promise.resolve()
    const input = term.onData((data) => {
      inputChain = inputChain.then(() => localTerminalWrite(id, encoder.encode(data))).catch(() => {})
    })
    let resizeTimer: number | null = null
    const fitAndResize = () => {
      if (container.clientWidth < 2 || container.clientHeight < 2) return
      try { fit.fit() } catch { return }
      if (term.cols >= 2 && term.rows >= 2) void localTerminalResize(id, term.cols, term.rows).catch(() => {})
    }
    const observer = new ResizeObserver(() => {
      if (resizeTimer !== null) window.clearTimeout(resizeTimer)
      resizeTimer = window.setTimeout(fitAndResize, 80)
    })
    observer.observe(container)
    const initial = requestAnimationFrame(() => requestAnimationFrame(fitAndResize))

    let pending: Uint8Array[] = []
    let flushQueued = false
    const flush = () => {
      flushQueued = false
      if (pending.length === 0) return
      const chunks = pending; pending = []
      const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0)
      const merged = new Uint8Array(total)
      let offset = 0
      for (const chunk of chunks) { merged.set(chunk, offset); offset += chunk.length }
      term.write(merged)
    }
    const dispose = subscribeLocalTerminal(id, (event) => {
      if (event.kind === 'data') {
        pending.push(new Uint8Array(event.bytes))
        if (!flushQueued) { flushQueued = true; requestAnimationFrame(flush) }
      } else if (event.kind === 'exited') {
        term.write(`\r\n[进程已退出，代码 ${event.code}]\r\n`)
      } else {
        term.write(`\r\n[本地终端错误：${event.message}]\r\n`)
      }
    })
    let selectionTimer: number | null = null
    const selection = term.onSelectionChange(() => {
      if (selectionTimer !== null) window.clearTimeout(selectionTimer)
      selectionTimer = window.setTimeout(() => {
        const text = term.getSelection()
        if (text) void clipboardCopyText(text).then(() => onStatus('已复制本机终端选区。')).catch((error) => onStatus(`复制失败：${String(error)}`))
      }, 120)
    })
    const onTheme = () => { term.options.theme = terminalTheme(); if (term.rows > 0) term.refresh(0, term.rows - 1) }
    window.addEventListener(THEME_CHANGE_EVENT, onTheme)
    return () => {
      dispose(); input.dispose(); selection.dispose(); observer.disconnect(); cancelAnimationFrame(initial)
      window.removeEventListener(THEME_CHANGE_EVENT, onTheme)
      if (resizeTimer !== null) window.clearTimeout(resizeTimer)
      if (selectionTimer !== null) window.clearTimeout(selectionTimer)
      term.dispose(); termRef.current = null; fitRef.current = null
    }
  }, [id, onStatus])

  useEffect(() => {
    if (!active) return
    requestAnimationFrame(() => {
      try { fitRef.current?.fit() } catch { /* layout is not ready */ }
      termRef.current?.focus()
    })
  }, [active])

  return <div className="terminal-host" ref={containerRef} />
})
