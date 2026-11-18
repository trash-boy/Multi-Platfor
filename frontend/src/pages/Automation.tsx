import { AlertTriangle, Pencil, Plus, Save, Trash2, Zap } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import {
  api,
  COMMENT_PLATFORMS,
  fmtNum,
  fmtTime,
  METRIC_LABEL,
  METRICS,
  type AutoAction,
  type Automation as AutomationData,
  type AutomationSettings,
  type Rule,
} from '../api'
import { useToast } from '../components/toast'
import { Empty, Modal, PageHeader, PlatformIcon, Skeleton } from '../components/ui'
import { useStore } from '../store'

const SOURCE_LABEL: Record<AutoAction['source'], string> = { first_comment: '首评', milestone: '达标追评', keyword: '关键词回复' }
const ACTION_STATUS: Record<AutoAction['status'], string> = { pending: '排队中', running: '发送中', success: '已发送', failed: '失败', canceled: '已取消' }
const STATUS_CLASS: Record<AutoAction['status'], string> = { pending: 'pending', running: 'running', success: 'success', failed: 'failed', canceled: 'canceled' }
const SYNC_CHOICES = [0, 30, 60, 120, 240, 360, 720]

type Draft = Omit<Rule, 'id' | 'created_at'> & { id?: string; created_at?: string }

const blank = (kind: Rule['kind']): Draft => ({
  kind,
  enabled: true,
  name: kind === 'keyword' ? '关键词自动回复' : '点赞破 100 追评',
  platform: null,
  account: null,
  keywords: [],
  metric: kind === 'milestone' ? 'likes' : null,
  threshold: kind === 'milestone' ? 100 : null,
  text: '',
})

export default function Automation() {
  const { platform } = useStore()
  const toast = useToast()
  const [data, setData] = useState<AutomationData | null>(null)
  const [settings, setSettings] = useState<AutomationSettings | null>(null)
  const [actions, setActions] = useState<AutoAction[] | null>(null)
  const [draft, setDraft] = useState<Draft | null>(null)
  const [saving, setSaving] = useState(false)

  const load = useCallback(() => {
    api
      .automation()
      .then((d) => {
        setData(d)
        setSettings(d.settings)
      })
      .catch((e) => toast('error', '加载失败', (e as Error).message))
    api
      .autoActions()
      .then(setActions)
      .catch(() => setActions([]))
  }, [toast])
  useEffect(load, [load])

  async function saveSettings(next: AutomationSettings) {
    setSaving(true)
    try {
      setSettings(await api.setAutomation(next))
      toast('success', '设置已保存')
      load()
    } catch (e) {
      toast('error', '保存失败', (e as Error).message)
    } finally {
      setSaving(false)
    }
  }

  async function toggleRule(r: Rule) {
    try {
      await api.updateRule({ ...r, enabled: !r.enabled })
      load()
    } catch (e) {
      toast('error', '操作失败', (e as Error).message)
    }
  }

  async function removeRule(r: Rule) {
    if (!confirm(`删除规则「${r.name}」？排队中的动作也会取消。`)) return
    await api.deleteRule(r.id).catch((e) => toast('error', '删除失败', (e as Error).message))
    load()
  }

  async function cancel(a: AutoAction) {
    await api.cancelAction(a.id).catch((e) => toast('error', '取消失败', (e as Error).message))
    load()
  }

  const dirty = settings && data && JSON.stringify(settings) !== JSON.stringify(data.settings)

  return (
    <div className="page">
      <PageHeader title="自动互动" desc="数据同步频率、自动评论规则和执行记录。自动评论只在抖音、B站、X 上可用。" />

      <div className="banner warn">
        <AlertTriangle size={16} />
        <span>
          自动评论是用你的账号发出的，频繁或内容重复容易被平台判定为营销、触发风控。建议每日上限保持在 20 条以内，回复内容写得具体一些。账号一旦触发风控，自动评论会暂停 24 小时。
        </span>
      </div>

      <div className="bento">
        <Stat label="排队中" value={data?.stats.pending} />
        <Stat label="近 24 小时已发送" value={data?.stats.sent24h} />
        <Stat label="近 24 小时失败" value={data?.stats.failed24h} danger={!!data?.stats.failed24h} />
        <section className="card kpi span-3">
          <div className="kpi-top">
            <span className="kpi-icon">
              <Zap size={16} />
            </span>
            <span className="muted">自动评论总开关</span>
          </div>
          {settings ? (
            <label className="field check big">
              <input type="checkbox" checked={settings.auto_enabled} disabled={saving} onChange={(e) => saveSettings({ ...settings, auto_enabled: e.target.checked })} />
              <span>{settings.auto_enabled ? '已开启' : '已暂停'}</span>
            </label>
          ) : (
            <Skeleton h={28} w={80} />
          )}
        </section>

        <section className="card span-12">
          <header className="card-head">
            <h3>同步与限额</h3>
          </header>
          {!settings ? (
            <Skeleton h={80} r={10} />
          ) : (
            <div className="form-grid">
              <label className="field">
                <span>数据同步间隔</span>
                <select value={settings.sync_minutes} onChange={(e) => setSettings({ ...settings, sync_minutes: Number(e.target.value) })}>
                  {SYNC_CHOICES.map((m) => (
                    <option key={m} value={m}>
                      {m === 0 ? '关闭自动同步' : m < 60 ? `每 ${m} 分钟` : `每 ${m / 60} 小时`}
                    </option>
                  ))}
                </select>
                <small className="muted">拉取作品数据、评论和私信；越频繁越容易触发风控</small>
              </label>
              <label className="field">
                <span>统计窗口</span>
                <select value={settings.window_days} onChange={(e) => setSettings({ ...settings, window_days: Number(e.target.value) })}>
                  {[3, 7, 14, 30].map((d) => (
                    <option key={d} value={d}>
                      最近 {d} 天发布的作品
                    </option>
                  ))}
                </select>
                <small className="muted">只对窗口内的作品记录数据、拉评论</small>
              </label>
              <label className="field">
                <span>每个账号每日自动评论上限</span>
                <input type="number" min={0} max={200} value={settings.daily_limit} onChange={(e) => setSettings({ ...settings, daily_limit: Number(e.target.value) })} />
                <small className="muted">首评、追评、关键词回复合计；超出的会顺延</small>
              </label>
              <div className="field end">
                <button className="btn primary" disabled={!dirty || saving} onClick={() => saveSettings(settings)}>
                  <Save size={15} /> 保存
                </button>
              </div>
            </div>
          )}
        </section>

        <section className="card span-12">
          <header className="card-head">
            <div>
              <h3>规则</h3>
              <div className="muted small">
                发布后首评在 <Link to="/compose">一键发布</Link> 里按帖子设置。规则只对创建之后的新评论、新作品生效。
              </div>
            </div>
            <div className="hstack">
              <button className="btn small" onClick={() => setDraft(blank('keyword'))}>
                <Plus size={14} /> 关键词回复
              </button>
              <button className="btn small" onClick={() => setDraft(blank('milestone'))}>
                <Plus size={14} /> 达标追评
              </button>
            </div>
          </header>
          {!data ? (
            <Skeleton h={80} r={10} />
          ) : data.rules.length === 0 ? (
            <Empty>还没有规则</Empty>
          ) : (
            <ul className="rule-list">
              {data.rules.map((r) => (
                <li key={r.id} className={r.enabled ? '' : 'off'}>
                  <label className="switch" title={r.enabled ? '停用' : '启用'}>
                    <input type="checkbox" checked={r.enabled} onChange={() => toggleRule(r)} />
                    <i />
                  </label>
                  <div className="grow">
                    <div className="hstack sm">
                      <b>{r.name}</b>
                      <span className="tag">{r.kind === 'keyword' ? '关键词回复' : '达标追评'}</span>
                      <span className="muted small">
                        {r.platform ? (platform(r.platform)?.name ?? r.platform) : '抖音、B站、X'}
                        {r.account ? ` · ${r.account}` : ' · 所有账号'}
                      </span>
                    </div>
                    <div className="muted small">
                      {r.kind === 'keyword' ? `评论包含「${r.keywords.join('」「')}」时回复` : `${METRIC_LABEL[r.metric ?? 'likes']}达到 ${fmtNum(r.threshold)} 时在作品下评论`}
                    </div>
                    <div className="rule-text">「{r.text}」</div>
                  </div>
                  <button className="icon-btn" aria-label="编辑" onClick={() => setDraft(r)}>
                    <Pencil size={15} />
                  </button>
                  <button className="icon-btn" aria-label="删除" onClick={() => removeRule(r)}>
                    <Trash2 size={15} />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section className="card span-12">
          <header className="card-head">
            <h3>执行记录</h3>
          </header>
          {actions === null ? (
            <Skeleton h={120} r={10} />
          ) : actions.length === 0 ? (
            <Empty>还没有自动评论</Empty>
          ) : (
            <div className="table-scroll">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>时间</th>
                    <th>类型</th>
                    <th>账号</th>
                    <th>作品</th>
                    <th>内容</th>
                    <th>状态</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {actions.map((a) => (
                    <tr key={a.id}>
                      <td className="nowrap muted">{fmtTime(a.status === 'pending' ? a.run_at : a.updated_at)}</td>
                      <td className="nowrap">
                        {SOURCE_LABEL[a.source]}
                        {a.rule_name && <div className="muted small">{a.rule_name}</div>}
                      </td>
                      <td className="nowrap">
                        <span className="hstack sm">
                          <PlatformIcon p={platform(a.platform)} size={16} />
                          {a.account}
                        </span>
                      </td>
                      <td className="clamp-cell">
                        {a.item_url ? (
                          <a href={a.item_url} target="_blank" rel="noreferrer">
                            {a.item_title ?? a.item_id}
                          </a>
                        ) : (
                          (a.item_title ?? a.item_id)
                        )}
                      </td>
                      <td className="clamp-cell">{a.text}</td>
                      <td>
                        <span className={`status status-${STATUS_CLASS[a.status]}`}>
                          <i />
                          {ACTION_STATUS[a.status]}
                        </span>
                        {a.error && <div className={'small ' + (a.status === 'failed' ? 'danger-text' : 'muted')}>{a.error}</div>}
                      </td>
                      <td>
                        {a.status === 'pending' && (
                          <button className="btn small" onClick={() => cancel(a)}>
                            取消
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
      </div>

      {draft && (
        <RuleEditor
          draft={draft}
          onClose={() => setDraft(null)}
          onSaved={() => {
            setDraft(null)
            load()
          }}
        />
      )}
    </div>
  )
}

function Stat({ label, value, danger }: { label: string; value: number | undefined; danger?: boolean }) {
  return (
    <section className="card kpi span-3">
      <div className="kpi-top">
        <span className="muted">{label}</span>
      </div>
      <div className={'kpi-value' + (danger ? ' danger-text' : '')}>{value === undefined ? <Skeleton h={32} w={48} /> : value}</div>
    </section>
  )
}

function RuleEditor({ draft, onClose, onSaved }: { draft: Draft; onClose: () => void; onSaved: () => void }) {
  const { accounts, platform } = useStore()
  const toast = useToast()
  const [d, setD] = useState<Draft>(draft)
  const [keywords, setKeywords] = useState(draft.keywords.join('，'))
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState<string | null>(null)
  const accs = accounts.filter((a) => (d.platform ? a.platform === d.platform : COMMENT_PLATFORMS.includes(a.platform)))
  const accountNames = [...new Set(accs.map((a) => a.account))]

  async function save() {
    setErr(null)
    setBusy(true)
    const body = { ...d, keywords: keywords.split(/[,，\n]/).map((k) => k.trim()).filter(Boolean) }
    try {
      if (d.id) await api.updateRule(body as Rule)
      else await api.createRule(body)
      toast('success', '规则已保存')
      onSaved()
    } catch (e) {
      setErr((e as Error).message)
    } finally {
      setBusy(false)
    }
  }

  return (
    <Modal title={d.kind === 'keyword' ? '关键词自动回复' : '数据达标后追评'} onClose={onClose}>
      <div className="stack">
        <label className="field">
          <span>规则名称</span>
          <input value={d.name} onChange={(e) => setD({ ...d, name: e.target.value })} />
        </label>
        <div className="form-grid two">
          <label className="field">
            <span>平台</span>
            <select value={d.platform ?? ''} onChange={(e) => setD({ ...d, platform: e.target.value || null, account: null })}>
              <option value="">抖音、B站、X 全部</option>
              {COMMENT_PLATFORMS.map((p) => (
                <option key={p} value={p}>
                  {platform(p)?.name ?? p}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span>账号</span>
            <select value={d.account ?? ''} onChange={(e) => setD({ ...d, account: e.target.value || null })}>
              <option value="">所有账号</option>
              {accountNames.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </select>
          </label>
        </div>
        {d.kind === 'keyword' ? (
          <label className="field">
            <span>关键词（逗号或换行分隔，命中任意一个即回复，不区分大小写）</span>
            <textarea rows={2} value={keywords} onChange={(e) => setKeywords(e.target.value)} placeholder="链接，多少钱，怎么买" />
          </label>
        ) : (
          <div className="form-grid two">
            <label className="field">
              <span>指标</span>
              <select value={d.metric ?? 'likes'} onChange={(e) => setD({ ...d, metric: e.target.value as Rule['metric'] })}>
                {METRICS.map((m) => (
                  <option key={m} value={m}>
                    {METRIC_LABEL[m]}
                  </option>
                ))}
              </select>
            </label>
            <label className="field">
              <span>达到</span>
              <input type="number" min={1} value={d.threshold ?? ''} onChange={(e) => setD({ ...d, threshold: e.target.value ? Number(e.target.value) : null })} />
            </label>
          </div>
        )}
        <label className="field">
          <span>{d.kind === 'keyword' ? '回复内容' : '追评内容'}</span>
          <textarea rows={3} maxLength={500} value={d.text} onChange={(e) => setD({ ...d, text: e.target.value })} />
        </label>
        <p className="muted small">
          {d.kind === 'keyword'
            ? '每条评论最多自动回复一次，命中后 1–5 分钟内发出。只回复规则创建之后的新评论。'
            : '每个作品只追评一次，在数据同步发现达标后 1–10 分钟内发出。只对规则创建之后发布的作品生效。'}
        </p>
        {err && <div className="error-box">{err}</div>}
        <div className="hstack end">
          <button className="btn" onClick={onClose}>
            取消
          </button>
          <button className="btn primary" disabled={busy} onClick={save}>
            保存
          </button>
        </div>
      </div>
    </Modal>
  )
}
