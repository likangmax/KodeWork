// Kodework icon set: hand-tuned 16x16 linear icons (stroke 1.5).
// Replaces emoji/unicode glyphs so the UI keeps one consistent voice.

import type { CSSProperties, ReactNode } from "react"

const PATHS: Record<string, ReactNode> = {
  terminal: <path d="M2.5 5 6 8l-3.5 3M8.5 12.5H14" />,
  folder: <path d="M1.75 4.25h3.9l1.6 2h7V13H1.75z" />,
  activity: <path d="M1.5 8h3l2-4.5 3 9L11.5 8H14.5" />,
  plus: <path d="M8 3v10M3 8h10" />,
  close: <path d="m4 4 8 8M12 4l-8 8" />,
  upload: <path d="M8 10.5V3M4.5 6 8 2.5 11.5 6M3 12.5h10" />,
  download: <path d="M8 2.5v7.5M4.5 7 8 10.5 11.5 7M3 13.5h10" />,
  refresh: <path d="M13.6 8a5.6 5.6 0 1 1-1.7-4M13.5 2.5v3h-3" />,
  pause: <path d="M5.5 3.5v9M10.5 3.5v9" />,
  play: <path d="M5.2 3.6 12 8l-6.8 4.4z" />,
  stop: <rect x="4.5" y="4.5" width="7" height="7" rx="1" />,
  trash: <path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5 5.2 13h5.6l.7-8.5" />,
  gear: <><circle cx="8" cy="8" r="2.1" /><path d="M8 1.9v1.9M8 12.2v1.9M1.9 8h1.9M12.2 8h1.9M3.7 3.7l1.3 1.3M11 11l1.3 1.3M12.3 3.7 11 5M5 11l-1.3 1.3" /></>,
  chevron: <path d="m6 3.5 4.5 4.5L6 12.5" />,
  link: <><path d="M5.9 10.1 3.1 12.9a2.1 2.1 0 0 1-3-3l2.8-2.8M10.1 5.9l2.8-2.8a2.1 2.1 0 0 1 3 3l-2.8 2.8M6.3 9.7 9.7 6.3" /></>,
  globe: <><circle cx="8" cy="8" r="6" /><path d="M2 8h12M8 2c2.4 2 2.4 10 0 12M8 2C5.6 4 5.6 12 8 14" /></>,
  check: <path d="m3 8.5 3.5 3.5L13 4.5" />,
  alert: <><path d="M8 2.5 14.5 13.5h-13z" /><path d="M8 7v3M8 12h.01" /></>,
  server: <><rect x="2.5" y="2.5" width="11" height="4.6" rx="1" /><rect x="2.5" y="8.9" width="11" height="4.6" rx="1" /><path d="M5.4 4.8h.01M5.4 11.2h.01" /></>,
  computer: <><rect x="2" y="2.5" width="12" height="8.5" rx="1" /><path d="M6 13.5h4M8 11v2.5M4.5 13.5h7" /></>,
  zap: <path d="M9 1.5 3.5 9h4l-.5 5.5L12.5 7h-4z" />,
  eye: <><path d="M1.5 8S4 3.5 8 3.5 14.5 8 12 12.5 8 12.5 1.5 8z" /><circle cx="8" cy="8" r="2" /></>,
  power: <path d="M8 2v6M4.6 3.9a6 6 0 1 0 6.8 0" />,
  arrow_up: <path d="M8 12V4M4.5 7 8 3.5 11.5 7" />,
  arrow_down: <path d="M8 4v8M4.5 9 8 12.5 11.5 9" />,
  clipboard: <><rect x="3" y="3.5" width="10" height="11" rx="1.5" /><path d="M6 3.5V2h4v1.5M5.5 7h5M5.5 10h5" /></>,
}

type IconName = keyof typeof PATHS

export function Icon({
  name,
  size = 14,
  className = "",
  style,
}: {
  name: IconName
  size?: number
  className?: string
  style?: CSSProperties
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      style={style}
      aria-hidden="true"
    >
      {PATHS[name]}
    </svg>
  )
}
