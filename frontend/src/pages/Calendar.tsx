import { ChevronLeft, ChevronRight, Plus } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { useToast } from '../components/toast'
import { Link } from 'react-router-dom'
import { api, type Post, type TargetStatus } from '../api'
import PostCard from '../components/PostCard'
import { Modal } from '../components/ui'
import { useStore } from '../store'

const WEEK = ['一', '二', '三', '四', '五', '六', '日']

const dayKey = (d: Date) => `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`

/** Worst status first, so a day with a failure is visibly red. */
function postStatus(p: Post): TargetStatus {
  const order: TargetStatus[] = ['failed', 'running', 'pending', 'success', 'canceled']
  return order.find((s) => p.targets.some((t) => t.status === s)) ?? 'canceled'
}

export default function Calendar() {
  const { platform } = useStore()
  const toast = useToast()
  const [month, setMonth] = useState(() => {
    const d = new Date()
    return new Date(d.getFullYear(), d.getMonth(), 1)
  })
  const [posts, setPosts] = useState<Post[]>([])
  const [open, setOpen] = useState<Post | null>(null)
  const [dragOver, setDragOver] = useState<string | null>(null)

  // Grid starts on the Monday on/before the 1st and spans 6 weeks.
  const days = useMemo(() => {
    const start = new Date(month)
    start.setDate(1 - ((month.getDay() + 6) % 7))
    return Array.from({ length: 42 }, (_, i) => new Date(start.getFullYear(), start.getMonth(), start.getDate() + i))
  }, [month])

  const load = useCallback(() => {
    const from = days[0]
    const to = new Date(days[41].getFullYear(), days[41].getMonth(), days[41].getDate() + 1)
    api
      .posts(from.toISOString(), to.toISOString())
      .then((p) => {
        setPosts(p)
        setOpen((o) => (o ? (p.find((x) => x.id === o.id) ?? null) : null))
      })
      .catch(() => undefined)
  }, [days])

  useEffect(load, [load])

  const byDay = useMemo(() => {
    const m = new Map<string, Post[]>()
    for (const p of posts) {
      const k = dayKey(new Date(p.scheduled_at ?? p.created_at))
      m.set(k, [...(m.get(k) ?? []), p])
    }
    for (const list of m.values()) list.sort((a, b) => (a.scheduled_at ?? a.created_at).localeCompare(b.scheduled_at ?? b.created_at))
    return m
  }, [posts])

  async function drop(day: Date, postId: string) {
    setDragOver(null)
    const p = posts.find((x) => x.id === postId)
    if (!p?.scheduled_at) return
    const old = new Date(p.scheduled_at)
    const next = new Date(day.getFullYear(), day.getMonth(), day.getDate(), old.getHours(), old.getMinutes())
    if (next.getTime() < Date.now()) return toast('error', '不能改到过去的时间')
    try {
      await api.reschedule(p.id, next.toISOString())
      toast('success', '已改期', `${next.getMonth() + 1}月${next.getDate()}日 ${next.toTimeString().slice(0, 5)}`)
      load()
    } catch (e) {
      toast('error', '改期失败', (e as Error).message)
    }
  }

  const today = dayKey(new Date())
  const shift = (n: number) => setMonth((m) => new Date(m.getFullYear(), m.getMonth() + n, 1))

  return (
    <div className="page">
      <header className="page-head row">
        <div>
          <h1>发布日历</h1>
          <p className="muted">拖动待发布的定时任务到其他日期即可改期（保留原来的时刻）。</p>
        </div>
        <div className="actions">
          <button className="ghost icon-only" onClick={() => shift(-1)} aria-label="上个月">
            <ChevronLeft size={16} />
          </button>
          <b className="month-label">
            {month.getFullYear()} 年 {month.getMonth() + 1} 月
          </b>
          <button className="ghost icon-only" onClick={() => shift(1)} aria-label="下个月">
            <ChevronRight size={16} />
          </button>
          <button className="ghost" onClick={() => setMonth(new Date(new Date().getFullYear(), new Date().getMonth(), 1))}>
            今天
          </button>
          <Link to="/compose" className="button primary">
            <Plus size={16} /> 新发布
          </Link>
        </div>
      </header>

      <div className="calendar">
        {WEEK.map((w) => (
          <div key={w} className="cal-head">
            周{w}
          </div>
        ))}
        {days.map((d) => {
          const k = dayKey(d)
          const list = byDay.get(k) ?? []
          return (
            <div
              key={k}
              className={['cal-day', d.getMonth() !== month.getMonth() && 'other', k === today && 'today', dragOver === k && 'drag'].filter(Boolean).join(' ')}
              onDragOver={(e) => {
                e.preventDefault()
                setDragOver(k)
              }}
              onDragLeave={() => setDragOver((v) => (v === k ? null : v))}
              onDrop={(e) => drop(d, e.dataTransfer.getData('text/post'))}
            >
              <div className="cal-date">{d.getDate()}</div>
              {list.map((p) => {
                const movable = !!p.scheduled_at && p.targets.some((t) => t.status === 'pending')
                const t = new Date(p.scheduled_at ?? p.created_at)
                return (
                  <button
                    key={p.id}
                    className={`cal-chip st-${postStatus(p)}`}
                    draggable={movable}
                    onDragStart={(e) => e.dataTransfer.setData('text/post', p.id)}
                    onClick={() => setOpen(p)}
                    title={p.title ?? p.text}
                  >
                    <span className="cal-time">{t.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })}</span>
                    {[...new Set(p.targets.map((x) => x.platform))].map((id) => (
                      <i key={id} style={{ background: platform(id)?.color }} />
                    ))}
                    <span className="cal-title">{p.title || p.text || '(无文字)'}</span>
                  </button>
                )
              })}
            </div>
          )
        })}
      </div>

      {open && (
        <Modal title="发布详情" onClose={() => setOpen(null)} wide>
          <PostCard post={open} onChange={load} />
        </Modal>
      )}
    </div>
  )
}
