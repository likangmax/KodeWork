import { memo } from 'react'
import type { UpdateCheck } from '../api'
import { Icon } from '../icons'
import type { AccentColor, ThemeMode, ThemePreference } from '../theme'
import { translate, type Language } from '../i18n'

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
  language: Language
  onLanguageChange: (language: Language) => void
}

export const SettingsPanel = memo(function SettingsPanel({ autoStart, updateCheck, updateBusy, version, onClose, onAutoStart, onCheck, onInstall, theme, onThemeChange, language, onLanguageChange }: Props) {
  const t = (key: Parameters<typeof translate>[1], ...args: string[]) => translate(language, key, ...args)
  const modes: Array<[ThemeMode, string, string]> = [['system', t('followSystem'), t('followSystemHint')], ['dark', t('dark'), t('darkHint')], ['light', t('light'), t('lightHint')]]
  const accents: Array<[AccentColor, string]> = [['amber', language === 'zh-CN' ? '琥珀' : 'Amber'], ['blue', language === 'zh-CN' ? '蓝色' : 'Blue'], ['green', language === 'zh-CN' ? '绿色' : 'Green'], ['purple', language === 'zh-CN' ? '紫色' : 'Purple'], ['rose', language === 'zh-CN' ? '玫红' : 'Rose'], ['cyan', language === 'zh-CN' ? '青色' : 'Cyan']]
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="settings-title"><div className="host-modal"><div className="modal-head"><div><div className="eyebrow">SETTINGS</div><h2 id="settings-title">{t('settings')}</h2></div><button type="button" className="ghost" onClick={onClose}>{t('close')}</button></div><section className="theme-section" aria-labelledby="language-title"><div className="config-section-title" id="language-title">{t('language')}</div><div className="language-inline"><button type="button" className={`mini ${language === 'zh-CN' ? 'selected' : ''}`} onClick={() => onLanguageChange('zh-CN')}>{t('chinese')}</button><button type="button" className={`mini ${language === 'en-US' ? 'selected' : ''}`} onClick={() => onLanguageChange('en-US')}>{t('english')}</button></div></section><section className="theme-section" aria-labelledby="theme-title"><div className="config-section-title" id="theme-title">{t('appearance')} <span>{t('localOnly')}</span></div><div className="theme-mode-grid">{modes.map(([value, label, hint]) => <button key={value} type="button" className={`theme-option ${theme.mode === value ? 'selected' : ''}`} aria-pressed={theme.mode === value} onClick={() => onThemeChange({ ...theme, mode: value })}><strong>{label}</strong><span>{hint}</span></button>)}</div><div className="accent-grid" role="group" aria-label={t('accent')}>{accents.map(([value, label]) => <button key={value} type="button" className={`accent-option ${theme.accent === value ? 'selected' : ''}`} aria-label={`${t('accent')}: ${label}`} aria-pressed={theme.accent === value} onClick={() => onThemeChange({ ...theme, accent: value })}><span className={`accent-swatch ${value}`} />{label}</button>)}</div></section><label className="toggle-row"><input type="checkbox" checked={autoStart} onChange={(event) => onAutoStart(event.target.checked)} />{t('autoStart')}</label><div className="update-row"><div className="update-info"><strong>{t('updates')}</strong><span className="modal-note">{t('currentVersion', version)}</span>{updateCheck?.status === 'available' && <span className="update-state">{t('found', updateCheck.version)}</span>}{updateCheck?.status === 'up-to-date' && <span className="update-state">{t('latest')}</span>}{updateCheck?.status === 'error' && <span className="update-state error">{t('checkFailed', updateCheck.error)}</span>}{updateCheck?.status === 'unsupported' && <span className="update-state">{t('unsupported')}</span>}</div><div className="update-actions">{updateCheck?.status === 'available' ? <button className="primary" disabled={updateBusy} onClick={onInstall}>{t('install')}</button> : <button className="mini" disabled={updateBusy} onClick={onCheck}>{updateBusy ? t('checking') : t('check')}</button>}</div></div><p className="modal-note">{t('security')}</p><div className="security-note"><Icon name="check" size={13} />{t('updateSecurity')}</div></div></div>
})
