import { BarChart3, Bookmark, ExternalLink, Eye, Heart, Loader2, MessageCircle, RefreshCw } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { Link } from 'react-router-dom'
import {
  api,
  fmtNum,
  fmtTime,
  METRIC_LABEL,
  METRICS,
  type AnalyticsSummary,
  type Metric,
  type Metrics,
  type SyncStatus,
  type TrackedItem,
} from '../api'
import { ColumnChart } from '../components/ColumnChart'
import { useToast } from '../components/toast'
import { Empty, Modal, PageHeader, PlatformIcon, Skeleton } from '../components/ui'
import { useStore } from '../store'

const KPIS: { m: Metric; icon: ReactNode }[] = [
  { m: 'views', icon: <Eye size={16} /> },
  { m: 'likes', icon: <Heart size={16} /> },
  { m: 'comments', icon: <MessageCircle size={16} /> },
  { m: 'collects', icon: <Bookmark size={16} /> },
]

export default function Analytics() {
  const { platform } = useStore()
  const toast = useToast()
  const [days, setDays] = useState(30)
  const [sum, setSum] = useState<AnalyticsSummary | null>(null)
  const [items, setItems] = useState<TrackedItem[] | null>(null)
  const [sort, setSort] = useState<string>('likes')
  const [filter, setFilter] = useState('')
  const [metric, setMetric] = useState<Metric>('likes')
  const [table, setTable] = useState(false)
  const [sync, setSync] = useState<SyncStatus | null>(null)
  const [detail, setDetail] = useState<TrackedItem | null>(null)

  const load = useCallback(() => {
    api
      .analytics(days)
      .then((s) => {
        setSum(s)
        setSync(s.sync)
      })
      .catch((e) => toast('error', '加载数据失败', (e as Error).message))
  }, [days, toast])
  useEffect(load, [load])

  const loadItems = useCallback(() => {
    api
      .trackedItems(sort, filter || undefined)
      .then(setItems)
      .catch(() => setItems([]))
  }, [sort, filter])
  useEffect(loadItems, [loadItems])

  // While a sync runs, poll its status; reload everything when it finishes.
  useEffect(() => {
    if (!sync?.running) return
    const t = setInterval(async () => {
      const st = await api.syncStatus().catch(() => null)
      if (!st) return
      setSync(st)
      if (!st.running) {
        load()
        loadItems()
        const bad = st.accounts.filter((a) => !a.ok)
        toast(bad.length ? 'error' : 'success', '同步完成', bad.length ? `${bad.length} 个账号失败` : `${st.accounts.length} 个账号`)
      }
    }, 4000)
    return () => clearInterval(t)
  }, [sync?.running, load, loadItems, toast])

  async function syncNow() {
    await api.syncNow().catch((e) => toast('error', '无法开始同步', (e as Error).message))
    setSync((s) => ({ ...(s ?? { started_at: null, finished_at: null, next_at: null, accounts: [] }), running: true }))
  }

  const columns = useMemo(
    () =>
      (sum?.daily ?? []).map((d) => {
        const dt = new Date(d.date + 'T00:00:00')
        return { key: d.date, label: `${dt.getMonth() + 1}/${dt.getDate()}`, tip: dt.toLocaleDateString('zh-CN', { month: 'long', day: 'numeric', weekday: 'short' }), value: d[metric] ?? 0 }
      }),
    [sum, metric],
  )
  const works = sum?.platforms.reduce((n, p) => n + p.works, 0) ?? 0
  const loading = sum === null

  return (
    <div className="page">
      <PageHeader
        title="数据"
        desc={<SyncLine sync={sync} works={works} />}
        actions={
          <>
            <div className="seg" role="radiogroup" aria-label="时间范围">
              {[7, 30, 90].map((d) => (
                <button key={d} className={days === d ? 'on' : ''} onClick={() => setDays(d)}>
                  {d} 天
                </button>
              ))}
            </div>
            <button className="btn" onClick={syncNow} disabled={sync?.running}>
              {sync?.running ? <Loader2 size={15} className="spin" /> : <RefreshCw size={15} />}
              {sync?.running ? '同步中…' : '立即同步'}
            </button>
          </>
        }
      />

      {!loading && works === 0 && !sync?.running ? (
        <section className="card">
          <Empty icon={<BarChart3 size={22} />}>
            还没有数据。服务启动约 1.5 分钟后会自动同步一次，之后每隔一段时间同步（在「自动互动」里设置间隔）。
            <br />
            也可以点右上角「立即同步」。需要先在「账号管理」登录账号。
          </Empty>
        </section>
      ) : (
        <div className="bento">
          {KPIS.map(({ m, icon }) => (
            <section key={m} className="card kpi span-3">
              <div className="kpi-top">
                <span className="kpi-icon">{icon}</span>
                <span className="muted">{METRIC_LABEL[m]}</span>
              </div>
              <div className="kpi-value">{loading ? <Skeleton h={32} w={64} /> : fmtNum(sum.totals[m])}</div>
              <div className="muted small">{loading ? '' : `近 7 天 +${fmtNum(sum.last7[m])}`}</div>
            </section>
          ))}

          <section className="card span-12">
            <header className="card-head">
              <div>
                <h3>每日新增{METRIC_LABEL[metric]}</h3>
                <div className="muted small">
                  由每次同步的数据差值算出，统计窗口内的作品；共 +{fmtNum(columns.reduce((n, c) => n + c.value, 0))}
                </div>
              </div>
              <div className="hstack">
                <div className="seg small" role="radiogroup" aria-label="指标">
                  {METRICS.map((m) => (
                    <button key={m} className={metric === m ? 'on' : ''} onClick={() => setMetric(m)}>
                      {METRIC_LABEL[m]}
                    </button>
                  ))}
                </div>
                <div className="seg small" role="tablist">
                  <button className={!table ? 'on' : ''} onClick={() => setTable(false)}>
                    图表
                  </button>
                  <button className={table ? 'on' : ''} onClick={() => setTable(true)}>
                    表格
                  </button>
                </div>
              </div>
            </header>
            {loading ? (
              <Skeleton h={200} r={10} />
            ) : table ? (
              <div className="table-scroll">
                <table className="data-table">
                  <thead>
                    <tr>
                      <th>日期</th>
                      {METRICS.map((m) => (
                        <th key={m} className="num">
                          {METRIC_LABEL[m]}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {[...sum.daily].reverse().map((d) => (
                      <tr key={d.date}>
                        <td>{d.date}</td>
                        {METRICS.map((m) => (
                          <td key={m} className="num">
                            {(d[m] ?? 0).toLocaleString('zh-CN')}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            ) : (
              <ColumnChart data={columns} unit="" label={`每日新增${METRIC_LABEL[metric]}`} empty="这段时间还没有增长数据（至少需要两次同步）" />
            )}
          </section>

          <section className="card span-12">
            <header className="card-head">
              <h3>平台概况</h3>
            </header>
            {loading ? (
              <Skeleton h={120} r={10} />
            ) : (
              <div className="table-scroll">
                <table className="data-table">
                  <thead>
                    <tr>
                      <th>平台</th>
                      <th className="num">账号</th>
                      <th className="num">作品</th>
                      {METRICS.map((m) => (
                        <th key={m} className="num">
                          {METRIC_LABEL[m]}
                        </th>
                      ))}
                      <th>更新于</th>
                    </tr>
                  </thead>
                  <tbody>
                    {sum.platforms.map((p) => (
                      <tr key={p.platform}>
                        <td>
                          <span className="hstack sm">
                            <PlatformIcon p={platform(p.platform)} size={18} />
                            {platform(p.platform)?.name ?? p.platform}
                          </span>
                        </td>
                        <td className="num">{p.accounts}</td>
                        <td className="num">{p.works}</td>
                        {METRICS.map((m) => (
                          <td key={m} className="num">
                            {fmtNum(p[m])}
                          </td>
                        ))}
                        <td className="muted">{fmtTime(p.synced_at)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>

          <section className="card span-12">
            <header className="card-head">
              <div>
                <h3>作品排行</h3>
                <div className="muted small">括号里是近 24 小时的变化。「本系统发布」的作品可以跳到发布记录。</div>
              </div>
              <div className="hstack">
                <select value={filter} onChange={(e) => setFilter(e.target.value)} aria-label="平台">
                  <option value="">全部平台</option>
                  {(sum?.platforms ?? []).map((p) => (
                    <option key={p.platform} value={p.platform}>
                      {platform(p.platform)?.name ?? p.platform}
                    </option>
                  ))}
                </select>
                <select value={sort} onChange={(e) => setSort(e.target.value)} aria-label="排序">
                  <option value="published">按发布时间</option>
                  {METRICS.map((m) => (
                    <option key={m} value={m}>
                      按{METRIC_LABEL[m]}
                    </option>
                  ))}
                </select>
              </div>
            </header>
            {items === null ? (
              <Skeleton h={160} r={10} />
            ) : items.length === 0 ? (
              <Empty>没有作品</Empty>
            ) : (
              <div className="table-scroll">
                <table className="data-table works-table">
                  <thead>
                    <tr>
                      <th>作品</th>
                      <th>发布时间</th>
                      {METRICS.map((m) => (
                        <th key={m} className="num">
                          {METRIC_LABEL[m]}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {items.map((it) => (
                      <tr key={`${it.platform}/${it.account}/${it.item_id}`} className="clickable" onClick={() => setDetail(it)}>
                        <td>
                          <div className="work-cell">
                            <PlatformIcon p={platform(it.platform)} size={18} />
                            {it.cover ? <img src={it.cover} referrerPolicy="no-referrer" alt="" /> : <span className="thumb-ph" />}
                            <div className="grow">
                              <div className="clamp-1">{it.title || '（无标题）'}</div>
                              <div className="muted small">
                                {it.account}
                                {it.post_id && <span className="tag">本系统发布</span>}
                              </div>
                            </div>
                          </div>
                        </td>
                        <td className="muted nowrap">{fmtTime(it.published_at)}</td>
                        {METRICS.map((m) => (
                          <td key={m} className="num nowrap">
                            {fmtNum(it.stats[m])}
                            <Delta v={it.delta24h[m]} />
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </section>

          {sync && sync.accounts.length > 0 && (
            <section className="card span-12">
              <header className="card-head">
                <h3>上次同步结果</h3>
              </header>
              <ul className="sync-list">
                {sync.accounts.map((a) => (
                  <li key={`${a.platform}/${a.account}`}>
                    <PlatformIcon p={platform(a.platform)} size={18} />
                    <span className="grow">
                      {platform(a.platform)?.name ?? a.platform} · {a.account}
                    </span>
                    {a.ok ? (
                      <span className="muted small">
                        {a.items} 个作品 · 新消息 {a.new_inbox}
                      </span>
                    ) : (
                      <span className="danger-text small" title={a.error ?? ''}>
                        失败：{(a.error ?? '').slice(0, 60)}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </section>
          )}
        </div>
      )}

      {detail && <ItemDetail item={detail} onClose={() => setDetail(null)} />}
    </div>
  )
}

function SyncLine({ sync, works }: { sync: SyncStatus | null; works: number }) {
  if (!sync) return <>从各平台拉回作品的播放、点赞、评论、收藏</>
  const parts = [`已跟踪 ${works} 个作品`]
  if (sync.running) parts.push('正在同步…')
  else if (sync.finished_at) parts.push(`上次同步 ${fmtTime(sync.finished_at)}`)
  if (!sync.running && sync.next_at) parts.push(`下次 ${fmtTime(sync.next_at)}`)
  return <>{parts.join(' · ')}</>
}

function Delta({ v }: { v: number | null }) {
  if (!v) return null
  return <span className={'delta' + (v < 0 ? ' down' : '')}>{v > 0 ? `(+${fmtNum(v)})` : `(${v})`}</span>
}

function ItemDetail({ item, onClose }: { item: TrackedItem; onClose: () => void }) {
  const [rows, setRows] = useState<(Metrics & { at: string })[] | null>(null)
  useEffect(() => {
    api
      .itemHistory(item.platform, item.account, item.item_id)
      .then(setRows)
      .catch(() => setRows([]))
  }, [item])
  return (
    <Modal title={item.title || '作品数据'} onClose={onClose} wide>
      <div className="hstack">
        {item.url && (
          <a className="btn small" href={item.url} target="_blank" rel="noreferrer">
            <ExternalLink size={14} /> 在平台打开
          </a>
        )}
        {item.post_id && (
          <Link className="btn small" to="/posts">
            查看发布记录
          </Link>
        )}
        <span className="muted small">发布于 {fmtTime(item.published_at)}</span>
      </div>
      <h4 className="mt">历次同步</h4>
      {rows === null ? (
        <Skeleton h={120} r={10} />
      ) : rows.length === 0 ? (
        <p className="muted">这个作品不在统计窗口内，没有快照。</p>
      ) : (
        <div className="table-scroll tall">
          <table className="data-table">
            <thead>
              <tr>
                <th>时间</th>
                {METRICS.map((m) => (
                  <th key={m} className="num">
                    {METRIC_LABEL[m]}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {[...rows].reverse().map((r) => (
                <tr key={r.at}>
                  <td className="nowrap">{fmtTime(r.at)}</td>
                  {METRICS.map((m) => (
                    <td key={m} className="num">
                      {r[m] === null ? '-' : r[m]!.toLocaleString('zh-CN')}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Modal>
  )
}
