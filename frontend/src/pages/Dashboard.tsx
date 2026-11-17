import { AlertTriangle, ArrowRight, CalendarClock, CheckCircle2, Clock3, ExternalLink, Heart, Images, Plus, RefreshCw, Send, TrendingUp } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { Link } from 'react-router-dom'
import { api, fmtNum, fmtTime, mediaUrl, PROVIDER_LABEL, type Post, type ProvidersOverview, type Target, type Works } from '../api'
import { Empty, PageHeader, PlatformIcon, Skeleton, StatusBadge } from '../components/ui'
import { useStore } from '../store'

const DAY = 86_400_000
const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate())
const WEEK = ['日', '一', '二', '三', '四', '五', '六']

type Row = Target & { post: Post }

function greeting() {
  const h = new Date().getHours()
  return h < 6 ? '夜深了' : h < 12 ? '早上好' : h < 14 ? '中午好' : h < 18 ? '下午好' : '晚上好'
}

export default function Dashboard() {
  const { platforms, accounts, platform } = useStore()
  const [posts, setPosts] = useState<Post[] | null>(null)
  const [prov, setProv] = useState<ProvidersOverview | null>(null)
  const [works, setWorks] = useState<Works | null>(null)
  const [worksBusy, setWorksBusy] = useState(false)

  useEffect(() => {
    const now = new Date()
    api
      .posts(new Date(startOfDay(now).getTime() - 30 * DAY).toISOString(), new Date(now.getTime() + 60 * DAY).toISOString())
      .then(setPosts)
      .catch(() => setPosts([]))
    api.providers().then(setProv).catch(() => undefined)
  }, [])

  const loadWorks = useCallback((refresh = false) => {
    setWorksBusy(true)
    api
      .works(refresh)
      .then(setWorks)
      .catch(() => setWorks((w) => w))
      .finally(() => setWorksBusy(false))
  }, [])
  useEffect(() => loadWorks(false), [loadWorks])

  const rows: Row[] = useMemo(() => (posts ?? []).flatMap((p) => p.targets.map((t) => ({ ...t, post: p }))), [posts])

  const stats = useMemo(() => {
    const today = startOfDay(new Date()).getTime()
    const weekAgo = today - 6 * DAY
    const done = (t: Row) => new Date(t.updated_at).getTime()
    const recent = rows.filter((t) => done(t) >= weekAgo && (t.status === 'success' || t.status === 'failed'))
    const ok = recent.filter((t) => t.status === 'success').length
    return {
      today: rows.filter((t) => t.status === 'success' && done(t) >= today).length,
      pending: rows.filter((t) => t.status === 'pending' || t.status === 'running').length,
      failed: rows.filter((t) => t.status === 'failed').length,
      rate: recent.length ? Math.round((ok / recent.length) * 100) : null,
      recentTotal: recent.length,
    }
  }, [rows])

  const days = useMemo(() => {
    const today = startOfDay(new Date())
    return Array.from({ length: 7 }, (_, i) => {
      const d = new Date(today.getTime() - (6 - i) * DAY)
      const next = d.getTime() + DAY
      const n = rows.filter((t) => t.status === 'success' && new Date(t.updated_at).getTime() >= d.getTime() && new Date(t.updated_at).getTime() < next).length
      return { date: d, label: `${d.getMonth() + 1}/${d.getDate()}`, week: `周${WEEK[d.getDay()]}`, n }
    })
  }, [rows])

  const upcoming = useMemo(() => rows.filter((t) => t.status === 'pending').sort((a, b) => a.run_at.localeCompare(b.run_at)).slice(0, 5), [rows])
  const activity = useMemo(
    () => rows.filter((t) => t.status !== 'pending').sort((a, b) => b.updated_at.localeCompare(a.updated_at)).slice(0, 6),
    [rows],
  )

  const loading = posts === null
  const today = new Date().toLocaleDateString('zh-CN', { month: 'long', day: 'numeric', weekday: 'long' })

  return (
    <div className="page">
      <PageHeader
        title={`${greeting()} 👋`}
        desc={`${today} · ${accounts.length} 个账号已连接`}
        actions={
          <Link to="/compose" className="btn primary">
            <Plus size={16} /> 新发布
          </Link>
        }
      />

      <div className="bento">
        <Kpi icon={<Send size={16} />} label="今日已发布" value={stats.today} loading={loading} to="/posts" />
        <Kpi icon={<Clock3 size={16} />} label="待发布" value={stats.pending} loading={loading} to="/calendar" />
        <Kpi
          icon={<AlertTriangle size={16} />}
          label="需要处理"
          value={stats.failed}
          loading={loading}
          to="/posts"
          tone={stats.failed > 0 ? 'danger' : undefined}
          note={stats.failed > 0 ? '失败任务需确认后重试' : '没有失败任务'}
        />
        <Kpi
          icon={<TrendingUp size={16} />}
          label="近 7 天成功率"
          value={stats.rate === null ? '—' : `${stats.rate}%`}
          loading={loading}
          note={stats.recentTotal ? `共 ${stats.recentTotal} 次发布` : '暂无数据'}
        />

        <WorksCard works={works} busy={worksBusy} onRefresh={() => loadWorks(true)} platformIcon={platform} />

        <section className="card span-7">
          <WeekChart days={days} loading={loading} />
        </section>

        <section className="card span-5">
          <CardHead title="即将发布" icon={<CalendarClock size={16} />} to="/calendar" />
          {loading ? (
            <Lines n={4} />
          ) : upcoming.length === 0 ? (
            <Empty icon={<CalendarClock size={22} />}>没有排队中的任务</Empty>
          ) : (
            <ul className="feed-list">
              {upcoming.map((t) => (
                <li key={t.id}>
                  <PlatformIcon p={platform(t.platform)} size={28} />
                  <div className="grow min0">
                    <div className="ellipsis strong">{t.post.title || t.post.text || '(无文字)'}</div>
                    <div className="muted small">
                      {t.account} · {fmtTime(t.run_at)}
                    </div>
                  </div>
                  <StatusBadge status={t.status} />
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="card span-7">
          <CardHead title="最近动态" icon={<CheckCircle2 size={16} />} to="/posts" />
          {loading ? (
            <Lines n={5} />
          ) : activity.length === 0 ? (
            <Empty>
              还没有发布记录，<Link to="/compose">去写第一条</Link>
            </Empty>
          ) : (
            <ul className="feed-list">
              {activity.map((t) => (
                <li key={t.id}>
                  <PlatformIcon p={platform(t.platform)} size={28} />
                  <div className="grow min0">
                    <div className="ellipsis strong">{t.post.title || t.post.text || '(无文字)'}</div>
                    <div className="muted small ellipsis">
                      {t.account} · {fmtTime(t.updated_at)}
                      {t.provider && ` · via ${PROVIDER_LABEL[t.provider] ?? t.provider}`}
                      {t.status === 'failed' && t.last_error && ` · ${t.last_error}`}
                    </div>
                  </div>
                  <StatusBadge status={t.status} />
                </li>
              ))}
            </ul>
          )}
        </section>

        <div className="span-5 stack">
          <section className="card">
            <CardHead title="账号" to="/accounts" />
            <div className="plat-grid">
              {platforms.map((p) => {
                const n = accounts.filter((a) => a.platform === p.id).length
                return (
                  <Link key={p.id} to="/accounts" className={'plat-cell' + (n ? '' : ' off')}>
                    <PlatformIcon p={p} size={30} />
                    <span className="small">{p.name}</span>
                    <b>{n}</b>
                  </Link>
                )
              })}
            </div>
          </section>
          <section className="card">
            <CardHead title="发布通道" to="/providers" />
            {!prov ? (
              <Lines n={4} />
            ) : (
              <ul className="health">
                <Health name="catbus" ok={prov.catbus.ok} text={prov.catbus.ok ? `v${prov.catbus.version}` : '不可用'} />
                <Health name="X 官方 API" ok={prov.settings.x_api.accounts.length > 0} text={`${prov.settings.x_api.accounts.length} 个授权`} />
                <Health name="xiaohongshu-mcp" ok={prov.settings.xhs_mcp.instances.length > 0} text={`${prov.settings.xhs_mcp.instances.length} 个实例`} />
                <Health name="social-auto-upload" ok={prov.settings.sau.accounts.length > 0} text={`${prov.settings.sau.accounts.length} 个账号`} />
              </ul>
            )}
          </section>
        </div>
      </div>
    </div>
  )
}

function Kpi({ icon, label, value, note, loading, to, tone }: { icon: ReactNode; label: string; value: ReactNode; note?: string; loading: boolean; to?: string; tone?: 'danger' }) {
  const body = (
    <>
      <div className="kpi-top">
        <span className={'kpi-icon' + (tone ? ` ${tone}` : '')}>{icon}</span>
        <span className="muted">{label}</span>
        {to && <ArrowRight size={14} className="kpi-arrow" />}
      </div>
      <div className="kpi-value">{loading ? <Skeleton h={32} w={64} /> : value}</div>
      {note && <div className="muted small">{note}</div>}
    </>
  )
  return to ? (
    <Link to={to} className="card kpi span-3 hoverable">
      {body}
    </Link>
  ) : (
    <section className="card kpi span-3">{body}</section>
  )
}

function CardHead({ title, icon, to }: { title: string; icon?: ReactNode; to?: string }) {
  return (
    <header className="card-head">
      <h3>
        {icon}
        {title}
      </h3>
      {to && (
        <Link to={to} className="link-muted small">
          查看全部 <ArrowRight size={12} />
        </Link>
      )}
    </header>
  )
}

function Lines({ n }: { n: number }) {
  return (
    <div className="stack">
      {Array.from({ length: n }, (_, i) => (
        <Skeleton key={i} h={36} r={8} />
      ))}
    </div>
  )
}

function Health({ name, ok, text }: { name: string; ok: boolean; text: string }) {
  return (
    <li>
      <span className={'dot-status ' + (ok ? 'ok' : 'off')} />
      <span className="grow">{name}</span>
      <span className="muted small">{ok ? text : '未配置'}</span>
    </li>
  )
}

/** Single-series column chart: one hue, no legend (title names it), hover tooltip, table view. */
function WeekChart({ days, loading }: { days: { label: string; week: string; n: number }[]; loading: boolean }) {
  const [hover, setHover] = useState<number | null>(null)
  const [table, setTable] = useState(false)
  const max = Math.max(...days.map((d) => d.n), 0)
  const top = Math.max(4, Math.ceil(max / 4) * 4)
  const peak = days.findIndex((d) => d.n === max && max > 0)
  const total = days.reduce((s, d) => s + d.n, 0)

  return (
    <>
      <header className="card-head">
        <div>
          <h3>近 7 天发布量</h3>
          <div className="muted small">成功发布的帖子数（按平台账号计），共 {total} 条</div>
        </div>
        <div className="seg small" role="tablist">
          <button className={!table ? 'on' : ''} onClick={() => setTable(false)}>
            图表
          </button>
          <button className={table ? 'on' : ''} onClick={() => setTable(true)}>
            表格
          </button>
        </div>
      </header>
      {loading ? (
        <Skeleton h={180} r={10} />
      ) : table ? (
        <table className="data-table">
          <thead>
            <tr>
              <th>日期</th>
              <th>星期</th>
              <th className="num">发布数</th>
            </tr>
          </thead>
          <tbody>
            {days.map((d) => (
              <tr key={d.label}>
                <td>{d.label}</td>
                <td>{d.week}</td>
                <td className="num">{d.n}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : (
        <div className="chart" role="img" aria-label={`近 7 天发布量：${days.map((d) => `${d.label} ${d.n} 条`).join('，')}`}>
          <div className="chart-grid">
            {[top, top / 2, 0].map((v) => (
              <div key={v} className="grid-line">
                <span>{v}</span>
              </div>
            ))}
          </div>
          <div className="chart-cols">
            {days.map((d, i) => (
              <div
                key={d.label}
                className={'chart-col' + (hover === i ? ' hover' : '')}
                onMouseEnter={() => setHover(i)}
                onMouseLeave={() => setHover(null)}
                tabIndex={0}
                onFocus={() => setHover(i)}
                onBlur={() => setHover(null)}
                aria-label={`${d.label} ${d.week}：${d.n} 条`}
              >
                <div className="bar-area">
                  {i === peak && hover === null && <span className="bar-value" style={{ bottom: `${(d.n / top) * 100}%` }}>{d.n}</span>}
                  <div className="bar" style={{ height: d.n ? `max(${(d.n / top) * 100}%, 3px)` : 0 }} />
                  {hover === i && (
                    <div className="chart-tip" style={{ bottom: `calc(${(d.n / top) * 100}% + 10px)` }}>
                      <b>{d.n} 条</b>
                      <span>
                        {d.label} {d.week}
                      </span>
                    </div>
                  )}
                </div>
                <div className="col-label">{d.week}</div>
              </div>
            ))}
          </div>
          {total === 0 && <div className="chart-empty">近 7 天还没有成功发布的帖子</div>}
        </div>
      )}
    </>
  )
}

function WorksCard({
  works,
  busy,
  onRefresh,
  platformIcon,
}: {
  works: Works | null
  busy: boolean
  onRefresh: () => void
  platformIcon: (id: string) => import('../api').Platform | undefined
}) {
  if (!works && busy) {
    return (
      <section className="card span-12">
        <Skeleton h={120} r={10} />
      </section>
    )
  }
  if (!works) return null
  return <WorksView works={works} busy={busy} onRefresh={onRefresh} platformIcon={platformIcon} />
}

function WorksView({
  works,
  busy,
  onRefresh,
  platformIcon,
}: {
  works: Works
  busy: boolean
  onRefresh: () => void
  platformIcon: (id: string) => import('../api').Platform | undefined
}) {
  const [active, setActive] = useState<string | null>(null)
  const ok = works.accounts.filter((a) => a.ok)
  const skipped = works.accounts.filter((a) => !a.ok)

  // Clicking a platform tab narrows the numbers and the grid to that platform.
  const sel = active ? ok.find((a) => a.platform === active) : null
  const count = sel ? sel.count : works.totals.count
  const likes = sel ? sel.likes : works.totals.likes
  const capped = sel ? sel.capped : works.totals.capped
  const shown = sel
    ? sel.works.map((w) => ({ ...w, _platform: sel.platform, _account: sel.account }))
    : works.recent

  return (
    <section className="card span-12 works-card">
      <header className="card-head">
        <div>
          <h3>
            <Images size={16} /> 账号作品 · 历史
          </h3>
          <div className="muted small">各平台上你已发布的内容（只读，含在这个应用之前发的）· 更新于 {fmtTime(works.fetched_at)}</div>
        </div>
        <button className="btn small ghost" onClick={onRefresh} disabled={busy}>
          <RefreshCw size={14} className={busy ? 'spin' : ''} /> 刷新
        </button>
      </header>

      <div className="works-body">
        <div className="works-tabs">
          <button className={'work-tab' + (active === null ? ' on' : '')} onClick={() => setActive(null)}>
            全部
            <b>{fmtNum(works.totals.count)}</b>
          </button>
          {ok.map((a) => (
            <button
              key={a.platform + a.account}
              className={'work-tab' + (active === a.platform ? ' on' : '')}
              onClick={() => setActive(a.platform)}
            >
              <PlatformIcon p={platformIcon(a.platform)} size={18} />
              {platformIcon(a.platform)?.name ?? a.platform}
              <b>
                {a.count}
                {a.capped && '+'}
              </b>
            </button>
          ))}
          {skipped.map((a) => (
            <span key={a.platform + a.account} className="work-tab disabled" title={a.error ?? ''}>
              <PlatformIcon p={platformIcon(a.platform)} size={18} />
              {platformIcon(a.platform)?.name ?? a.platform}
              <span className="muted small">跳过</span>
            </span>
          ))}
        </div>

        <div className="works-stats">
          <div className="big-stat">
            <div className="big-num">
              {fmtNum(count)}
              {capped && '+'}
            </div>
            <div className="muted small">{sel ? `${platformIcon(sel.platform)?.name ?? sel.platform}作品数` : '作品总数'}</div>
          </div>
          <div className="big-stat">
            <div className="big-num">
              <Heart size={18} className="inline-heart" /> {fmtNum(likes)}
            </div>
            <div className="muted small">累计获赞</div>
          </div>
        </div>

        {shown.length === 0 ? (
          <Empty icon={<Images size={22} />}>{sel ? '这个平台还没有作品' : '还没有拉到作品'}</Empty>
        ) : (
          <div className="works-grid">
            {shown.map((w) => {
              const cover = w.cover ?? w.media?.find((m) => m.type === 'image')?.url ?? null
              return (
                <a key={(w._platform ?? '') + w.id} className="work-tile" href={w.url ?? undefined} target="_blank" rel="noreferrer">
                  <div className="work-thumb">
                    {cover ? <img src={mediaUrl(cover)} referrerPolicy="no-referrer" loading="lazy" alt="" /> : <Images size={18} />}
                    <span className="work-plat">
                      <PlatformIcon p={platformIcon(w._platform ?? '')} size={16} />
                    </span>
                  </div>
                  <div className="work-title">{w.title || w.text || '(无标题)'}</div>
                  <div className="work-meta muted small">
                    <span>
                      <Heart size={11} /> {fmtNum(w.stats?.likes)}
                    </span>
                    <ExternalLink size={11} />
                  </div>
                </a>
              )
            })}
          </div>
        )}
      </div>
    </section>
  )
}
