import { memo, useState } from 'react'
import { translate, type Language } from '../i18n'

type Props = { language: Language; onChoose: (language: Language) => void }

export const LanguagePrompt = memo(function LanguagePrompt({ language, onChoose }: Props) {
  const [choice, setChoice] = useState(language)
  const t = (key: Parameters<typeof translate>[1], ...args: string[]) => translate(choice, key, ...args)
  return <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="language-prompt-title">
    <div className="host-modal language-prompt">
      <div className="modal-head"><div><div className="eyebrow">LANGUAGE / 语言</div><h2 id="language-prompt-title">{t('chooseLanguage')}</h2></div></div>
      <p className="modal-note">{t('chooseLanguageHint')}</p>
      <div className="language-choice-grid">
        <button type="button" className={`theme-option ${choice === 'zh-CN' ? 'selected' : ''}`} aria-pressed={choice === 'zh-CN'} onClick={() => setChoice('zh-CN')}><strong>简体中文</strong><span>中文界面</span></button>
        <button type="button" className={`theme-option ${choice === 'en-US' ? 'selected' : ''}`} aria-pressed={choice === 'en-US'} onClick={() => setChoice('en-US')}><strong>English</strong><span>English interface</span></button>
      </div>
      <div className="modal-actions"><button className="primary" type="button" onClick={() => onChoose(choice)}>{t('continue')}</button></div>
    </div>
  </div>
})
