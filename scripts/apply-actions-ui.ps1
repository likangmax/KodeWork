$ErrorActionPreference = "Stop"
$p = "D:\OneDrive\AAA_KK\MYCODE\redock-windows\src\App.tsx"
$c = [System.IO.File]::ReadAllText($p).Replace("`r`n", "`n")
$old = @'
import type { HerdrAgentInfo, Host, HostKeyRequest, RemoteFileMeta, Snippet, TmuxSession, TunnelInfo } from './api'
'@
$new = @'
import type { Action, HerdrAgentInfo, Host, HostKeyRequest, Project, RemoteFileMeta, RunOutcome, Snippet, TmuxSession, TunnelInfo } from './api'
'@
if (-not $c.Contains($old)) { throw "ANCHOR MISSING: " + $old.Substring(0, [Math]::Min(60, $old.Length)) }
$c = $c.Replace($old, $new)
$old = @'
  closePane, herdrAgents, herdrAttach, herdrBridge, herdrBridgeStop, herdrDetect, openPane, sendInput,
  autostartStatus,
'@
$new = @'
  actionDelete, actionList, actionSave,
  closePane, herdrAgents, herdrAttach, herdrBridge, herdrBridgeStop, herdrDetect, openPane,
  projectDelete, projectList, projectSave, runAction, sendInput,
  autostartStatus,
'@
if (-not $c.Contains($old)) { throw "ANCHOR MISSING: " + $old.Substring(0, [Math]::Min(60, $old.Length)) }
$c = $c.Replace($old, $new)
$old = @'
useState<'terminal' | 'files' | 'preview'>('terminal')
'@
$new = @'
useState<'terminal' | 'files' | 'preview' | 'actions'>('terminal')
'@
if (-not $c.Contains($old)) { throw "ANCHOR MISSING: " + $old.Substring(0, [Math]::Min(60, $old.Length)) }
$c = $c.Replace($old, $new)
$old = @'
          <button className={activeTab === 'preview' ? 'active' : ''} onClick={() => setActiveTab('preview')} disabled={listeningTunnels.length === 0 && !previewUrl}><Icon name="globe" size={13} />预览</button>
'@
$new = @'
          <button className={activeTab === 'preview' ? 'active' : ''} onClick={() => setActiveTab('preview')} disabled={listeningTunnels.length === 0 && !previewUrl}><Icon name="globe" size={13} />预览</button>
          <button className={activeTab === 'actions' ? 'active' : ''} onClick={() => setActiveTab('actions')}><Icon name="activity" size={13} />活动</button>
'@
if (-not $c.Contains($old)) { throw "ANCHOR MISSING: " + $old.Substring(0, [Math]::Min(60, $old.Length)) }
$c = $c.Replace($old, $new)
$old = @'
  const [snippetDraft, setSnippetDraft] = useState<Snippet | null>(null)
'@
$new = @'
  const [snippetDraft, setSnippetDraft] = useState<Snippet | null>(null)
  const [projects, setProjects] = useState<Project[]>([])
  const [actionsByProject, setActionsByProject] = useState<Record<string, Action[]>>({})
  const [projectDraft, setProjectDraft] = useState<Project | null>(null)
  const [actionDraft, setActionDraft] = useState<Action | null>(null)
  const [runResult, setRunResult] = useState<RunOutcome | null>(null)
  const [confirmAction, setConfirmAction] = useState<Action | null>(null)
'@
if (-not $c.Contains($old)) { throw "ANCHOR MISSING: " + $old.Substring(0, [Math]::Min(60, $old.Length)) }
$c = $c.Replace($old, $new)
$old = @'
  // ---- file panel ----
'@
$new = @'
  // ---- workspace controls ----
  useEffect(() => {
    if (!isDesktop() || !selectedId) return
    void projectList(selectedId).then(setProjects).catch(() => {})
  }, [selectedId, activeTab])

  const refreshActions = async (projectId: string) => {
    try {
      const actions = await actionList(projectId)
      setActionsByProject((map) => ({ ...map, [projectId]: actions }))
    } catch { /* transient */ }
  }

  useEffect(() => {
    for (const project of projects) void refreshActions(project.id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projects])

  const onProjectSave = async () => {
    if (!projectDraft || !selectedId) return
    try {
      await projectSave(projectDraft)
      setProjects(await projectList(selectedId))
      setProjectDraft(null)
    } catch (error) {
      setMessage('项目保存失败：' + String(error))
    }
  }

  const onActionSave = async () => {
    if (!actionDraft) return
    try {
      await actionSave(actionDraft)
      await refreshActions(actionDraft.project_id)
      setActionDraft(null)
    } catch (error) {
      setMessage('动作保存失败：' + String(error))
    }
  }

  const onRunAction = async (action: Action) => {
    if (!selected) return
    if (action.danger_level === 'Dangerous' && action.confirmation !== 'Never') {
      setConfirmAction(action)
      return
    }
    void executeAction(action, false)
  }

  const executeAction = async (action: Action, confirmed: boolean) => {
    if (!selected) return
    try {
      const outcome = await runAction(selected.id, action, confirmed)
      setRunResult(outcome)
      setMessage('动作完成：退出码 ' + String(outcome.exit_code ?? '交互式'))
    } catch (error) {
      setMessage('动作失败：' + String(error))
    }
  }

  // ---- file panel ----
'@
if (-not $c.Contains($old)) { throw "ANCHOR MISSING: " + $old.Substring(0, [Math]::Min(60, $old.Length)) }
$c = $c.Replace($old, $new)
$old = @'
          {activeTab === 'preview' ? (
'@
$new = @'
          {activeTab === 'actions' ? (
            <section className="terminal-card files-card">
              <div className="terminal-head">
                <span>◷ 项目与动作</span>
                <span className="terminal-tools">Quick · Interactive · Background</span>
              </div>
              <div className="files-body">
                {projects.length === 0 && <div className="runtime-empty">还没有项目；点击“新建项目”创建</div>}
                {projects.map((project) => (
                  <div key={project.id} style={{ marginBottom: 10 }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '6px 4px' }}>
                      <Icon name="folder" size={12} />
                      <span style={{ fontWeight: 600, fontSize: 13 }}>{project.name}</span>
                      <span className="tunnel-remote">{project.remote_cwd}</span>
                      <span style={{ marginLeft: 'auto', display: 'flex', gap: 6 }}>
                        <button className="mini" onClick={() => setProjectDraft({ ...project })}>编辑</button>
                        <button className="mini danger" onClick={() => void projectDelete(project.id).then(() => setProjects((items) => items.filter((p) => p.id !== project.id))).catch((e) => setMessage('删除失败：' + String(e)))}>删除</button>
                        <button className="mini" onClick={() => setActionDraft({ id: crypto.randomUUID(), project_id: project.id, name: '', command: '', mode: 'Quick', cwd: project.remote_cwd, timeout_ms: 60000, danger_level: 'Safe', confirmation: 'Never', env: {} })}>＋动作</button>
                      </span>
                    </div>
                    {(actionsByProject[project.id] ?? []).map((action) => (
                      <div className="tunnel-row" key={action.id} style={{ paddingLeft: 20 }}>
                        <span style={{ fontWeight: 600, flex: 1, minWidth: 0, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{action.name}</span>
                        <span className="tunnel-state" style={action.danger_level === 'Dangerous' ? { color: 'var(--danger)' } : {}}>{action.danger_level}</span>
                        <span className="tunnel-conns">{action.mode}</span>
                        <button className="mini" onClick={() => void onRunAction(action)}><Icon name="play" size={10} />运行</button>
                        <button className="mini" onClick={() => setActionDraft({ ...action })}>编辑</button>
                        <button className="mini danger" onClick={() => void actionDelete(action.id).then(() => refreshActions(project.id)).catch((e) => setMessage('删除失败：' + String(e)))}><Icon name="trash" size={10} />删除</button>
                      </div>
                    ))}
                  </div>
                ))}
                {runResult && (
                  <div style={{ marginTop: 8, padding: 10, background: 'var(--bg-inset)', border: '1px solid var(--line)', borderRadius: 5, fontFamily: 'var(--mono)', fontSize: 11, whiteSpace: 'pre-wrap', maxHeight: 160, overflowY: 'auto' }}>
                    {runResult.stdout_preview || '(无输出)'}
                    {runResult.stderr_preview && '\n[stderr] ' + runResult.stderr_preview}
                  </div>
                )}
              </div>
              <div className="files-toolbar" style={{ borderTop: '1px solid var(--line)', borderBottom: 'none' }}>
                <button className="mini" onClick={() => setProjectDraft({ id: crypto.randomUUID(), host_id: selectedId ?? '', name: '', remote_cwd: '~', preferred_runtime: selected?.default_runtime ?? 'Tmux' })}><Icon name="plus" size={11} />新建项目</button>
                {runResult && <button className="mini" onClick={() => setRunResult(null)}>清除输出</button>}
              </div>
            </section>
          ) : activeTab === 'preview' ? (
'@
if (-not $c.Contains($old)) { throw "ANCHOR MISSING: " + $old.Substring(0, [Math]::Min(60, $old.Length)) }
$c = $c.Replace($old, $new)
$old = @'
      {snippetsOpen && (
'@
$new = @'
      {projectDraft && (
        <div className="modal-backdrop" role="presentation">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">PROJECT</div><h2>编辑项目</h2></div>
              <button type="button" className="ghost" onClick={() => setProjectDraft(null)}>关闭</button>
            </div>
            <label>名称
              <input value={projectDraft.name} onChange={(e) => setProjectDraft({ ...projectDraft, name: e.target.value })} autoFocus />
            </label>
            <label>远程目录
              <input value={projectDraft.remote_cwd} onChange={(e) => setProjectDraft({ ...projectDraft, remote_cwd: e.target.value })} placeholder="~/projects/foo" />
            </label>
            <div className="modal-actions">
              <button className="ghost" onClick={() => setProjectDraft(null)}>取消</button>
              <button className="primary" onClick={() => void onProjectSave()}>保存</button>
            </div>
          </div>
        </div>
      )}

      {actionDraft && (
        <div className="modal-backdrop" role="presentation">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">ACTION</div><h2>编辑动作</h2></div>
              <button type="button" className="ghost" onClick={() => setActionDraft(null)}>关闭</button>
            </div>
            <label>名称
              <input value={actionDraft.name} onChange={(e) => setActionDraft({ ...actionDraft, name: e.target.value })} autoFocus />
            </label>
            <label>命令
              <textarea
                rows={3}
                value={actionDraft.command}
                onChange={(e) => setActionDraft({ ...actionDraft, command: e.target.value })}
                style={{ background: 'var(--bg-inset)', border: '1px solid var(--line-strong)', borderRadius: 4, padding: '7px 9px', fontSize: 12, fontFamily: 'var(--mono)', color: 'var(--text)', resize: 'vertical' }}
              />
            </label>
            <div className="form-row">
              <label>模式
                <select value={actionDraft.mode} onChange={(e) => setActionDraft({ ...actionDraft, mode: e.target.value as Action['mode'] })}>
                  <option value="Quick">Quick（快速）</option>
                  <option value="Interactive">Interactive（终端）</option>
                  <option value="Background">Background（后台）</option>
                </select>
              </label>
              <label>危险级别
                <select value={actionDraft.danger_level} onChange={(e) => setActionDraft({ ...actionDraft, danger_level: e.target.value as Action['danger_level'] })}>
                  <option value="Safe">安全</option>
                  <option value="Review">需复核</option>
                  <option value="Dangerous">危险</option>
                </select>
              </label>
            </div>
            <label>超时（毫秒，留空默认 30s）
              <input type="number" value={actionDraft.timeout_ms ?? ''} onChange={(e) => setActionDraft({ ...actionDraft, timeout_ms: e.target.value ? Number(e.target.value) : null })} />
            </label>
            <div className="modal-actions">
              <button className="ghost" onClick={() => setActionDraft(null)}>取消</button>
              <button className="primary" onClick={() => void onActionSave()}>保存</button>
            </div>
          </div>
        </div>
      )}

      {confirmAction && selected && (
        <div className="modal-backdrop" role="presentation">
          <div className="host-modal">
            <div className="modal-head">
              <div><div className="eyebrow">CONFIRM</div><h2>确认运行危险动作</h2></div>
            </div>
            <p className="modal-note">动作 <strong>{confirmAction.name}</strong> 被标记为危险，将在远程执行：</p>
            <code className="fingerprint">{confirmAction.command}</code>
            <div className="modal-actions">
              <button className="ghost" onClick={() => setConfirmAction(null)}>取消</button>
              <button className="primary" style={{ background: 'var(--danger)' }} onClick={() => { const action = confirmAction; setConfirmAction(null); void executeAction(action, true) }}>确认执行</button>
            </div>
          </div>
        </div>
      )}

      {snippetsOpen && (
'@
if (-not $c.Contains($old)) { throw "ANCHOR MISSING: " + $old.Substring(0, [Math]::Min(60, $old.Length)) }
$c = $c.Replace($old, $new)
[System.IO.File]::WriteAllText($p, $c)
"APPLIED"