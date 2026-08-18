import { memo } from 'react'
import type { Host, HostAddress } from '../api'
import { Icon } from '../icons'

type Props = { selected: Host | null; address: HostAddress | undefined; phase: 'idle' | 'connecting' | 'ready' | 'failed'; onConnect: () => void; onDisconnect: () => void; onDelete: () => void; onTunnel: () => void; onEdit: () => void }

export const WorkspaceHeader = memo(function WorkspaceHeader({ selected, address, phase, onConnect, onDisconnect, onDelete, onTunnel, onEdit }: Props) {
  return <header className="topbar"><div><div className="eyebrow">WORKSPACE / {selected ? selected.label.toUpperCase() : '未配置'}</div><h1>{selected ? selected.label : '先添加一台远程工作站'}</h1><div className="path">{selected ? `${selected.username}@${address?.hostname_or_ip ?? '未填写地址'}:${address?.port ?? 22}` : '—'}</div></div><div className="top-actions">{phase === 'ready' ? <button className="ghost" onClick={onDisconnect}><Icon name="power" size={13} />断开</button> : <button className="primary" onClick={onConnect} disabled={!selected || phase === 'connecting'}>{phase === 'connecting' ? <><span className="spinner" />连接中…</> : <><Icon name="power" size={13} />连接</>}</button>}{selected && phase !== 'connecting' && phase !== 'ready' && <button className="ghost danger-action" onClick={onDelete}><Icon name="trash" size={13} />删除</button>}{selected && phase === 'ready' && <button className="ghost" onClick={onTunnel}><Icon name="link" size={12} />隧道</button>}{selected && phase !== 'connecting' && <button className="ghost" onClick={onEdit}><Icon name="gear" size={13} />编辑</button>}</div></header>
})
