import { Clock, ExternalLink, RotateCcw, Trash2, Ban, CalendarClock } from 'lucide-react'
import { useState } from 'react'
import { useToast } from './toast'
import { api, fmtTime, fromLocalInput, mediaUrl, PROVIDER_LABEL, toLocalInput, type Post, type Target } from '../api'
import { useStore } from '../store'
import { PlatformBadge, StatusBadge } from './ui'

/** One post with its per-platform targets and actions. */
export default function PostCard({ post, onChange }: { post: Post; onChange: () => void }) {
  const { platform, accounts } = useStore()
  const providersFor = (t: Target) => accounts.find((a) => a.platform === t.platform && a.account === t.account)?.providers ?? []
  const toast = useToast()
  const [busy, setBusy] = useState(false)
  const [editing, setEditing] = useState(false)
  const [when, setWhen] = useState(toLocalInput(post.scheduled_at))

  const pending = post.targets.some((t) => t.status === 'pending')
  const running = post.targets.some((t) => t.status === 'running')

  const run = async (f: () => Promise<unknown>) => {
    setBusy(true)
    try {
      await f()
      onChange()
    } catch (e) {
      toast('error', '操作失败', (e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <article className="card post-card">
      <header>
        <div>
          {post.scheduled_at ? (
            <span className="tag time"><Clock size={12} /> 定时 {fmtTime(post.scheduled_at)}</span>
          ) : (
            <span className="tag">创建于 {fmtTime(post.created_at)}</span>
          )}
        </div>
        <div className="actions">
          {pending && (
            <>
              <button className="ghost small" disabled={busy} onClick={() => setEditing((v) => !v)}>
                <CalendarClock size={14} /> 改时间
              </button>
              <button className="ghost small" disabled={busy} onClick={() => run(() => api.cancelPost(post.id))}>
                <Ban size={14} /> 取消未发布
              </button>
            </>
          )}
          {!running && (
            <button
              className="ghost small danger"
              disabled={busy}
              onClick={() => confirm('删除这条发布记录？已经发出去的内容不会被删除。') && run(() => api.deletePost(post.id))}
            >
              <Trash2 size={14} /> 删除记录
            </button>
          )}
        </div>
      </header>

      {editing && (
        <div className="inline-form">
          <input type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} />
          <button
            className="small"
            disabled={busy}
            onClick={() =>
              run(async () => {
                await api.reschedule(post.id, fromLocalInput(when))
                setEditing(false)
              })
            }
          >
            保存
          </button>
          <button
            className="ghost small"
            onClick={() =>
              run(async () => {
                await api.reschedule(post.id, null)
                setEditing(false)
              })
            }
          >
            改为立即发布
          </button>
        </div>
      )}

      <div className="post-body">
        {(post.images[0] || post.cover) && <img className="post-thumb" src={mediaUrl((post.images[0] ?? post.cover)!.url)} alt="" />}
        {post.video && !post.cover && <video className="post-thumb" src={mediaUrl(post.video.url)} muted />}
        <div className="grow">
          {post.title && <h3>{post.title}</h3>}
          <p className="post-text">{post.text || <span className="muted">(无正文)</span>}</p>
          <p className="muted small">
            {post.images.length > 0 && `${post.images.length} 张图 `}
            {post.video && '1 个视频 '}
            {post.tags.length > 0 && '标签：' + post.tags.join('、')}
          </p>
        </div>
      </div>

      <ul className="targets">
        {post.targets.map((t) => (
          <li key={t.id}>
            <PlatformBadge p={platform(t.platform)} small />
            <span className="muted small">· {t.account}</span>
            <StatusBadge status={t.status} />
            {t.status === 'pending' && t.attempts > 0 && <span className="muted small">将在 {fmtTime(t.run_at)} 重试</span>}
            {t.item_url && (
              <a href={t.item_url} target="_blank" rel="noreferrer" className="small">
                查看 <ExternalLink size={12} />
              </a>
            )}
            {t.provider && t.status === 'success' && <span className="tag">via {PROVIDER_LABEL[t.provider] ?? t.provider}</span>}
            {t.force_provider && t.status === 'pending' && <span className="tag">指定通道 {PROVIDER_LABEL[t.force_provider]}</span>}
            {(t.status === 'failed' || t.status === 'canceled') && (
              <>
                <button className="ghost small" disabled={busy} onClick={() => run(() => api.retryTarget(t.id))}>
                  <RotateCcw size={13} /> 重试
                </button>
                {providersFor(t).length > 1 && (
                  <select
                    className="small-select"
                    value=""
                    disabled={busy}
                    onChange={(e) => e.target.value && run(() => api.retryTarget(t.id, e.target.value))}
                  >
                    <option value="">换通道重试…</option>
                    {providersFor(t).map((p) => (
                      <option key={p} value={p}>
                        {PROVIDER_LABEL[p] ?? p}
                      </option>
                    ))}
                  </select>
                )}
              </>
            )}
            {t.attempt_log.length > 1 && (
              <details className="attempts">
                <summary className="small muted">{t.attempt_log.length} 次尝试</summary>
                <ol>
                  {t.attempt_log.map((a, i) => (
                    <li key={i} className="small">
                      {fmtTime(a.at)} {PROVIDER_LABEL[a.provider] ?? a.provider}：{a.ok ? '✓ 成功' : `✗ ${a.code} ${a.message ?? ''}`}
                    </li>
                  ))}
                </ol>
              </details>
            )}
            {t.last_error && <div className="target-error">{t.last_error}</div>}
          </li>
        ))}
      </ul>
    </article>
  )
}
