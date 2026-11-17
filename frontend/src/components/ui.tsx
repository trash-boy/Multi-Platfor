import { Inbox, X } from 'lucide-react'
import { useEffect, type ReactNode } from 'react'
import { STATUS_LABEL, type Platform, type TargetStatus } from '../api'

/** App-icon style glyphs; brand colors come from the platform registry. */
const GLYPH: Record<string, string> = {
  xhs: '红',
  xianyu: '鱼',
  x: '𝕏',
  weibo: '微',
  douyin: '抖',
  bilibili: 'B',
  kuaishou: '快',
  tencent: '视',
}
const DARK_GLYPH = new Set(['xianyu'])

export function PlatformIcon({ p, size = 22 }: { p: Platform | undefined; size?: number }) {
  if (!p) return null
  return (
    <span
      className="picon"
      style={{ width: size, height: size, fontSize: size * 0.5, background: p.color, color: DARK_GLYPH.has(p.id) ? '#1f1f1f' : '#fff' }}
      aria-hidden
    >
      {GLYPH[p.id] ?? p.name.slice(0, 1)}
    </span>
  )
}

export function PlatformBadge({ p, small }: { p: Platform | undefined; small?: boolean }) {
  if (!p) return null
  return (
    <span className={'pbadge' + (small ? ' small' : '')}>
      <PlatformIcon p={p} size={small ? 18 : 22} />
      {p.name}
    </span>
  )
}

export function StatusBadge({ status }: { status: TargetStatus }) {
  return (
    <span className={`status status-${status}`}>
      <i />
      {STATUS_LABEL[status]}
    </span>
  )
}

export function Modal({ title, onClose, children, wide }: { title: string; onClose: () => void; children: ReactNode; wide?: boolean }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={'modal' + (wide ? ' wide' : '')} role="dialog" aria-label={title}>
        <header>
          <h3>{title}</h3>
          <button className="icon-btn" onClick={onClose} aria-label="关闭">
            <X size={16} />
          </button>
        </header>
        <div className="modal-body">{children}</div>
      </div>
    </div>
  )
}

export function Avatar({ src, name, size = 28 }: { src?: string | null; name?: string | null; size?: number }) {
  return src ? (
    <img className="avatar" src={src} width={size} height={size} referrerPolicy="no-referrer" alt="" />
  ) : (
    <span className="avatar placeholder" style={{ width: size, height: size, fontSize: size * 0.42 }}>
      {(name ?? '?').slice(0, 1).toUpperCase()}
    </span>
  )
}

export function Empty({ children, icon }: { children: ReactNode; icon?: ReactNode }) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon ?? <Inbox size={22} />}</div>
      <div>{children}</div>
    </div>
  )
}

export function Skeleton({ h = 16, w = '100%', r = 6 }: { h?: number; w?: number | string; r?: number }) {
  return <span className="skeleton" style={{ height: h, width: w, borderRadius: r }} />
}

export function SkeletonList({ rows = 3, h = 72 }: { rows?: number; h?: number }) {
  return (
    <div className="stack">
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} h={h} r={12} />
      ))}
    </div>
  )
}

export function PageHeader({ title, desc, actions }: { title: string; desc?: ReactNode; actions?: ReactNode }) {
  return (
    <header className="page-head">
      <div>
        <h1>{title}</h1>
        {desc && <p>{desc}</p>}
      </div>
      {actions && <div className="actions">{actions}</div>}
    </header>
  )
}
