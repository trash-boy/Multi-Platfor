import { useEffect, useRef, useState } from 'react'
import { Link } from 'react-router-dom'
import { api, fmtTime, PROVIDER_LABEL, type LoginStatus, type Platform } from '../api'
import { Avatar, Modal, PageHeader, PlatformIcon } from '../components/ui'
import { useToast } from '../components/toast'
import { CheckCircle2, Cookie, LogOut, QrCode, RefreshCw } from 'lucide-react'
import { useStore } from '../store'

export default function Accounts() {
  const { platforms, accounts, refreshAccounts } = useStore()
  const [qr, setQr] = useState<{ p: Platform; account: string } | null>(null)
  const [cookie, setCookie] = useState<{ p: Platform; account: string } | null>(null)
  const [names, setNames] = useState<Record<string, string>>({})
  const [msg, setMsg] = useState<Record<string, string>>({})

  const nameFor = (p: Platform) => (names[p.id] ?? '').trim() || 'default'

  async function check(p: Platform, account: string) {
    const key = `${p.id}/${account}`
    setMsg((m) => ({ ...m, [key]: '检查中…' }))
    try {
      const env = await api.checkAccount(p.id, account)
      const text = env.ok ? (env.data.logged_in ? `有效 · ${env.data.user?.name ?? ''}` : '已失效，请重新登录') : `${env.error?.code}: ${env.error?.message}`
      setMsg((m) => ({ ...m, [key]: text }))
    } catch (e) {
      setMsg((m) => ({ ...m, [key]: (e as Error).message }))
    }
  }

  async function logout(p: Platform, account: string) {
    if (!confirm(`确定退出 ${p.name} 账号「${account}」？本地登录态会被删除。`)) return
    await api.logout(p.id, account).catch(() => undefined)
    await refreshAccounts()
  }

  return (
    <div className="page">
      <PageHeader title="账号管理" desc="登录态由 catbus 保存在本机 data/catbus 目录。同一平台可以登录多个账号，用不同的账号名区分。" />

      <div className="grid cards">
        {platforms.map((p) => {
          const mine = accounts.filter((a) => a.platform === p.id)
          return (
            <section key={p.id} className="card platform-card">
              <header>
                <PlatformIcon p={p} size={32} />
                <div>
                  <h2>{p.name}</h2>
                  <div className="muted small">{mine.length ? `${mine.length} 个账号` : '未登录'}</div>
                </div>
                <span className="caps">
                  {p.can_search && <em>搜索</em>}
                  {p.can_feed && <em>信息流</em>}
                  {p.publish && <em>发布</em>}
                </span>
              </header>

              {mine.length === 0 && <p className="muted small">还没有登录账号</p>}
              <ul className="account-list">
                {mine.map((a) => {
                  const key = `${p.id}/${a.account}`
                  return (
                    <li key={key}>
                      <Avatar src={a.user?.avatar} name={a.user?.name ?? a.account} />
                      <div className="grow">
                        <div>
                          <b>{a.user?.name ?? '(未知昵称)'}</b> <span className="tag">{a.account}</span>
                          {a.current && <span className="tag">当前</span>}
                          {a.providers.filter((x) => x !== 'catbus').map((x) => (
                            <span key={x} className="tag prov">
                              {PROVIDER_LABEL[x] ?? x}
                            </span>
                          ))}
                        </div>
                        <div className="muted small">
                          {a.method ?? ''} {a.updated_at ? '· ' + fmtTime(a.updated_at) : ''} {msg[key] ? '· ' + msg[key] : ''}
                        </div>
                      </div>
                      {a.providers.includes('catbus') && (
                        <>
                          <button className="icon-btn" title="检查登录状态" onClick={() => check(p, a.account)}>
                            <RefreshCw size={15} />
                          </button>
                          <button className="icon-btn danger" title="退出" onClick={() => logout(p, a.account)}>
                            <LogOut size={15} />
                          </button>
                        </>
                      )}
                    </li>
                  )
                })}
              </ul>

              {p.login_methods.length === 0 ? (
                <p className="muted small">
                  该平台通过其他通道发布，请到 <Link to="/providers">通道设置</Link> 绑定账号。
                </p>
              ) : (
              <div className="add-account">
                <input
                  placeholder="账号名（默认 default）"
                  value={names[p.id] ?? ''}
                  onChange={(e) => setNames((n) => ({ ...n, [p.id]: e.target.value }))}
                />
                {p.login_methods.includes('qrcode') && (
                  <button className="primary" onClick={() => setQr({ p, account: nameFor(p) })}>
                    <QrCode size={15} /> 扫码
                  </button>
                )}
                {p.login_methods.includes('cookie') && (
                  <button className="secondary" onClick={() => setCookie({ p, account: nameFor(p) })}>
                    <Cookie size={15} /> Cookie
                  </button>
                )}
              </div>
              )}
            </section>
          )
        })}
      </div>

      {qr && (
        <QrLogin
          p={qr.p}
          account={qr.account}
          onClose={() => setQr(null)}
          onDone={() => {
            refreshAccounts()
          }}
        />
      )}
      {cookie && (
        <CookieLogin
          p={cookie.p}
          account={cookie.account}
          onClose={() => setCookie(null)}
          onDone={() => {
            setCookie(null)
            refreshAccounts()
          }}
        />
      )}
    </div>
  )
}

function QrLogin({ p, account, onClose, onDone }: { p: Platform; account: string; onClose: () => void; onDone: () => void }) {
  const [st, setSt] = useState<LoginStatus | null>(null)
  const [err, setErr] = useState<string | null>(null)
  const [round, setRound] = useState(0)
  const [code, setCode] = useState('')
  const [sending, setSending] = useState(false)
  const finished = useRef(false)

  const submitCode = async (value: string) => {
    setSending(true)
    setErr(null)
    try {
      setSt(await api.qrCode(p.id, value))
      setCode('')
    } catch (e) {
      setErr((e as Error).message)
    } finally {
      setSending(false)
    }
  }

  useEffect(() => {
    let stop = false
    let timer: number | undefined
    finished.current = false
    setSt(null)
    setErr(null)
    const poll = async () => {
      try {
        const s = await api.qrStatus(p.id)
        if (stop) return
        setSt(s)
        if (s.state === 'success') {
          finished.current = true
          onDone()
          return
        }
        if (s.state === 'failed') {
          finished.current = true
          return
        }
      } catch (e) {
        if (!stop) setErr((e as Error).message)
      }
      timer = window.setTimeout(poll, 1500)
    }
    api
      .qrStart(p.id, account)
      .then((s) => {
        if (stop) return
        setSt(s)
        timer = window.setTimeout(poll, 1000)
      })
      .catch((e) => setErr((e as Error).message))
    return () => {
      stop = true
      window.clearTimeout(timer)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [p.id, account, round])

  const close = () => {
    if (!finished.current) api.qrCancel(p.id).catch(() => undefined)
    onClose()
  }

  return (
    <Modal title={`扫码登录 ${p.name}（账号名：${account}）`} onClose={close}>
      <div className="qr-box">
        {st?.state === 'success' ? (
          <div className="qr-result ok inline-icon">
            <CheckCircle2 size={20} /> {st.message ?? '登录成功'}
          </div>
        ) : st?.state === 'failed' ? (
          <div className="qr-result bad">
            <p>{st.message ?? '登录失败'}</p>
            <button onClick={() => setRound((r) => r + 1)}>重新获取二维码</button>
          </div>
        ) : st?.state === 'need_code' ? (
          <form
            className="qr-code-form"
            onSubmit={(e) => {
              e.preventDefault()
              if (/^\d{6}$/.test(code)) void submitCode(code)
            }}
          >
            <p>{st.message}</p>
            <input
              autoFocus
              inputMode="numeric"
              autoComplete="one-time-code"
              maxLength={6}
              placeholder="6 位验证码"
              value={code}
              onChange={(e) => setCode(e.target.value.replace(/\D/g, ''))}
            />
            <div className="actions">
              <button type="button" className="ghost" disabled={sending} onClick={() => void submitCode('')}>
                跳过，在 App 里验证
              </button>
              <button type="submit" className="primary" disabled={sending || code.length !== 6}>
                {sending ? '提交中…' : '提交验证码'}
              </button>
            </div>
          </form>
        ) : st?.qrcode_ready ? (
          <>
            <img
              src={api.qrImageUrl(p.id, `${st.id}-${st.qr_seq}`)}
              alt="登录二维码"
              className={st.state === 'waiting' ? '' : 'scanned'}
            />
            <p>
              {st.state === 'verify'
                ? st.message
                : st.state === 'scanned'
                  ? '已扫码，请在手机上确认'
                  : `打开${p.name} App 扫一扫`}
            </p>
          </>
        ) : (
          <div className="spinner">正在获取二维码…</div>
        )}
        {err && <p className="error-text">{err}</p>}
      </div>
      <p className="muted small">
        二维码几分钟内有效。抖音扫码后如果要求短信验证，可以直接在这里输入验证码；其他人机验证网页端无法完成，请改用「导入 Cookie」。
      </p>
      {st?.state === 'success' && (
        <div className="actions">
          <button onClick={onClose}>完成</button>
        </div>
      )}
    </Modal>
  )
}

function CookieLogin({ p, account, onClose, onDone }: { p: Platform; account: string; onClose: () => void; onDone: () => void }) {
  const toast = useToast()
  const [cookie, setCookie] = useState('')
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(null)

  async function submit() {
    setBusy(true)
    setErr(null)
    try {
      const env = await api.cookieLogin(p.id, account, cookie)
      if (env.ok) {
        toast('success', `${p.name} 登录成功`, account)
        onDone()
      }
      else setErr(`${env.error?.code}: ${env.error?.message}${env.error?.hint ? `（${env.error.hint}）` : ''}`)
    } catch (e) {
      setErr((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal title={`导入 ${p.name} Cookie（账号名：${account}）`} onClose={onClose} wide>
      <ol className="steps">
        <li>
          在浏览器打开{' '}
          <a href={p.cookie_site} target="_blank" rel="noreferrer">
            {p.cookie_site}
          </a>{' '}
          并登录
        </li>
        <li>按 F12 打开开发者工具，切到 Network（网络）面板，刷新页面</li>
        <li>点开任意一个发往该域名的请求，在 Request Headers 里复制 Cookie 的完整值</li>
        <li>
          需要包含：<code>{p.cookie_keys}</code>（document.cookie 拿不到 HttpOnly 的登录态，必须从请求头复制）
        </li>
      </ol>
      <textarea rows={6} placeholder="a=1; b=2; ..." value={cookie} onChange={(e) => setCookie(e.target.value)} />
      {err && <p className="error-text">{err}</p>}
      <div className="actions">
        <button className="ghost" onClick={onClose}>
          取消
        </button>
        <button disabled={busy || !cookie.trim()} onClick={submit}>
          {busy ? '校验中…' : '导入并校验'}
        </button>
      </div>
    </Modal>
  )
}
