import { useCallback, useEffect, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { api, PROVIDER_LABEL, type ProvidersOverview } from '../api'
import { Modal, PageHeader, PlatformBadge } from '../components/ui'
import { useStore } from '../store'

const DESC: Record<string, { tech: string; repo: string; note: string }> = {
  catbus: { tech: '逆向网页签名，直接发 HTTP 请求', repo: 'https://github.com/cv-cat/catbus', note: '已内置。轻量、快；平台更新签名后可能暂时失效。' },
  'xhs-mcp': {
    tech: '无头浏览器模拟人工操作（go-rod）',
    repo: 'https://github.com/xpzouying/xiaohongshu-mcp',
    note: '小红书的备用通道，签名变化不受影响。一个服务实例对应一个账号，需与本服务运行在同一台机器上（发布时传本地文件路径）。',
  },
  'x-api': { tech: 'X 官方 API v2（OAuth 2.0）', repo: 'https://docs.x.com', note: '最稳定、无封号风险。按量计费：每条约 $0.015，带链接约 $0.20。' },
  sau: {
    tech: '浏览器自动化（patchright）',
    repo: 'https://github.com/dreammis/social-auto-upload',
    note: '覆盖抖音、快手、B站、小红书、微博视频和视频号。需要单独安装，并在终端里执行 sau <平台> login 登录。',
  },
}

export default function Providers() {
  const { platforms, refreshAccounts } = useStore()
  const [ov, setOv] = useState<ProvidersOverview | null>(null)
  const [err, setErr] = useState<string | null>(null)
  const [params, setParams] = useSearchParams()

  const load = useCallback(() => {
    api.providers().then(setOv).catch((e) => setErr((e as Error).message))
    refreshAccounts()
  }, [refreshAccounts])
  useEffect(load, [load])

  const run = async (f: () => Promise<unknown>) => {
    try {
      setErr(null)
      await f()
      load()
    } catch (e) {
      setErr((e as Error).message)
    }
  }

  if (!ov) return <div className="page">{err ? <div className="banner error">{err}</div> : '加载中…'}</div>

  const xOk = params.get('x_ok')
  const xErr = params.get('x_error')

  return (
    <div className="page">
      <PageHeader
        title="通道设置"
        desc={
          <>
          每个平台可以配置多个发布通道，按顺序尝试。只有在<b>确定前一个通道没有发出去</b>时（未登录、未安装、被风控拒绝、官方接口明确拒绝）才会自动换下一个；结果不确定时会停下，避免重复发帖，你可以在发布记录里手动换通道重试。
          </>
        }
      />
      {err && <div className="banner error">{err}</div>}
      {(xOk || xErr) && (
        <div className={'banner ' + (xErr ? 'error' : 'ok')}>
          {xErr ? `X 授权失败：${xErr}` : `X 账号 ${xOk} 授权成功`}{' '}
          <button className="ghost small" onClick={() => setParams({})}>
            关闭
          </button>
        </div>
      )}

      <section className="card">
        <h3>各平台的通道顺序</h3>
        <table className="order-table">
          <tbody>
            {platforms
              .filter((p) => p.publish)
              .map((p) => {
                const o = ov.order[p.id]
                if (!o) return null
                const unused = o.supported.filter((x) => !o.order.includes(x))
                const save = (order: string[]) => run(() => api.setOrder(p.id, order))
                return (
                  <tr key={p.id}>
                    <td>
                      <PlatformBadge p={p} />
                    </td>
                    <td>
                      <div className="order-chain">
                        {o.order.map((id, i) => (
                          <span key={id} className="order-item">
                            {i > 0 && <span className="arrow">→</span>}
                            <span className="pill">
                              {PROVIDER_LABEL[id] ?? id}
                              <button className="icon" title="提前" disabled={i === 0} onClick={() => save(swap(o.order, i, i - 1))}>
                                ‹
                              </button>
                              <button className="icon" title="移除" disabled={o.order.length === 1} onClick={() => save(o.order.filter((x) => x !== id))}>
                                ×
                              </button>
                            </span>
                          </span>
                        ))}
                        {unused.map((id) => (
                          <button key={id} className="ghost small" onClick={() => save([...o.order, id])}>
                            + {PROVIDER_LABEL[id] ?? id}
                          </button>
                        ))}
                      </div>
                    </td>
                  </tr>
                )
              })}
          </tbody>
        </table>
      </section>

      <div className="grid cards" style={{ marginTop: 16 }}>
        <ProviderCard id="catbus" status={ov.catbus.ok ? `运行正常 · v${ov.catbus.version}` : `不可用：${ov.catbus.error}`} ok={ov.catbus.ok}>
          <p className="muted small">登录、搜索、信息流和大部分平台的发布都由 catbus 完成。账号在「账号管理」里登录。</p>
        </ProviderCard>
        <XApiCard ov={ov} run={run} />
        <XhsMcpCard ov={ov} run={run} />
        <SauCard ov={ov} run={run} />
      </div>
    </div>
  )
}

function swap<T>(a: T[], i: number, j: number): T[] {
  const n = [...a]
  ;[n[i], n[j]] = [n[j], n[i]]
  return n
}

function ProviderCard({ id, status, ok, children }: { id: string; status?: string; ok?: boolean; children: React.ReactNode }) {
  const d = DESC[id]
  return (
    <section className="card provider-card">
      <header>
        <h2>{PROVIDER_LABEL[id]}</h2>
        {status && <span className={'status ' + (ok ? 'status-success' : 'status-canceled')}>{status}</span>}
      </header>
      <p className="small">
        {d.tech} ·{' '}
        <a href={d.repo} target="_blank" rel="noreferrer">
          项目主页 ↗
        </a>
      </p>
      <p className="muted small">{d.note}</p>
      {children}
    </section>
  )
}

type CardProps = { ov: ProvidersOverview; run: (f: () => Promise<unknown>) => Promise<void> }

function XApiCard({ ov, run }: CardProps) {
  const x = ov.settings.x_api
  const [form, setForm] = useState({ client_id: x.client_id, client_secret: x.client_secret, redirect_uri: x.redirect_uri })
  const [account, setAccount] = useState('default')
  const configured = !!x.client_id
  return (
    <ProviderCard id="x-api" status={configured ? `${x.accounts.length} 个账号已授权` : '未配置'} ok={configured && x.accounts.length > 0}>
      <details open={!configured}>
        <summary className="small">X 应用设置</summary>
        <ol className="steps small">
          <li>
            在{' '}
            <a href="https://developer.x.com" target="_blank" rel="noreferrer">
              developer.x.com
            </a>{' '}
            创建应用，开启 OAuth 2.0，类型选 Web App，权限选 Read and write
          </li>
          <li>
            Callback URL 填：<code>{form.redirect_uri || ov.x_redirect_uri}</code>
          </li>
          <li>把 Client ID / Client Secret 填到下面，并在开发者后台充值额度</li>
        </ol>
        <input placeholder="Client ID" value={form.client_id} onChange={(e) => setForm({ ...form, client_id: e.target.value })} />
        <input placeholder="Client Secret（公共客户端可留空）" type="password" value={form.client_secret} onChange={(e) => setForm({ ...form, client_secret: e.target.value })} />
        <input placeholder={`回调地址（默认 ${ov.x_redirect_uri}）`} value={form.redirect_uri} onChange={(e) => setForm({ ...form, redirect_uri: e.target.value })} />
        <button className="small" onClick={() => run(() => api.setXApp(form))}>
          保存
        </button>
      </details>
      <ul className="account-list">
        {x.accounts.map((a) => (
          <li key={a.account}>
            <div className="grow">
              <b>{a.name}</b> <span className="muted">@{a.username}</span> <span className="tag">{a.account}</span>
            </div>
            <button className="ghost small danger" onClick={() => confirm('移除这个授权？') && run(() => api.removeXAccount(a.account))}>
              移除
            </button>
          </li>
        ))}
      </ul>
      <div className="add-account">
        <input value={account} onChange={(e) => setAccount(e.target.value)} placeholder="账号名" />
        <a className={'button' + (configured ? '' : ' disabled')} href={configured ? api.xAuthorizeUrl(account || 'default') : undefined}>
          授权 X 账号
        </a>
      </div>
      <p className="muted small">账号名与「账号管理」里 catbus 登录的 X 账号同名时，两个通道会合并成一个账号，官方 API 失败会自动改用 catbus。</p>
    </ProviderCard>
  )
}

function XhsMcpCard({ ov, run }: CardProps) {
  const list = ov.settings.xhs_mcp.instances
  const [form, setForm] = useState({ account: 'default', url: 'http://127.0.0.1:18060', token: '' })
  const [status, setStatus] = useState<Record<string, string>>({})
  const [qr, setQr] = useState<{ account: string; img: string | null; msg?: string } | null>(null)

  const check = async (account: string) => {
    setStatus((s) => ({ ...s, [account]: '检查中…' }))
    try {
      const r = await api.xhsMcpStatus(account)
      setStatus((s) => ({ ...s, [account]: !r.reachable ? `✗ ${r.error}` : r.logged_in ? `✓ 已登录 ${r.username ?? ''}` : '! 服务正常但未登录' }))
    } catch (e) {
      setStatus((s) => ({ ...s, [account]: (e as Error).message }))
    }
  }

  const login = async (account: string) => {
    setQr({ account, img: null, msg: '正在获取二维码（首次可能需要几十秒）…' })
    try {
      const r = await api.xhsMcpQr(account)
      setQr({ account, img: r.img, msg: r.logged_in ? '已经是登录状态' : '用小红书 App 扫码' })
    } catch (e) {
      setQr({ account, img: null, msg: (e as Error).message })
    }
  }

  return (
    <ProviderCard id="xhs-mcp" status={list.length ? `${list.length} 个实例` : '未配置'} ok={list.length > 0}>
      <details>
        <summary className="small">如何启动</summary>
        <pre className="code">{`# 从 Releases 下载对应平台的二进制，然后：
./xiaohongshu-mcp-darwin-arm64 -port :18060
# 多账号：每个账号单独一个进程、单独端口和数据目录`}</pre>
      </details>
      <ul className="account-list">
        {list.map((i) => (
          <li key={i.account}>
            <div className="grow">
              <span className="tag">{i.account}</span> <code>{i.url}</code>
              <div className="muted small">{status[i.account]}</div>
            </div>
            <button className="ghost small" onClick={() => check(i.account)}>
              检查
            </button>
            <button className="ghost small" onClick={() => login(i.account)}>
              扫码
            </button>
            <button className="ghost small danger" onClick={() => run(() => api.removeXhsMcp(i.account))}>
              移除
            </button>
          </li>
        ))}
      </ul>
      <div className="add-account">
        <input style={{ flex: '0 0 90px' }} value={form.account} onChange={(e) => setForm({ ...form, account: e.target.value })} placeholder="账号名" />
        <input value={form.url} onChange={(e) => setForm({ ...form, url: e.target.value })} placeholder="http://127.0.0.1:18060" />
        <input style={{ flex: '0 0 90px' }} type="password" value={form.token} onChange={(e) => setForm({ ...form, token: e.target.value })} placeholder="令牌(可选)" />
        <button onClick={() => run(() => api.upsertXhsMcp(form))}>添加</button>
      </div>
      <p className="muted small">账号名填成和「账号管理」里小红书账号相同的名字，catbus 失败时就会自动改用这里。</p>
      {qr && (
        <Modal title={`xiaohongshu-mcp 登录（${qr.account}）`} onClose={() => setQr(null)}>
          <div className="qr-box">
            {qr.img && <img src={qr.img} alt="二维码" />}
            <p>{qr.msg}</p>
          </div>
          <div className="actions">
            <button
              onClick={() => {
                check(qr.account)
                setQr(null)
              }}
            >
              我已扫码
            </button>
          </div>
        </Modal>
      )}
    </ProviderCard>
  )
}

function SauCard({ ov, run }: CardProps) {
  const { platforms } = useStore()
  const sau = ov.settings.sau
  const [bin, setBin] = useState(sau.bin)
  const [form, setForm] = useState({ platform: 'douyin', account: 'default', sau_account: 'creator' })
  const [check, setCheck] = useState<Record<string, string>>({})
  const sauPlatforms = platforms.filter((p) => p.providers.includes('sau'))
  const name = (id: string) => platforms.find((p) => p.id === id)?.name ?? id
  const sauName: Record<string, string> = { xhs: 'xiaohongshu' }

  return (
    <ProviderCard id="sau" status={sau.accounts.length ? `${sau.accounts.length} 个账号` : '未配置'} ok={sau.accounts.length > 0}>
      <details>
        <summary className="small">如何安装和登录</summary>
        <pre className="code">{`git clone https://github.com/dreammis/social-auto-upload
cd social-auto-upload && uv sync   # 或按项目文档安装
uv run patchright install chromium
# 每个平台登录一次（会弹出浏览器扫码）：
sau douyin login --account creator
sau tencent login --account creator   # 视频号`}</pre>
      </details>
      <div className="add-account">
        <input value={bin} onChange={(e) => setBin(e.target.value)} placeholder="sau 可执行文件路径" />
        <button className="secondary" onClick={() => run(() => api.setSauBin(bin))}>
          保存路径
        </button>
      </div>
      <ul className="account-list">
        {sau.accounts.map((a) => {
          const key = `${a.platform}/${a.account}`
          return (
            <li key={key}>
              <div className="grow">
                {name(a.platform)} <span className="tag">{a.account}</span> → sau 账号 <code>{a.sau_account}</code>
                <div className="muted small">{check[key]}</div>
              </div>
              <button
                className="ghost small"
                onClick={async () => {
                  setCheck((c) => ({ ...c, [key]: '检查中…' }))
                  const r = await api.sauCheck(a.platform, a.account).catch((e) => ({ ok: false, message: (e as Error).message }))
                  setCheck((c) => ({ ...c, [key]: (r.ok ? '✓ ' : '✗ ') + r.message }))
                }}
              >
                检查
              </button>
              <button className="ghost small danger" onClick={() => run(() => api.removeSau(a.platform, a.account))}>
                移除
              </button>
            </li>
          )
        })}
      </ul>
      <div className="add-account">
        <select style={{ flex: '0 0 90px' }} value={form.platform} onChange={(e) => setForm({ ...form, platform: e.target.value })}>
          {sauPlatforms.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        <input value={form.account} onChange={(e) => setForm({ ...form, account: e.target.value })} placeholder="账号名" />
        <input value={form.sau_account} onChange={(e) => setForm({ ...form, sau_account: e.target.value })} placeholder="sau --account" />
        <button onClick={() => run(() => api.upsertSau(form))}>绑定</button>
      </div>
      <p className="muted small">
        先在终端执行 <code>sau {sauName[form.platform] ?? form.platform} login --account {form.sau_account || '<名字>'}</code> 完成登录，再绑定。
      </p>
    </ProviderCard>
  )
}
