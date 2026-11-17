import { ChevronLeft, ChevronRight, CircleCheck, CircleX, ImagePlus, Loader2, Send, TriangleAlert, X } from 'lucide-react'
import { useMemo, useRef, useState } from 'react'
import { useToast } from '../components/toast'
import { Link, useLocation, useNavigate } from 'react-router-dom'
import { mediaUrl, api, ApiError, charLen, fromLocalInput, validate, xWeight, type Media, type NewTarget, type Platform } from '../api'
import { PageHeader, PlatformBadge } from '../components/ui'
import { useStore } from '../store'

type TargetCfg = { visibility: string; options: Record<string, unknown>; override: boolean; title: string; text: string }

const VIS_LABEL: Record<string, string> = { public: '公开', private: '仅自己', friends: '好友', fans: '粉丝' }

const splitList = (s: string) =>
  s
    .split(/[,，\s#]+/)
    .map((x) => x.trim())
    .filter(Boolean)

export type ComposeDraft = { title?: string; text?: string }

export default function Compose() {
  const { platforms, accounts, loading } = useStore()
  const nav = useNavigate()
  const toast = useToast()
  const draft = (useLocation().state ?? {}) as ComposeDraft

  const [title, setTitle] = useState(draft.title ?? '')
  const [text, setText] = useState(draft.text ?? '')
  const [tags, setTags] = useState('')
  const [topics, setTopics] = useState('')
  const [images, setImages] = useState<Media[]>([])
  const [video, setVideo] = useState<Media | null>(null)
  const [cover, setCover] = useState<Media | null>(null)
  const [selected, setSelected] = useState<string[]>([]) // "platform/account"
  const [cfg, setCfg] = useState<Record<string, TargetCfg>>({}) // by platform
  const [mode, setMode] = useState<'now' | 'later'>('now')
  const [when, setWhen] = useState('')
  const [uploading, setUploading] = useState(false)
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<{ msg: string; details: string[] } | null>(null)
  const fileRef = useRef<HTMLInputElement>(null)
  const coverRef = useRef<HTMLInputElement>(null)

  const publishable = platforms.filter((p) => p.publish)
  const selectedPlatforms = useMemo(() => {
    const ids = new Set(selected.map((s) => s.split('/')[0]))
    return publishable.filter((p) => ids.has(p.id))
  }, [selected, publishable])

  const cfgFor = (p: Platform): TargetCfg =>
    cfg[p.id] ?? { visibility: p.publish!.visibility[0] ?? 'public', options: {}, override: false, title: '', text: '' }
  const setCfgFor = (p: Platform, patch: Partial<TargetCfg>) => setCfg((c) => ({ ...c, [p.id]: { ...cfgFor(p), ...patch } }))

  const checks = useMemo(() => {
    const out: Record<string, ReturnType<typeof validate>> = {}
    for (const p of selectedPlatforms) {
      const c = cfgFor(p)
      out[p.id] = validate(p, {
        title: c.override ? c.title : title,
        text: c.override ? c.text : text,
        imageCount: images.length,
        hasVideo: !!video,
        hasCover: !!cover,
        visibility: c.visibility,
        options: c.options,
      })
    }
    return out
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedPlatforms, cfg, title, text, images, video, cover])

  const blocking = Object.values(checks).some((c) => c.errors.length > 0)

  async function upload(files: FileList | null, as: 'media' | 'cover') {
    if (!files?.length) return
    setUploading(true)
    setErr(null)
    try {
      const saved = await api.upload([...files])
      if (as === 'cover') {
        setCover(saved.find((m) => m.kind === 'image') ?? null)
      } else {
        const vids = saved.filter((m) => m.kind === 'video')
        const imgs = saved.filter((m) => m.kind === 'image')
        if (vids.length) {
          setVideo(vids[0])
          setImages([])
        } else {
          setVideo(null)
          setImages((cur) => [...cur, ...imgs])
        }
      }
    } catch (e) {
      setErr({ msg: (e as Error).message, details: [] })
    } finally {
      setUploading(false)
      if (fileRef.current) fileRef.current.value = ''
      if (coverRef.current) coverRef.current.value = ''
    }
  }

  const move = (i: number, d: -1 | 1) =>
    setImages((cur) => {
      const next = [...cur]
      const j = i + d
      if (j < 0 || j >= next.length) return cur
      ;[next[i], next[j]] = [next[j], next[i]]
      return next
    })

  const toggle = (key: string) => setSelected((s) => (s.includes(key) ? s.filter((x) => x !== key) : [...s, key]))

  async function submit() {
    setErr(null)
    const scheduled_at = mode === 'later' ? fromLocalInput(when) : null
    if (mode === 'later' && !scheduled_at) return setErr({ msg: '请选择定时时间', details: [] })
    if (scheduled_at && new Date(scheduled_at).getTime() < Date.now() - 60_000) return setErr({ msg: '定时时间已经过去了', details: [] })
    const targets: NewTarget[] = selected.map((key) => {
      const [platform, ...rest] = key.split('/')
      const p = platforms.find((x) => x.id === platform)!
      const c = cfgFor(p)
      return {
        platform,
        account: rest.join('/'),
        visibility: c.visibility,
        options: Object.fromEntries(Object.entries(c.options).filter(([, v]) => v !== '' && v !== undefined && v !== false)),
        title: c.override ? c.title : null,
        text: c.override ? c.text : null,
      }
    })
    setBusy(true)
    try {
      await api.createPost({
        title: title.trim() || null,
        text,
        images: images.map((m) => m.id),
        video: video?.id ?? null,
        cover: video ? (cover?.id ?? null) : null,
        tags: splitList(tags),
        topics: splitList(topics),
        scheduled_at,
        targets,
      })
      toast('success', scheduled_at ? '已加入定时队列' : '已开始发布', `${targets.length} 个账号`)
      nav(scheduled_at ? '/calendar' : '/posts')
    } catch (e) {
      setErr({ msg: (e as Error).message, details: e instanceof ApiError ? e.details : [] })
    } finally {
      setBusy(false)
    }
  }

  const loggedIn = (p: Platform) => accounts.filter((a) => a.platform === p.id)

  return (
    <div className="page">
      <PageHeader title="一键发布" desc="写一次内容，同时发到多个平台和账号。每个平台的规则会实时校验。" />

      <div className="compose">
        <section className="card editor">
          <label className="field">
            <span>
              标题 <em className="muted">{charLen(title)} 字</em>
            </span>
            <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="小红书 ≤20 字，B站必填；X / 微博会忽略标题" />
          </label>
          <label className="field">
            <span>
              正文 <em className="muted">{charLen(text)} 字 · X 权重 {xWeight(text)}</em>
            </span>
            <textarea rows={10} value={text} onChange={(e) => setText(e.target.value)} placeholder="写点什么…" />
          </label>
          <div className="row2">
            <label className="field">
              <span>标签（小红书 / 抖音 / B站）</span>
              <input value={tags} onChange={(e) => setTags(e.target.value)} placeholder="空格或逗号分隔" />
            </label>
            <label className="field">
              <span>话题（小红书 / 微博 / 抖音）</span>
              <input value={topics} onChange={(e) => setTopics(e.target.value)} placeholder="空格或逗号分隔" />
            </label>
          </div>

          <div className="field">
            <span>
              媒体 <em className="muted">图片（可多张）或一个视频</em>
            </span>
            <div
              className="dropzone"
              onDragOver={(e) => e.preventDefault()}
              onDrop={(e) => {
                e.preventDefault()
                upload(e.dataTransfer.files, 'media')
              }}
            >
              {images.map((m, i) => (
                <div key={m.id} className="thumb">
                  <img src={mediaUrl(m.url)} alt={m.filename} />
                  <div className="thumb-actions">
                    <button className="icon" onClick={() => move(i, -1)} aria-label="左移">
                      <ChevronLeft size={16} />
                    </button>
                    <button className="icon" onClick={() => setImages((c) => c.filter((x) => x.id !== m.id))} aria-label="删除">
                      <X size={16} />
                    </button>
                    <button className="icon" onClick={() => move(i, 1)} aria-label="右移">
                      <ChevronRight size={16} />
                    </button>
                  </div>
                  {i === 0 && <span className="thumb-tag">首图</span>}
                </div>
              ))}
              {video && (
                <div className="thumb video">
                  <video src={mediaUrl(video.url)} muted />
                  <div className="thumb-actions">
                    <button className="icon" onClick={() => setVideo(null)} aria-label="删除">
                      <X size={16} />
                    </button>
                  </div>
                  <span className="thumb-tag">视频</span>
                </div>
              )}
              <button className="add-media" onClick={() => fileRef.current?.click()} disabled={uploading}>
                {uploading ? <Loader2 size={20} className="spin" /> : <ImagePlus size={20} />}
                {uploading ? '上传中…' : '添加或拖入'}
              </button>
              <input ref={fileRef} type="file" hidden multiple accept="image/*,video/*" onChange={(e) => upload(e.target.files, 'media')} />
            </div>
          </div>

          {video && (
            <div className="field">
              <span>视频封面（小红书必填）</span>
              <div className="dropzone small">
                {cover && (
                  <div className="thumb">
                    <img src={mediaUrl(cover.url)} alt="封面" />
                    <div className="thumb-actions">
                      <button className="icon" onClick={() => setCover(null)} aria-label="删除">
                        <X size={16} />
                      </button>
                    </div>
                  </div>
                )}
                <button className="add-media" onClick={() => coverRef.current?.click()}>
                  {cover ? '更换封面' : '+ 上传封面'}
                </button>
                <input ref={coverRef} type="file" hidden accept="image/*" onChange={(e) => upload(e.target.files, 'cover')} />
              </div>
            </div>
          )}
        </section>

        <aside className="side">
          <section className="card">
            <h3>发布到</h3>
            {publishable.map((p) => {
              const accs = loggedIn(p)
              return (
                <div key={p.id} className="target-pick">
                  <PlatformBadge p={p} />
                  {loading ? (
                    <span className="muted small">加载中…</span>
                  ) : accs.length === 0 ? (
                    <Link to="/accounts" className="muted small">
                      未登录 →
                    </Link>
                  ) : (
                    <div className="chips">
                      {accs.map((a) => {
                        const key = `${p.id}/${a.account}`
                        return (
                          <label key={key} className={'chip' + (selected.includes(key) ? ' on' : '')}>
                            <input type="checkbox" checked={selected.includes(key)} onChange={() => toggle(key)} />
                            {a.user?.name ?? a.account}
                            {a.account !== 'default' && <small> ({a.account})</small>}
                          </label>
                        )
                      })}
                    </div>
                  )}
                </div>
              )
            })}
          </section>

          {selectedPlatforms.map((p) => {
            const c = cfgFor(p)
            const spec = p.publish!
            const chk = checks[p.id]
            return (
              <section key={p.id} className={'card target-cfg' + (chk?.errors.length ? ' has-error' : '')}>
                <header>
                  <PlatformBadge p={p} />
                  {chk && !chk.errors.length && (
                    <span className="ok-text inline-icon">
                      <CircleCheck size={14} /> 可以发布
                    </span>
                  )}
                </header>
                {chk?.errors.map((e) => (
                  <p key={e} className="error-text small inline-icon">
                    <CircleX size={13} /> {e}
                  </p>
                ))}
                {chk?.warnings.map((w) => (
                  <p key={w} className="warn-text small inline-icon">
                    <TriangleAlert size={13} /> {w}
                  </p>
                ))}
                {spec.notes.map((n) => (
                  <p key={n} className="muted small">
                    · {n}
                  </p>
                ))}

                {spec.visibility.length > 1 && (
                  <label className="field inline">
                    <span>可见范围</span>
                    <select value={c.visibility} onChange={(e) => setCfgFor(p, { visibility: e.target.value })}>
                      {spec.visibility.map((v) => (
                        <option key={v} value={v}>
                          {VIS_LABEL[v] ?? v}
                        </option>
                      ))}
                    </select>
                  </label>
                )}

                {spec.options.map((o) => (
                  <label key={o.key} className={'field' + (o.kind === 'bool' ? ' check' : ' inline')} title={o.help}>
                    {o.kind === 'bool' ? (
                      <>
                        <input
                          type="checkbox"
                          checked={!!c.options[o.key]}
                          onChange={(e) => setCfgFor(p, { options: { ...c.options, [o.key]: e.target.checked } })}
                        />
                        <span>{o.label}</span>
                      </>
                    ) : (
                      <>
                        <span>
                          {o.label}
                          {o.required && <b className="req">*</b>}
                        </span>
                        {o.kind === 'select' ? (
                          <select
                            value={String(c.options[o.key] ?? '')}
                            onChange={(e) => setCfgFor(p, { options: { ...c.options, [o.key]: e.target.value } })}
                          >
                            <option value="">默认</option>
                            {o.choices.map((ch) => (
                              <option key={ch} value={ch}>
                                {ch}
                              </option>
                            ))}
                          </select>
                        ) : (
                          <input
                            inputMode={o.kind === 'number' ? 'decimal' : undefined}
                            value={String(c.options[o.key] ?? '')}
                            placeholder={o.help}
                            onChange={(e) => setCfgFor(p, { options: { ...c.options, [o.key]: e.target.value } })}
                          />
                        )}
                      </>
                    )}
                  </label>
                ))}

                <label className="field check">
                  <input
                    type="checkbox"
                    checked={c.override}
                    onChange={(e) => setCfgFor(p, { override: e.target.checked, title: c.title || title, text: c.text || text })}
                  />
                  <span>为这个平台单独改写文案</span>
                </label>
                {c.override && (
                  <>
                    {spec.title && <input value={c.title} onChange={(e) => setCfgFor(p, { title: e.target.value })} placeholder="标题" />}
                    <textarea rows={4} value={c.text} onChange={(e) => setCfgFor(p, { text: e.target.value })} />
                  </>
                )}
              </section>
            )
          })}

          <section className="card">
            <h3>发布时间</h3>
            <label className="field check">
              <input type="radio" checked={mode === 'now'} onChange={() => setMode('now')} />
              <span>立即发布</span>
            </label>
            <label className="field check">
              <input type="radio" checked={mode === 'later'} onChange={() => setMode('later')} />
              <span>定时发布</span>
            </label>
            {mode === 'later' && <input type="datetime-local" value={when} onChange={(e) => setWhen(e.target.value)} />}
            <p className="muted small">同一账号的多条任务会依次执行，每条间隔约 20 秒。</p>

            {err && (
              <div className="error-box">
                <b>{err.msg}</b>
                {err.details.map((d) => (
                  <div key={d}>{d}</div>
                ))}
              </div>
            )}
            <button className="primary block" disabled={busy || uploading || !selected.length || blocking} onClick={submit}>
              {busy ? <Loader2 size={16} className="spin" /> : <Send size={15} />}
              {busy ? '提交中…' : !selected.length ? '先选择要发布的账号' : blocking ? '请先修正上面的问题' : mode === 'now' ? `立即发布到 ${selected.length} 个账号` : '加入定时队列'}
            </button>
          </section>
        </aside>
      </div>
    </div>
  )
}
