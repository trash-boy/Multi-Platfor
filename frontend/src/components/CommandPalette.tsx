import { CornerDownLeft, Monitor, Moon, Search, Sun, type LucideIcon } from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { NAV } from '../nav'
import type { ThemePref } from '../theme'

type Cmd = { id: string; label: string; hint?: string; icon: LucideIcon; group: string; run: () => void }

/** ⌘K / Ctrl+K palette: jump to pages, search all platforms, switch theme. */
export default function CommandPalette({ open, onClose, setTheme }: { open: boolean; onClose: () => void; setTheme: (t: ThemePref) => void }) {
  const nav = useNavigate()
  const [q, setQ] = useState('')
  const [idx, setIdx] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (open) {
      setQ('')
      setIdx(0)
      window.setTimeout(() => inputRef.current?.focus(), 0)
    }
  }, [open])

  const cmds = useMemo<Cmd[]>(() => {
    const go = (to: string) => () => nav(to)
    const base: Cmd[] = [
      ...NAV.map((n) => ({ id: n.to, label: n.label, hint: n.desc, icon: n.icon, group: '跳转', run: go(n.to) })),
      { id: 't-light', label: '浅色主题', icon: Sun, group: '外观', run: () => setTheme('light') },
      { id: 't-dark', label: '深色主题', icon: Moon, group: '外观', run: () => setTheme('dark') },
      { id: 't-sys', label: '跟随系统', icon: Monitor, group: '外观', run: () => setTheme('system') },
    ]
    const kw = q.trim().toLowerCase()
    const hits = kw ? base.filter((c) => (c.label + (c.hint ?? '')).toLowerCase().includes(kw)) : base
    if (q.trim()) {
      hits.unshift({
        id: 'search',
        label: `在所有平台搜索「${q.trim()}」`,
        icon: Search,
        group: '搜索',
        run: () => nav(`/discover?q=${encodeURIComponent(q.trim())}`),
      })
    }
    return hits
  }, [q, nav, setTheme])

  if (!open) return null

  const run = (c: Cmd | undefined) => {
    if (!c) return
    c.run()
    onClose()
  }

  let lastGroup = ''
  return (
    <div className="modal-backdrop cmdk-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="cmdk" role="dialog" aria-label="命令面板">
        <div className="cmdk-input">
          <Search size={16} />
          <input
            ref={inputRef}
            value={q}
            placeholder="搜索内容，或跳转到页面…"
            onChange={(e) => {
              setQ(e.target.value)
              setIdx(0)
            }}
            onKeyDown={(e) => {
              if (e.key === 'ArrowDown') {
                e.preventDefault()
                setIdx((i) => Math.min(i + 1, cmds.length - 1))
              } else if (e.key === 'ArrowUp') {
                e.preventDefault()
                setIdx((i) => Math.max(i - 1, 0))
              } else if (e.key === 'Enter') {
                run(cmds[idx])
              } else if (e.key === 'Escape') {
                onClose()
              }
            }}
          />
          <kbd>Esc</kbd>
        </div>
        <ul className="cmdk-list">
          {cmds.length === 0 && <li className="cmdk-empty">没有匹配的命令</li>}
          {cmds.map((c, i) => {
            const header = c.group !== lastGroup ? c.group : null
            lastGroup = c.group
            const I = c.icon
            return (
              <li key={c.id}>
                {header && <div className="cmdk-group">{header}</div>}
                <button className={'cmdk-item' + (i === idx ? ' on' : '')} onMouseEnter={() => setIdx(i)} onClick={() => run(c)}>
                  <I size={16} />
                  <span className="grow">{c.label}</span>
                  {c.hint && <span className="cmdk-hint">{c.hint}</span>}
                  {i === idx && <CornerDownLeft size={14} className="cmdk-enter" />}
                </button>
              </li>
            )
          })}
        </ul>
      </div>
    </div>
  )
}
