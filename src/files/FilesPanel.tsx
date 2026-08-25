import type { RemoteFileMeta } from '../api'
import { Icon } from '../icons'
import type { Translator } from '../i18n'
import { VirtualFileList } from './VirtualFileList'

export type TransferView = {
  status: string
  transferred: number
  total: number | null
  speed: number
  message?: string
}

type Props = {
  currentPath: string
  entries: RemoteFileMeta[]
  loading: boolean
  selectedRemote: string | null
  transfers: Record<string, TransferView>
  formatSize: (bytes: number) => string
  onPathChange: (path: string) => void
  onOpen: (entry: RemoteFileMeta) => void
  onRefresh: () => void
  onUpload: () => void
  onDownload: () => void
  onYazi: () => void
  pinnedPath: string
  pinned: boolean
  onPinCurrentPath: () => void
  onGoPinnedPath: () => void
  onPauseTransfer: (id: string) => void
  onResumeTransfer: (id: string) => void
  onCancelTransfer: (id: string) => void
  onDismissTransfer: (id: string) => void
  t: Translator
}

export function FilesPanel({
  currentPath, entries, loading, selectedRemote, transfers, formatSize,
  onPathChange, onOpen, onRefresh, onUpload, onDownload, onYazi, pinnedPath, pinned, onPinCurrentPath, onGoPinnedPath,
  onPauseTransfer, onResumeTransfer, onCancelTransfer, onDismissTransfer, t,
}: Props) {
  const parts = currentPath.split('/').filter(Boolean)
  const crumbs = ['/', ...parts]
  const goUp = () => {
    if (currentPath === '/') return
    const parent = [...parts]
    parent.pop()
    onPathChange(parent.length === 0 ? '/' : `/${parent.join('/')}`)
  }

  return (
    <section className="terminal-card files-card">
      <div className="terminal-head">
        <span>▱ {currentPath}</span>
        <span className="terminal-tools">{t('sftpStreamingResume')}</span>
      </div>
      <div className="files-toolbar">
        <div className="crumbs">
          <button className="crumb" onClick={goUp} disabled={currentPath === '/'} title={t('parentDirectory')} aria-label={t('parentDirectory')} style={{ opacity: currentPath === '/' ? 0.4 : 1 }}>..</button>
          {crumbs.map((part, index) => (
            <span key={`${part}-${index}`} style={{ display: 'inline-flex', alignItems: 'center', gap: 2 }}>
              <button className="crumb" onClick={() => onPathChange(index === 0 ? '/' : `/${parts.slice(0, index).join('/')}`)}>{part}</button>
              {index < crumbs.length - 1 && <span className="crumb-sep">/</span>}
            </span>
          ))}
        </div>
        <div className="files-actions">
          <button className="mini" onClick={onRefresh}><Icon name="refresh" size={11} />{t('refresh')}</button>
          <button className="mini" onClick={onUpload}><Icon name="upload" size={11} />{t('upload')}</button>
          <button className="mini" disabled={!selectedRemote} onClick={onDownload}><Icon name="download" size={11} />{t('download')}</button>
          {pinned ? <button className="mini active" onClick={onPinCurrentPath} title={t('updatePinnedDirectory')}><Icon name="link" size={11} />{t('pinned')}</button> : <><button className="mini" onClick={onGoPinnedPath} title={t('returnToPinnedDirectory', pinnedPath)}><Icon name="folder" size={11} />{t('returnToPinned')}</button><button className="mini" onClick={onPinCurrentPath} title={t('pinCurrentDirectoryHint')}><Icon name="link" size={11} />{t('pinCurrent')}</button></>}
          <button className="mini" onClick={onYazi} title={t('openYazi')}><Icon name="folder" size={11} />Yazi</button>
        </div>
      </div>
      {loading ? (
        <div className="files-body"><div className="runtime-empty">{t('loading')}</div></div>
      ) : entries.length === 0 ? (
        <div className="files-body"><div className="runtime-empty">{t('emptyDirectory')}</div></div>
      ) : (
        <VirtualFileList entries={entries} currentPath={currentPath} selectedRemote={selectedRemote} formatSize={formatSize} onOpen={onOpen} t={t} />
      )}
      {Object.keys(transfers).length > 0 && (
        <div className="transfers">
          {Object.entries(transfers).map(([id, info]) => (
            <div className="transfer-row" key={id}>
              <span className={`transfer-status ${info.status.toLowerCase()}`}>{info.status}</span>
              {info.total ? <div className="transfer-bar"><div className="transfer-fill" style={{ width: `${Math.min(100, (info.transferred / info.total) * 100)}%` }} /></div> : <span className="transfer-meta">{formatSize(info.transferred)}</span>}
              {info.message && <span className="transfer-meta">{info.message}</span>}
              {['Completed', 'Failed', 'Cancelled'].includes(info.status) ? (
                <button className="mini" onClick={() => onDismissTransfer(id)}><Icon name="close" size={10} />{t('clear')}</button>
              ) : <>
                <button className="mini" onClick={() => onPauseTransfer(id)}><Icon name="pause" size={10} />{t('pause')}</button>
                <button className="mini" onClick={() => onResumeTransfer(id)}><Icon name="play" size={10} />{t('resume')}</button>
                <button className="mini danger" onClick={() => onCancelTransfer(id)}><Icon name="stop" size={10} />{t('cancel')}</button>
              </>}
            </div>
          ))}
        </div>
      )}
    </section>
  )
}
