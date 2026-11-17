import { ExternalLink, Heart, MessageCircle, Quote, RefreshCw, Search } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { Link, useNavigate, useSearchParams } from 'react-router-dom'
import { api, fmtNum, fmtTime, type Comment, type Envelope, type Item, type Platform } from '../api'
import { Avatar, Empty, Modal, PlatformBadge, PlatformIcon, Skeleton } from '../components/ui'
import { useStore } from '../store'

type Column = { env: Envelope<Item[] | null> | null; items: Item[]; loading: boolean; via?: string }

function errorText(env: Envelope<unknown>): string {
  const e = env.error
  if (!e) return '未知错误'
  if (e.code === 'AUTH_REQUIRED') return '需要登录这个平台'
  if (e.code === 'RISK_CONTROL') return `触发平台风控：${e.message}，请稍后再试或更换账号`
  return `${e.code}：${e.message}`
}

export default function Discover() {
  const { platforms, accounts } = useStore()
  const [params] = useSearchParams()
  const [tab, setTab] = useState<'search' | 'feed'>('search')
  const [q, setQ] = useState(params.get('q') ?? '')
  const [sort, setSort] = useState('')
  const [picked, setPicked] = useState<string[] | null>(null)
  const [cols, setCols] = useState<Record<string, Column>>({})
  const [open, setOpen] = useState<{ p: Platform; item: Item } | null>(null)

  const hasAccount = (id: string) => accounts.some((a) => a.platform === id)
  const usable = platforms.filter((p) => (tab === 'search' ? p.can_search : p.can_feed))
  // Default selection: usable platforms that have a login.
  const active = (picked ?? usable.filter((p) => hasAccount(p.id)).map((p) => p.id)).filter((id) => usable.some((p) => p.id === id))

  const toggle = (id: string) => setPicked(active.includes(id) ? active.filter((x) => x !== id) : [...active, id])

  async function runSearch(kw = q) {
    if (!kw.trim() || !active.length) return
    const q = kw
    setCols(Object.fromEntries(active.map((id) => [id, { env: null, items: [], loading: true }])))
    try {
      const res = await api.search(q.trim(), active, sort || undefined)
      setCols(Object.fromEntries(res.map((r) => [r.platform, { env: r.envelope, items: r.envelope.data ?? [], loading: false, via: r.via }])))
    } catch (e) {
      setCols(Object.fromEntries(active.map((id) => [id, { env: fakeErr((e as Error).message), items: [], loading: false }])))
    }
  }

  async function loadFeed(id: string, more = false) {
    const prev = cols[id]
    const cursor = more ? prev?.env?.page?.cursor : null
    setCols((c) => ({ ...c, [id]: { env: prev?.env ?? null, items: more ? (prev?.items ?? []) : [], loading: true } }))
    try {
      const env = await api.feed(id, cursor)
      setCols((c) => ({ ...c, [id]: { env, items: [...(more ? (c[id]?.items ?? []) : []), ...(env.data ?? [])], loading: false, via: env.via } }))
    } catch (e) {
      setCols((c) => ({ ...c, [id]: { env: fakeErr((e as Error).message), items: c[id]?.items ?? [], loading: false } }))
    }
  }

  // Arriving from the command palette with ?q=… runs the search once.
  const autoRan = useRef(false)
  const initialQ = params.get('q')
  useEffect(() => {
    if (initialQ && !autoRan.current && active.length) {
      autoRan.current = true
      runSearch(initialQ)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [initialQ, active.length])

  // Feed tab: load each selected platform's feed when it appears.
  const activeKey = active.join(',')
  useEffect(() => {
    if (tab !== 'feed') return
    setCols({})
    activeKey.split(',').filter(Boolean).forEach((id) => loadFeed(id))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, activeKey])

  const switchTab = (t: 'search' | 'feed') => {
    setTab(t)
    setCols({})
    setPicked(null)
  }

  return (
    <div className="page wide-page">
      <header className="page-head row">
        <div>
          <h1>聚合浏览</h1>
          <p className="muted">同一个关键词同时搜多个平台，或并排看各平台推荐流。</p>
        </div>
        <div className="tabs">
          <button className={tab === 'search' ? 'on' : ''} onClick={() => switchTab('search')}>
            搜索
          </button>
          <button className={tab === 'feed' ? 'on' : ''} onClick={() => switchTab('feed')}>
            信息流
          </button>
        </div>
      </header>

      <div className="toolbar card">
        {tab === 'search' && (
          <form
            className="search-form"
            onSubmit={(e) => {
              e.preventDefault()
              runSearch()
            }}
          >
            <Search size={18} className="search-icon" />
            <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="输入关键词，例如：露营装备" autoFocus />
            <select value={sort} onChange={(e) => setSort(e.target.value)}>
              <option value="">综合排序</option>
              <option value="latest">最新</option>
              <option value="popular">最热</option>
            </select>
            <button className="primary" disabled={!q.trim() || !active.length}>
              搜索
            </button>
          </form>
        )}
        <div className="chips">
          {usable.map((p) => (
            <label key={p.id} className={'chip' + (active.includes(p.id) ? ' on' : '')} title={hasAccount(p.id) ? '' : '未登录，结果会提示需要登录'}>
              <input type="checkbox" checked={active.includes(p.id)} onChange={() => toggle(p.id)} />
              <PlatformIcon p={p} size={16} />
              {p.name}
              {!hasAccount(p.id) && <small className="muted"> 未登录</small>}
            </label>
          ))}
        </div>
        {platforms.some((p) => p.id === 'xianyu') && <p className="muted small">闲鱼暂不支持搜索和信息流（catbus 规划中），可以在「一键发布」里发商品。</p>}
      </div>

      {Object.keys(cols).length === 0 ? (
        <Empty>{tab === 'search' ? '输入关键词开始搜索' : active.length ? '加载中…' : '选择至少一个平台'}</Empty>
      ) : (
        <div className="columns">
          {active
            .filter((id) => cols[id])
            .map((id) => {
              const p = platforms.find((x) => x.id === id)!
              const c = cols[id]
              return (
                <section key={id} className="column">
                  <header>
                    <PlatformBadge p={p} />
                    {c.via && c.via !== 'catbus' && <span className="tag" title="catbus 失败，已自动改用备用通道">via {c.via}</span>}
                    <span className="muted small">{c.items.length} 条</span>
                    {tab === 'feed' && (
                      <button className="icon-btn" title="刷新" onClick={() => loadFeed(id)} disabled={c.loading}>
                        <RefreshCw size={14} className={c.loading ? 'spin' : ''} />
                      </button>
                    )}
                  </header>
                  <div className="column-body">
                    {c.env && !c.env.ok && (
                      <div className="banner error small">
                        {errorText(c.env)}
                        {c.env.error?.code === 'AUTH_REQUIRED' && (
                          <>
                            {' '}
                            <Link to="/accounts">去登录 →</Link>
                          </>
                        )}
                      </div>
                    )}
                    {c.items.map((it) => (
                      <ItemCard key={it.id} item={it} onOpen={() => setOpen({ p, item: it })} />
                    ))}
                    {c.loading &&
                      Array.from({ length: c.items.length ? 1 : 4 }, (_, i) => (
                        <div key={i} className="item-card skeleton-card">
                          <Skeleton w={78} h={78} r={8} />
                          <div className="grow stack" style={{ gap: 8 }}>
                            <Skeleton h={14} />
                            <Skeleton h={14} w="70%" />
                            <Skeleton h={12} w="40%" />
                          </div>
                        </div>
                      ))}
                    {!c.loading && c.env?.ok && c.items.length === 0 && <Empty>没有结果</Empty>}
                    {tab === 'feed' && !c.loading && c.env?.page?.has_more && (
                      <button className="ghost block" onClick={() => loadFeed(id, true)}>
                        加载更多
                      </button>
                    )}
                  </div>
                </section>
              )
            })}
        </div>
      )}

      {open && <ItemDetail p={open.p} item={open.item} onClose={() => setOpen(null)} />}
    </div>
  )
}

function fakeErr(message: string): Envelope<null> {
  return { ok: false, platform: null, account: null, data: null, page: null, error: { code: 'ERROR', message, hint: null, detail: null } }
}

function ItemCard({ item, onOpen }: { item: Item; onOpen: () => void }) {
  const cover = item.cover ?? item.media.find((m) => m.type === 'image')?.url ?? null
  return (
    <button className="item-card" onClick={onOpen}>
      {cover && <img src={cover} referrerPolicy="no-referrer" loading="lazy" alt="" />}
      <div className="item-main">
        <div className="item-title">{item.title || item.text || '(无标题)'}</div>
        <div className="item-meta">
          <Avatar src={item.author?.avatar} name={item.author?.name} size={18} />
          <span className="grow ellipsis">{item.author?.name}</span>
          <span>
            <Heart size={12} /> {fmtNum(item.stats?.likes)}
          </span>
          {item.stats?.comments != null && (
            <span>
              <MessageCircle size={12} /> {fmtNum(item.stats.comments)}
            </span>
          )}
        </div>
        {item.price && <div className="price">¥{item.price.amount}</div>}
      </div>
    </button>
  )
}

function ItemDetail({ p, item, onClose }: { p: Platform; item: Item; onClose: () => void }) {
  const nav = useNavigate()
  const [full, setFull] = useState<Item>(item)
  const [comments, setComments] = useState<Comment[] | null>(null)
  const [cErr, setCErr] = useState<string | null>(null)
  const ref = item.url ?? item.id

  useEffect(() => {
    api
      .item(p.id, ref)
      .then((env) => env.ok && env.data && setFull(env.data))
      .catch(() => undefined)
    if (p.can_comments) {
      api
        .comments(p.id, ref)
        .then((env) => (env.ok ? setComments(env.data ?? []) : setCErr(errorText(env))))
        .catch((e) => setCErr((e as Error).message))
    }
  }, [p.id, p.can_comments, ref])

  const images = full.media.filter((m) => m.type === 'image' && m.url)
  const video = full.media.find((m) => m.type === 'video' && m.url)

  const draft = () =>
    nav('/compose', {
      state: { title: full.title ?? '', text: `${full.text ?? ''}\n\n来源：${full.url ?? ''}`.trim() },
    })

  return (
    <Modal title={`${p.name} · 详情`} onClose={onClose} wide>
      <div className="detail">
        <div className="detail-author">
          <Avatar src={full.author?.avatar} name={full.author?.name} size={36} />
          <div className="grow">
            <b>{full.author?.name}</b>
            <div className="muted small">{fmtTime(full.created_at)}</div>
          </div>
          {full.url && (
            <a href={full.url} target="_blank" rel="noreferrer" className="button ghost small">
              原文 <ExternalLink size={12} />
            </a>
          )}
          <button className="small primary" onClick={draft}>
            <Quote size={13} /> 引用为草稿
          </button>
        </div>
        {full.title && <h2>{full.title}</h2>}
        {full.text && <p className="detail-text">{full.text}</p>}
        {full.price && <p className="price">¥{full.price.amount}</p>}
        {video?.url && <video src={video.url} controls className="detail-video" />}
        {images.length > 0 && (
          <div className="detail-images">
            {images.map((m, i) => (
              <a key={m.id ?? i} href={m.url!} target="_blank" rel="noreferrer">
                <img src={m.url!} referrerPolicy="no-referrer" loading="lazy" alt="" />
              </a>
            ))}
          </div>
        )}
        <div className="stats">
          <span>
            浏览 <b>{fmtNum(full.stats?.views)}</b>
          </span>
          <span>
            点赞 <b>{fmtNum(full.stats?.likes)}</b>
          </span>
          <span>
            评论 <b>{fmtNum(full.stats?.comments)}</b>
          </span>
          <span>
            收藏 <b>{fmtNum(full.stats?.collects)}</b>
          </span>
          <span>
            分享 <b>{fmtNum(full.stats?.shares)}</b>
          </span>
        </div>

        {p.can_comments && (
          <>
            <h3>评论</h3>
            {cErr && <p className="error-text small">{cErr}</p>}
            {comments === null && !cErr && <div className="spinner">加载评论…</div>}
            {comments?.length === 0 && <p className="muted">暂无评论</p>}
            <ul className="comments">
              {comments?.map((c) => (
                <li key={c.id}>
                  <Avatar src={c.author?.avatar} name={c.author?.name} size={24} />
                  <div className="grow">
                    <div className="small">
                      <b>{c.author?.name}</b> <span className="muted">{fmtTime(c.created_at)}</span>
                    </div>
                    <div>{c.text}</div>
                  </div>
                  <span className="muted small">♥ {fmtNum(c.stats?.likes)}</span>
                </li>
              ))}
            </ul>
          </>
        )}
      </div>
    </Modal>
  )
}
