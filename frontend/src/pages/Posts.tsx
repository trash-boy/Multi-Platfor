import { useCallback, useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { api, type Post, type TargetStatus } from '../api'
import PostCard from '../components/PostCard'
import { Plus } from 'lucide-react'
import { Empty, SkeletonList } from '../components/ui'

const FILTERS: { key: 'all' | TargetStatus; label: string }[] = [
  { key: 'all', label: '全部' },
  { key: 'pending', label: '待发布' },
  { key: 'running', label: '发布中' },
  { key: 'success', label: '已发布' },
  { key: 'failed', label: '失败' },
]

export default function Posts() {
  const [posts, setPosts] = useState<Post[] | null>(null)
  const [filter, setFilter] = useState<'all' | TargetStatus>('all')
  const [err, setErr] = useState<string | null>(null)

  const load = useCallback(() => {
    api
      .posts()
      .then((p) => {
        setPosts(p)
        setErr(null)
      })
      .catch((e) => setErr((e as Error).message))
  }, [])

  useEffect(load, [load])

  // Poll while anything is still in flight.
  const active = posts?.some((p) => p.targets.some((t) => t.status === 'pending' || t.status === 'running'))
  useEffect(() => {
    if (!active) return
    const t = window.setInterval(load, 4000)
    return () => window.clearInterval(t)
  }, [active, load])

  const shown = posts?.filter((p) => filter === 'all' || p.targets.some((t) => t.status === filter)) ?? []

  return (
    <div className="page">
      <header className="page-head row">
        <div>
          <h1>发布记录</h1>
          <p className="muted">每个平台、每个账号单独记录状态。失败任务不会自动重发，以免重复发布，确认后可手动重试。</p>
        </div>
        <Link to="/compose" className="button primary">
          <Plus size={16} /> 新发布
        </Link>
      </header>
      <div className="tabs">
        {FILTERS.map((f) => (
          <button key={f.key} className={filter === f.key ? 'on' : ''} onClick={() => setFilter(f.key)}>
            {f.label}
          </button>
        ))}
      </div>
      {err && <div className="banner error">{err}</div>}
      {posts === null ? (
        <SkeletonList rows={3} h={150} />
      ) : shown.length === 0 ? (
        <Empty>暂无记录</Empty>
      ) : (
        <div className="post-list">
          {shown.map((p) => (
            <PostCard key={p.id} post={p} onChange={load} />
          ))}
        </div>
      )}
    </div>
  )
}
