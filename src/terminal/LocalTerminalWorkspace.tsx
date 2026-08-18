import { lazy, memo, Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react'
import type { LocalTerminalCapabilities, LocalTerminalDescriptor, LocalTerminalKind } from '../api'
import { localTerminalCapabilities, localTerminalClose, localTerminalOpen } from '../api'
import { Icon } from '../icons'

const LocalTerminalPane = lazy(async () => import('./LocalTerminalPane').then((module) => ({ default: module.LocalTerminalPane })))

type Props = { visible: boolean; onStatus: (message: string) => void }

const kindLabel: Record<LocalTerminalKind, string> = {
  power_shell: 'PowerShell',
  command_prompt: 'CMD',
  wsl: 'WSL',
}

export const LocalTerminalWorkspace = memo(function LocalTerminalWorkspace({ visible, onStatus }: Props) {
  const [capabilities, setCapabilities] = useState<LocalTerminalCapabilities | null>(null)
  const [terminals, setTerminals] = useState<LocalTerminalDescriptor[]>([])
  const [activeId, setActiveId] = useState<number | null>(null)
  const [wslDistribution, setWslDistribution] = useState('')
  const [opening, setOpening] = useState<LocalTerminalKind | null>(null)
  const terminalsRef = useRef<LocalTerminalDescriptor[]>([])
  terminalsRef.current = terminals

  const refreshCapabilities = useCallback(async () => {
    try {
      const result = await localTerminalCapabilities()
      setCapabilities(result)
      setWslDistribution((current) => current || result.wsl_distributions[0] || '')
    } catch (error) {
      onStatus(`读取本机终端能力失败：${String(error)}`)
    }
  }, [onStatus])

  useEffect(() => { void refreshCapabilities() }, [refreshCapabilities])

  const openTerminal = useCallback(async (kind: LocalTerminalKind) => {
    if (kind === 'wsl' && !wslDistribution) {
      onStatus('没有可用的 WSL 发行版。请先安装并初始化 WSL。')
      return
    }
    setOpening(kind)
    try {
      const descriptor = await localTerminalOpen(kind, kind === 'wsl' ? wslDistribution : undefined)
      setTerminals((current) => [...current, descriptor])
      setActiveId(descriptor.id)
      onStatus(`已打开 ${descriptor.label}。`)
    } catch (error) {
      onStatus(`打开本机终端失败：${String(error)}`)
    } finally {
      setOpening(null)
    }
  }, [onStatus, wslDistribution])

  const closeTerminal = useCallback((id: number) => {
    void localTerminalClose(id).catch((error) => onStatus(`关闭本机终端失败：${String(error)}`))
    setTerminals((current) => {
      const next = current.filter((terminal) => terminal.id !== id)
      setActiveId((active) => active === id ? (next.at(-1)?.id ?? null) : active)
      return next
    })
  }, [onStatus])

  useEffect(() => () => {
    for (const terminal of terminalsRef.current) void localTerminalClose(terminal.id).catch(() => {})
  }, [])

  const available = useMemo(() => capabilities ?? {
    powershell: false, command_prompt: false, wsl: false, wsl_distributions: [],
  }, [capabilities])

  return <section className="terminal-card local-terminal-card" style={{ display: visible ? undefined : 'none' }}>
    <div className="terminal-head local-terminal-head">
      <span><i className="dot online" /> 本机终端</span>
      <span className="terminal-tools local-terminal-tools">
        <button className="mini" disabled={!available.powershell || opening !== null} onClick={() => void openTerminal('power_shell')} title="打开 PowerShell"><Icon name="plus" size={11} />PowerShell</button>
        <button className="mini" disabled={!available.command_prompt || opening !== null} onClick={() => void openTerminal('command_prompt')} title="打开命令提示符"><Icon name="plus" size={11} />CMD</button>
        {available.wsl && <>
          <select aria-label="WSL 发行版" value={wslDistribution} onChange={(event) => setWslDistribution(event.target.value)} disabled={opening !== null}>
            <option value="">选择 WSL…</option>
            {available.wsl_distributions.map((name) => <option key={name} value={name}>{name}</option>)}
          </select>
          <button className="mini" disabled={!wslDistribution || opening !== null} onClick={() => void openTerminal('wsl')} title="打开 WSL"><Icon name="plus" size={11} />WSL</button>
        </>}
        <button className="icon-btn" onClick={() => void refreshCapabilities()} title="重新检测本机终端" aria-label="重新检测本机终端"><Icon name="refresh" size={12} /></button>
      </span>
    </div>
    <div className="local-terminal-tabs" role="tablist" aria-label="本机终端标签">
      {terminals.map((terminal) => <div key={terminal.id} className={`local-terminal-tab ${activeId === terminal.id ? 'active' : ''}`}>
        <button type="button" className="local-tab-select" role="tab" aria-selected={activeId === terminal.id} onClick={() => setActiveId(terminal.id)}>
          {kindLabel[terminal.kind]}{terminal.kind === 'wsl' ? ` · ${terminal.label.replace('WSL · ', '')}` : ''}
        </button>
        <button type="button" className="local-tab-close" aria-label={`关闭 ${terminal.label}`} title={`关闭 ${terminal.label}`} onClick={() => closeTerminal(terminal.id)}><Icon name="close" size={10} /></button>
      </div>)}
    </div>
    <div className="terminal-body local-terminal-body">
      {terminals.length === 0 ? <div className="terminal-empty">
        <div className="empty-symbol"><Icon name="terminal" size={40} /></div>
        <h2>选择一个本机终端</h2>
        <p>PowerShell、CMD 和 WSL 与远程 SSH 会话相互独立，可同时打开多个标签。</p>
      </div> : terminals.map((terminal) => <div className="local-terminal-pane" key={terminal.id} style={{ display: activeId === terminal.id ? undefined : 'none' }}>
        <Suspense fallback={<div className="terminal-loading">正在加载本机终端渲染器…</div>}>
          <LocalTerminalPane id={terminal.id} active={activeId === terminal.id} onStatus={onStatus} />
        </Suspense>
      </div>)}
    </div>
  </section>
})
