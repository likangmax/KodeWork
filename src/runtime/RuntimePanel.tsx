import { memo } from 'react'
import { agentStatusLabel, type HerdrAgentInfo, type TmuxSession } from '../api'
import { Icon } from '../icons'
import type { Translator } from '../i18n'

const statusClass = (agent: HerdrAgentInfo): string => ({ Working: 'working', Blocked: 'blocked', Idle: 'idle', Done: 'done', Unknown: 'unknown' }[agentStatusLabel(agent)])

type Props = {
  herdrVersion: string | null
  herdrMissing: boolean
  herdrError: string | null
  agents: HerdrAgentInfo[]
  bridgeInfo: { local: string; socket: string } | null
  tmuxSessions: TmuxSession[]
  newTmuxName: string
  onNewTmuxName: (value: string) => void
  onHerdrBridge: () => void
  onHerdrBridgeStop: () => void
  onHerdrAttach: () => void
  onTmuxAttach: (name: string) => void
  onTmuxKill: (name: string) => void
  onTmuxCreate: () => void
  t: Translator
}

export const RuntimePanel = memo(function RuntimePanel({
  herdrVersion, herdrMissing, herdrError, agents, bridgeInfo, tmuxSessions,
  newTmuxName, onNewTmuxName, onHerdrBridge, onHerdrBridgeStop, onHerdrAttach,
  onTmuxAttach, onTmuxKill, onTmuxCreate, t,
}: Props) {
  return (
    <aside className="runtime-panel">
      <div className="runtime-block">
        <div className="runtime-title">
          <span>herdr</span>
          <small title={herdrError ?? undefined}>{herdrMissing ? t('notDetected') : (herdrVersion ?? (herdrError ? t('temporarilyUnavailable') : t('detecting')))}</small>
          <button className="mini" onClick={onHerdrBridge} title={t('bridgeHerdrHint')}><Icon name="link" size={10} />{t('bridge')}</button>
          <button className="mini" onClick={onHerdrAttach} title={t('startHerdrHint')}>{t('start')}</button>
        </div>
        {bridgeInfo && <div className="bridge-note">{bridgeInfo.local} ↔ {bridgeInfo.socket}<button className="mini" onClick={onHerdrBridgeStop}>{t('stop')}</button></div>}
        {agents.length === 0 ? (
          <div className={`runtime-empty${herdrError ? ' error' : ''}`} title={herdrError ?? undefined}>
            {herdrMissing ? t('remoteHerdrMissing') : herdrError ? t('herdrAgentsUnavailable') : t('noRunningAgents')}
          </div>
        ) : agents.map((agent, index) => (
          <div className="agent-row" key={agent.name ?? agent.pane_id ?? String(index)}>
            <span className={`agent-dot ${statusClass(agent)}`} />
            <span className="agent-name">{agent.name ?? '?'}</span>
            <span className="agent-kind">{agent.kind ?? ''}</span>
            <span className="agent-status">{agentStatusLabel(agent)}</span>
          </div>
        ))}
      </div>
      <div className="runtime-block">
        <div className="runtime-title"><span>tmux</span><small>{t('sessionCount', String(tmuxSessions.length))}</small></div>
        {tmuxSessions.length === 0 && <div className="runtime-empty">{t('noTmuxSessions')}</div>}
        {tmuxSessions.map((session) => (
          <div className="tmux-row" key={session.name}>
            <span className="tmux-name">{session.name}</span>
            <span className="tmux-meta">{session.windows}w · {session.attached}a</span>
            <button className="mini" onClick={() => onTmuxAttach(session.name)}>attach</button>
            <button className="mini danger" onClick={() => { if (window.confirm(t('confirmKillTmux', session.name))) onTmuxKill(session.name) }}>kill</button>
          </div>
        ))}
        <div className="tmux-create">
          <input value={newTmuxName} onChange={(event) => onNewTmuxName(event.target.value)} placeholder={t('newSessionName')} onKeyDown={(event) => { if (event.key === 'Enter') onTmuxCreate() }} />
          <button className="mini" onClick={onTmuxCreate}>{t('create')}</button>
        </div>
      </div>
    </aside>
  )
})
