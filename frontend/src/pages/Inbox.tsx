import { AtSign, CheckCheck, ExternalLink, Loader2, Mail, MailOpen, MessageCircle, MessagesSquare, RefreshCw, Reply, Send } from 'lucide-react'
import { useCallback, useEffect, useState, type ReactNode } from 'react'
import { api, fmtTime, type InboxItem, type InboxKind, type Message, type UnreadCounts } from '../api'
import { useToast } from '../components/toast'
import { Avatar, Empty, Modal, PageHeader, PlatformIcon, SkeletonList } from '../components/ui'
import { publishUnread } from '../inboxEvents'
import { useStore } from '../store'

const TABS: { id: '' | InboxKind; label: string }[] = [
  { id: '', label: '全部' },
  { id: 'comment', label: '评论' },
  { id: 'mention', label: '@我' },
  { id: 'dm', label: '私信' },
]
const KIND_ICON = { comment: MessageCircle, mention: AtSign, dm: Mail }
const KIND_LABEL = { comment: '评论', mention: '@我', dm: '私信' }

export default function Inbox() {
  const { platforms, platform } = useStore()
  const toast = useToast()
  const [kind, setKind] = useState<'' | InboxKind>('')
  const [pf, setPf] = useState('')
  const [unreadOnly, setUnreadOnly] = useState(false)
  const [items, setItems] = useState<InboxItem[] | null>(null)
  const [more, setMore] = useState(false)
  const [unread, setUnread] = useState<UnreadCounts>({ total: 0 })
  const [chat, setChat] = useState<InboxItem | null>(null)
  const [syncing, setSyncing] = useState(false)

  const load = useCallback(
    async (append?: string) => {
      try {
        const r = await api.inbox({ kind: kind || undefined, platform: pf || undefined, unread: unreadOnly, before: append })
        setItems((cur) => (append ? [...(cur ?? []), ...r.items] : r.items))
        setMore(r.has_more)
        setUnread(r.unread)
        publishUnread(r.unread)
      } catch (e) {
        toast('error', '加载收件箱失败', (e as Error).message)
        setItems((c) => c ?? [])
      }
    },
    [kind, pf, unreadOnly, toast],
  )
  useEffect(() => {
    setItems(null)
    load()
  }, [load])

  const patch = (it: InboxItem) => setItems((cur) => cur?.map((x) => (x.id === it.id ? it : x)) ?? null)

  async function setRead(ids: string[], read: boolean) {
    const r = await api.markRead({ ids, read }).catch(() => null)
    if (!r) return
    setItems((cur) => cur?.map((x) => (ids.includes(x.id) ? { ...x, is_read: read } : x)) ?? null)
    setUnread(r.unread)
    publishUnread(r.unread)
  }

  async function readAll() {
    const r = await api.markRead({ all: true }).catch(() => null)
    if (!r) return
    setItems((cur) => cur?.map((x) => ({ ...x, is_read: true })) ?? null)
    setUnread(r.unread)
    publishUnread(r.unread)
  }

  async function syncNow() {
    setSyncing(true)
    await api.syncNow().catch(() => null)
    // Poll until the collector finishes, then reload.
    for (let i = 0; i < 90; i++) {
      await new Promise((r) => setTimeout(r, 4000))
      const st = await api.syncStatus().catch(() => null)
      if (st && !st.running) break
    }
    setSyncing(false)
    load()
  }

  const known = new Set((items ?? []).map((i) => i.platform))
  return (
    <div className="page">
      <PageHeader
        title="收件箱"
        desc="各平台的评论、@ 和私信，每次同步时拉取。能在这里回复的平台会显示回复按钮。"
        actions={
          <>
            <button className="btn" onClick={readAll} disabled={!unread.total}>
              <CheckCheck size={15} /> 全部已读
            </button>
            <button className="btn" onClick={syncNow} disabled={syncing}>
              {syncing ? <Loader2 size={15} className="spin" /> : <RefreshCw size={15} />}
              {syncing ? '同步中…' : '立即同步'}
            </button>
          </>
        }
      />

      <div className="inbox-toolbar">
        <div className="tabs">
          {TABS.map((t) => {
            const n = t.id ? (unread[t.id] ?? 0) : unread.total
            return (
              <button key={t.id} className={kind === t.id ? 'on' : ''} onClick={() => setKind(t.id)}>
                {t.label}
                {n > 0 && <span className="count">{n}</span>}
              </button>
            )
          })}
        </div>
        <div className="hstack">
          <select value={pf} onChange={(e) => setPf(e.target.value)} aria-label="平台">
            <option value="">全部平台</option>
            {platforms
              .filter((p) => known.has(p.id) || p.id === pf || ['xhs', 'douyin', 'bilibili', 'x', 'weibo', 'kuaishou'].includes(p.id))
              .map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
          </select>
          <label className="field check">
            <input type="checkbox" checked={unreadOnly} onChange={(e) => setUnreadOnly(e.target.checked)} />
            <span>只看未读</span>
          </label>
        </div>
      </div>

      {items === null ? (
        <SkeletonList rows={5} h={84} />
      ) : items.length === 0 ? (
        <section className="card">
          <Empty icon={<MessagesSquare size={22} />}>{unreadOnly ? '没有未读消息' : '还没有消息。同步后，新的评论、@ 和私信会出现在这里。'}</Empty>
        </section>
      ) : (
        <div className="inbox-list">
          {items.map((it) => (
            <InboxRow
              key={it.id}
              it={it}
              platformName={platform(it.platform)?.name ?? it.platform}
              icon={<PlatformIcon p={platform(it.platform)} size={16} />}
              onRead={(read) => setRead([it.id], read)}
              onReplied={patch}
              onOpenChat={() => {
                setChat(it)
                if (!it.is_read) setRead([it.id], true)
              }}
            />
          ))}
          {more && (
            <button className="btn block" onClick={() => load(items[items.length - 1]?.sort_key)}>
              加载更多
            </button>
          )}
        </div>
      )}

      {chat && <ChatModal it={chat} onClose={() => setChat(null)} onReplied={patch} />}
    </div>
  )
}

function InboxRow({
  it,
  platformName,
  icon,
  onRead,
  onReplied,
  onOpenChat,
}: {
  it: InboxItem
  platformName: string
  icon: ReactNode
  onRead: (read: boolean) => void
  onReplied: (it: InboxItem) => void
  onOpenChat: () => void
}) {
  const toast = useToast()
  const [open, setOpen] = useState(false)
  const [text, setText] = useState('')
  const [busy, setBusy] = useState(false)
  const K = KIND_ICON[it.kind]

  async function send() {
    setBusy(true)
    try {
      onReplied(await api.reply(it.id, text))
      setOpen(false)
      setText('')
      toast('success', '已回复')
    } catch (e) {
      toast('error', '回复失败', (e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  const external = it.kind === 'dm' ? it.author.url : it.item_url
  return (
    <article className={'card inbox-row' + (it.is_read ? '' : ' unread')}>
      <Avatar src={it.author.avatar} name={it.author.name} size={36} />
      <div className="grow">
        <div className="inbox-meta">
          {it.author.url ? (
            <a href={it.author.url} target="_blank" rel="noreferrer" className="b">
              {it.author.name ?? '未知用户'}
            </a>
          ) : (
            <b>{it.author.name ?? '未知用户'}</b>
          )}
          <span className="tag">
            <K size={12} /> {KIND_LABEL[it.kind]}
          </span>
          <span className="muted small hstack sm">
            {icon}
            {platformName} · {it.account}
          </span>
          <span className="muted small">{fmtTime(it.created_at ?? it.fetched_at)}</span>
        </div>
        <p className="inbox-text">{it.text || <span className="muted">（无文字内容）</span>}</p>
        {it.item_title && it.kind !== 'dm' && (
          <div className="muted small clamp-1">
            作品：
            {it.item_url ? (
              <a href={it.item_url} target="_blank" rel="noreferrer">
                {it.item_title}
              </a>
            ) : (
              it.item_title
            )}
          </div>
        )}
        {it.replied_at && (
          <div className="replied">
            <Reply size={12} /> 已回复：{it.reply_text}
          </div>
        )}
        {open && (
          <div className="reply-box">
            <textarea rows={2} maxLength={500} autoFocus value={text} onChange={(e) => setText(e.target.value)} placeholder={`回复 ${it.author.name ?? ''}`} />
            <div className="hstack">
              <button className="btn primary small" disabled={busy || !text.trim()} onClick={send}>
                {busy ? <Loader2 size={14} className="spin" /> : <Send size={14} />} 发送
              </button>
              <button className="btn small" onClick={() => setOpen(false)}>
                取消
              </button>
            </div>
          </div>
        )}
      </div>
      <div className="inbox-actions">
        {it.kind === 'dm' ? (
          <button className="btn small" onClick={onOpenChat}>
            <MessagesSquare size={14} /> 查看对话
          </button>
        ) : it.can_reply ? (
          !open && (
            <button className="btn small" onClick={() => setOpen(true)}>
              <Reply size={14} /> 回复
            </button>
          )
        ) : (
          external && (
            <a className="btn small" href={external} target="_blank" rel="noreferrer" title="这个平台暂不支持在聚合台里回复">
              <ExternalLink size={14} /> 去平台回复
            </a>
          )
        )}
        <button className="icon-btn" title={it.is_read ? '标为未读' : '标为已读'} aria-label={it.is_read ? '标为未读' : '标为已读'} onClick={() => onRead(!it.is_read)}>
          {it.is_read ? <Mail size={15} /> : <MailOpen size={15} />}
        </button>
      </div>
    </article>
  )
}

function ChatModal({ it, onClose, onReplied }: { it: InboxItem; onClose: () => void; onReplied: (it: InboxItem) => void }) {
  const toast = useToast()
  const [msgs, setMsgs] = useState<Message[] | null>(null)
  const [err, setErr] = useState<string | null>(null)
  const [text, setText] = useState('')
  const [busy, setBusy] = useState(false)

  const load = useCallback(() => {
    api
      .messages(it.id)
      .then((env) => {
        if (!env.ok) setErr(env.error?.message ?? '加载失败')
        // Oldest first for reading top-down.
        setMsgs([...(env.data ?? [])].sort((a, b) => (a.created_at ?? '').localeCompare(b.created_at ?? '')))
      })
      .catch((e) => {
        setErr((e as Error).message)
        setMsgs([])
      })
  }, [it.id])
  useEffect(load, [load])

  async function send() {
    setBusy(true)
    try {
      onReplied(await api.reply(it.id, text))
      setText('')
      load()
    } catch (e) {
      toast('error', '发送失败', (e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal title={`与 ${it.author.name ?? '对方'} 的私信`} onClose={onClose} wide>
      {msgs === null ? (
        <SkeletonList rows={4} h={40} />
      ) : (
        <div className="chat">
          {err && <div className="banner error">{err}</div>}
          {msgs.length === 0 && !err && <p className="muted">没有消息记录</p>}
          {msgs.map((m) => {
            const mine = m.from?.id && it.author.id ? m.from.id !== it.author.id : false
            return (
              <div key={m.id} className={'bubble' + (mine ? ' mine' : '')}>
                <div>{messageText(m)}</div>
                <span className="muted small">{fmtTime(m.created_at)}</span>
              </div>
            )
          })}
        </div>
      )}
      {it.can_reply ? (
        <div className="reply-box">
          <textarea rows={2} maxLength={500} value={text} onChange={(e) => setText(e.target.value)} placeholder="输入消息" />
          <button className="btn primary small" disabled={busy || !text.trim()} onClick={send}>
            {busy ? <Loader2 size={14} className="spin" /> : <Send size={14} />} 发送
          </button>
        </div>
      ) : (
        <p className="muted small mt">这个平台暂不支持在聚合台里发私信，只能查看。</p>
      )}
    </Modal>
  )
}

/** Cards (shared notes, goods) arrive as JSON text; show them as "[笔记] 标题". */
function messageText(m: Message): string {
  const t = m.text ?? ''
  if (t.startsWith('{')) {
    try {
      const j = JSON.parse(t) as { title?: string; type?: string }
      if (j.title) return `[${j.type === 'note' ? '笔记' : '卡片'}] ${j.title}`
    } catch {
      /* plain text that happens to start with { */
    }
  }
  return t || `[${m.type}]`
}
