import { memo } from 'react'
import type { UpdateCheck } from '../api'
import { Icon } from '../icons'
import type { AccentColor, ThemeMode, ThemePreference } from '../theme'

type Props = {
  autoStart: boolean
  updateCheck: UpdateCheck | null
  updateBusy: boolean
  version: string
  onClose: () => void
  onAutoStart: (enabled: boolean) => void
  onCheck: () => void
  onInstall: () => void
  theme: ThemePreference
  onThemeChange: (theme: ThemePreference) => void
}

export const SettingsPanel = memo(function SettingsPanel({ autoStart, updateCheck, updateBusy, version, onClose, onAutoStart, onCheck, onInstall, theme, onThemeChange }: Props) {
  const modes: Array<[ThemeMode, string, string]> = [['system', '跟随系统', '根据系统外观自动切换'], ['dark', '黑色', '低亮度深色工作台'], ['light', '白色', '明亮高对比界面']]
  const accents: Array<[AccentColor, string]> = [['amber', '琥珀'], ['blue', '蓝色'], ['green', '绿色'], ['purple', '紫色'], ['rose', '玫红'], ['cyan', '青色']]
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="settings-title"><div className="host-modal"><div className="modal-head"><div><div className="eyebrow">SETTINGS</div><h2 id="settings-title">设置</h2></div><button type="button" className="ghost" onClick={onClose}>关闭</button></div><section className="theme-section" aria-labelledby="theme-title"><div className="config-section-title" id="theme-title">外观主题 <span>仅保存在本机界面偏好中</span></div><div className="theme-mode-grid">{modes.map(([value, label, hint]) => <button key={value} type="button" className={`theme-option ${theme.mode === value ? 'selected' : ''}`} aria-pressed={theme.mode === value} onClick={() => onThemeChange({ ...theme, mode: value })}><strong>{label}</strong><span>{hint}</span></button>)}</div><div className="accent-grid" role="group" aria-label="强调色">{accents.map(([value, label]) => <button key={value} type="button" className={`accent-option ${theme.accent === value ? 'selected' : ''}`} aria-label={`强调色：${label}`} aria-pressed={theme.accent === value} onClick={() => onThemeChange({ ...theme, accent: value })}><span className={`accent-swatch ${value}`} />{label}</button>)}</div></section><label className="toggle-row"><input type="checkbox" checked={autoStart} onChange={(event) => onAutoStart(event.target.checked)} />登录时自动启动 Kodework（托盘常驻）</label><div className="update-row"><div className="update-info"><strong>自动更新</strong><span className="modal-note">当前版本 v{version} · 签名更新（tauri-plugin-updater）</span>{updateCheck?.status === 'available' && <span className="update-state">发现新版本 v{updateCheck.version}</span>}{updateCheck?.status === 'up-to-date' && <span className="update-state">已是最新版本</span>}{updateCheck?.status === 'error' && <span className="update-state error">检查失败：{updateCheck.error}</span>}{updateCheck?.status === 'unsupported' && <span className="update-state">浏览器预览模式不支持自动更新</span>}</div><div className="update-actions">{updateCheck?.status === 'available' ? <button className="primary" disabled={updateBusy} onClick={onInstall}>下载并安装</button> : <button className="mini" disabled={updateBusy} onClick={onCheck}>{updateBusy ? '检查中…' : '检查更新'}</button>}</div></div><p className="modal-note">密码仅在连接时一次性输入并即时清零，不写入本机 SQLite 或日志。</p><div className="security-note"><Icon name="check" size={13} />更新包必须通过 Tauri updater 签名验证；Windows 正式分发还需要 Authenticode。</div></div></div>
})
