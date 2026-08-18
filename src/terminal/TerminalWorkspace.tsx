import { lazy, memo, Suspense, useState } from 'react'
import type { Host } from '../api'
import { Icon } from '../icons'
import { translate, type Language } from '../i18n'

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
  language: Language
  onMicToggle: () => void
  onSplit: (direction: 'h' | 'v') => void
  onToggleRuntime: () => void
  onToggleFocus: () => void
  onClosePane: (paneId: number) => void
  onPasteStatus: (message: string) => void
}

export const TerminalWorkspace = memo(function TerminalWorkspace({
  visible, phase, stateLabel, selected, panes, splitDir, micListening, runtimeOpen, focusMode, language,
  onMicToggle, onSplit, onToggleRuntime, onToggleFocus, onClosePane, onPasteStatus,
}: Props) {
  const [pasteRequests, setPasteRequests] = useState<Record<string, number>>({})
  const requestPaste = (paneKey: string) => setPasteRequests((current) => ({
    ...current,
    [paneKey]: (current[paneKey] ?? 0) + 1,
  }))
  const t = (key: Parameters<typeof translate>[1], ...args: string[]) => translate(language, key, ...args)
  const primary = selected && panes[0] ? `${selected.id}:${panes[0].id}:${panes[0].channel}` : null
  const atPaneLimit = panes.length >= 20

  return <section className="terminal-card" style={{ display: visible ? undefined : 'none' }}>
    <div className="terminal-head">
      <span><i className={`dot ${phase === 'ready' ? 'online' : 'offline'}`} /> {stateLabel}</span>
      <span className="terminal-tools">
        {phase === 'ready' && <>
          <button className="mini terminal-new-action" disabled={atPaneLimit} onClick={() => onSplit(splitDir)} title={t('newTerminal')}><Icon name="plus" size={11} />{t('newTerminal')}</button>
          <button className="mini terminal-paste-action" disabled={!primary} onClick={() => primary && requestPaste(primary)} title={t('pasteAssets')}><Icon name="clipboard" size={11} />{t('pasteAssets')}</button>
          <button className={`mini terminal-tool-action${micListening ? ' mic-on' : ''}`} onClick={onMicToggle} title={t('voice')} aria-label={t('voice')}><Icon name="mic" size={11} />{t('voice')}</button>
          <button className="mini terminal-tool-action" disabled={atPaneLimit} onClick={() => onSplit('h')} title={t('splitRight')} aria-label={t('splitRight')}><Icon name="chevron" size={10} style={{ transform: 'rotate(90deg)' }} />{t('splitRight')}</button>
          <button className="mini terminal-tool-action" disabled={atPaneLimit} onClick={() => onSplit('v')} title={t('splitBelow')} aria-label={t('splitBelow')}><Icon name="chevron" size={10} style={{ transform: 'rotate(180deg)' }} />{t('splitBelow')}</button>
          <button className={`mini terminal-tool-action${runtimeOpen ? ' active' : ''}`} onClick={onToggleRuntime} title={t('runtime')} aria-label={t('runtime')}><Icon name="server" size={11} />{t('runtime')}</button>
          <button className={`mini terminal-tool-action${focusMode ? ' active' : ''}`} onClick={onToggleFocus} title={t('focus')} aria-label={t('focus')}><Icon name="eye" size={11} />{t('focus')}</button>
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
                <span className="pane-title">{t('remoteTerminal')} {index + 1} · PTY {pane.id}</span>
                <span className="pane-actions">
                  <button className="mini pane-paste" onClick={() => requestPaste(paneKey)} title={t('pasteAssets')}><Icon name="clipboard" size={10} />{language === 'zh-CN' ? '粘贴' : 'Paste'}</button>
                  <button className="icon-btn" onClick={() => onClosePane(pane.id)} title={t('closeTerminal')} aria-label={`${t('closeTerminal')} ${index + 1}`}><Icon name="close" size={10} /></button>
                </span>
              </div>
              <Suspense fallback={<div className="terminal-loading">{language === 'zh-CN' ? '正在加载终端渲染器…' : 'Loading terminal renderer…'}</div>}>
                <TerminalPane hostId={selected.id} paneId={pane.id} channelId={pane.channel} pasteRequest={pasteRequests[paneKey] ?? 0} connected onPasteStatus={onPasteStatus} />
              </Suspense>
            </div>
          })}
        </div>
      ) : <div className="terminal-empty">
        <div className="empty-symbol"><Icon name="terminal" size={40} /></div>
        <h2>{phase === 'connecting' ? (language === 'zh-CN' ? '正在建立安全连接…' : 'Establishing a secure connection…') : phase === 'ready' ? (language === 'zh-CN' ? '没有打开的远程终端' : 'No remote terminal open') : (language === 'zh-CN' ? '尚未连接' : 'Not connected')}</h2>
        <p>{phase === 'ready' ? (language === 'zh-CN' ? '使用上方“新建终端”打开一个独立 PTY。' : 'Use “New terminal” above to open an independent PTY.') : selected ? (language === 'zh-CN' ? '连接建立后，这里将显示远程 PTY。首次连接会要求确认主机密钥指纹。' : 'Remote PTY sessions appear here after connection. The first connection requires host-key fingerprint confirmation.') : (language === 'zh-CN' ? '添加工作站后，这里将成为远程终端。' : 'Add a workstation to use this remote terminal.')}</p>
      </div>}
    </div>
  </section>
})
