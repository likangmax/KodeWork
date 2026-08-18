import { lazy, memo, Suspense, useState } from 'react'
import type { Host } from '../api'
import { Icon } from '../icons'

const TerminalPane = lazy(async () => import('../terminal').then((module) => ({ default: module.TerminalPane })))

type Pane = { id: number; channel: number }

type Props = {
  visible: boolean
  phase: 'idle' | 'connecting' | 'ready' | 'failed'
  stateLabel: string
  selected: Host | null
  panes: Pane[]
  splitDir: 'h' | 'v'
  micListening: boolean
  runtimeOpen: boolean
  focusMode: boolean
  onMicToggle: () => void
  onSplit: (direction: 'h' | 'v') => void
  onToggleRuntime: () => void
  onToggleFocus: () => void
  onClosePane: (paneId: number) => void
  onPasteStatus: (message: string) => void
}

export const TerminalWorkspace = memo(function TerminalWorkspace({
  visible, phase, stateLabel, selected, panes, splitDir, micListening, runtimeOpen, focusMode,
  onMicToggle, onSplit, onToggleRuntime, onToggleFocus, onClosePane, onPasteStatus,
}: Props) {
  const [pasteRequests, setPasteRequests] = useState<Record<string, number>>({})
  const requestPaste = (paneKey: string) => setPasteRequests((current) => ({
    ...current,
    [paneKey]: (current[paneKey] ?? 0) + 1,
  }))
  const primary = selected && panes[0] ? `${selected.id}:${panes[0].id}:${panes[0].channel}` : null

  return <section className="terminal-card" style={{ display: visible ? undefined : 'none' }}>
    <div className="terminal-head">
      <span><i className={`dot ${phase === 'ready' ? 'online' : 'offline'}`} /> {stateLabel}</span>
      <span className="terminal-tools">
        {phase === 'ready' && <>
          <button className="mini terminal-new-action" onClick={() => onSplit(splitDir)} title="创建一个独立的远程 PTY"><Icon name="plus" size={11} />新建终端</button>
          <button className="mini terminal-paste-action" disabled={!primary} onClick={() => primary && requestPaste(primary)} title="读取系统剪贴板；图片或 PDF 会上传并插入当前终端"><Icon name="clipboard" size={11} />粘贴图片/PDF</button>
          <button className={`icon-btn${micListening ? ' mic-on' : ''}`} onClick={onMicToggle} title="语音输入（发送到第一个远程终端）" aria-label="语音输入"><Icon name="power" size={11} /></button>
          <button className="icon-btn" onClick={() => onSplit('h')} title="新建左右分屏终端" aria-label="新建左右分屏终端"><Icon name="chevron" size={11} style={{ transform: 'rotate(90deg)' }} /></button>
          <button className="icon-btn" onClick={() => onSplit('v')} title="新建上下分屏终端" aria-label="新建上下分屏终端"><Icon name="chevron" size={11} style={{ transform: 'rotate(180deg)' }} /></button>
          <button className={`icon-btn${runtimeOpen ? ' active' : ''}`} onClick={onToggleRuntime} title="显示或隐藏 Herdr/tmux 运行时" aria-label="切换运行时面板"><Icon name="server" size={12} /></button>
          <button className={`icon-btn${focusMode ? ' active' : ''}`} onClick={onToggleFocus} title="终端专注模式" aria-label="切换终端专注模式"><Icon name="eye" size={12} /></button>
        </>}
      </span>
    </div>
    <div className="terminal-body">
      {phase === 'ready' && selected && panes.length > 0 ? (
        <div className={`split ${panes.length > 1 ? (splitDir === 'h' ? 'split-h' : 'split-v') : ''}`}>
          {panes.map((pane, index) => {
            const paneKey = `${selected.id}:${pane.id}:${pane.channel}`
            return <div className="split-pane" key={pane.id}>
              <div className="pane-bar">
                <span className="pane-title">远程终端 {index + 1} · PTY {pane.id}</span>
                <span className="pane-actions">
                  <button className="mini pane-paste" onClick={() => requestPaste(paneKey)} title="粘贴文本；图片或 PDF 会上传并插入远端路径"><Icon name="clipboard" size={10} />粘贴</button>
                  <button className="icon-btn" onClick={() => onClosePane(pane.id)} title="关闭终端" aria-label={`关闭远程终端 ${index + 1}`}><Icon name="close" size={10} /></button>
                </span>
              </div>
              <Suspense fallback={<div className="terminal-loading">正在加载终端渲染器…</div>}>
                <TerminalPane hostId={selected.id} paneId={pane.id} channelId={pane.channel} pasteRequest={pasteRequests[paneKey] ?? 0} connected onPasteStatus={onPasteStatus} />
              </Suspense>
            </div>
          })}
        </div>
      ) : <div className="terminal-empty">
        <div className="empty-symbol"><Icon name="terminal" size={40} /></div>
        <h2>{phase === 'connecting' ? '正在建立安全连接…' : phase === 'ready' ? '没有打开的远程终端' : '尚未连接'}</h2>
        <p>{phase === 'ready' ? '使用上方“新建终端”打开一个独立 PTY。' : selected ? '连接建立后，这里将显示远程 PTY。首次连接会要求确认主机密钥指纹。' : '添加工作站后，这里将成为远程终端。'}</p>
      </div>}
    </div>
  </section>
})
