import { memo } from 'react'
import type { Host, HostAddress } from '../api'
import type { Language, TranslationKey } from '../i18n'
import { translate } from '../i18n'
import { Icon } from '../icons'

type Props = { language: Language; selected: Host | null; address: HostAddress | undefined; phase: 'idle' | 'connecting' | 'ready' | 'failed'; stateLabel: string; onConnect: () => void; onDisconnect: () => void; onDelete: () => void; onTunnel: () => void; onEdit: () => void }

export const WorkspaceHeader = memo(function WorkspaceHeader({ language, selected, address, phase, stateLabel, onConnect, onDisconnect, onDelete, onTunnel, onEdit }: Props) {
  const t = (key: TranslationKey, ...args: string[]) => translate(language, key, ...args)
  const stateClass = phase === 'ready' ? 'online' : phase === 'connecting' ? 'busy' : phase === 'failed' ? 'reconnecting' : 'offline'
  return <header className="topbar"><div><div className="eyebrow">{t('workspace').toUpperCase()} / {selected ? selected.label.toUpperCase() : t('unconfigured')}</div><div className="title-row"><h1>{selected ? selected.label : t('addWorkstation')}</h1><span className={'status-chip ' + phase}><span className={'dot ' + stateClass} />{stateLabel}</span></div><div className="path">{selected ? `${selected.username}@${address?.hostname_or_ip ?? t('unconfigured')}:${address?.port ?? 22}` : '—'}</div></div><div className="top-actions">{phase === 'ready' ? <button className="ghost" onClick={onDisconnect}><Icon name="power" size={13} />{t('disconnect')}</button> : <button className="primary" onClick={onConnect} disabled={!selected || phase === 'connecting'}>{phase === 'connecting' ? <><span className="spinner" />{t('connecting')}</> : <><Icon name="power" size={13} />{t('connect')}</>}</button>}{selected && phase !== 'connecting' && phase !== 'ready' && <button className="ghost danger-action" onClick={onDelete}><Icon name="trash" size={13} />{t('delete')}</button>}{selected && phase === 'ready' && <button className="ghost" onClick={onTunnel}><Icon name="link" size={12} />{t('tunnel')}</button>}{selected && phase !== 'connecting' && <button className="ghost" onClick={onEdit}><Icon name="gear" size={13} />{t('edit')}</button>}</div></header>
})
