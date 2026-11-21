// Types mirror the Rust backend and catbus' normalized objects.
import { BASE } from './base'

export type OptionSpec = {
  key: string
  label: string
  kind: 'number' | 'text' | 'bool' | 'select'
  choices: string[]
  required: boolean
  help: string
}

export type PublishSpec = {
  title: boolean
  title_required: boolean
  title_max: number | null
  text_max: number | null
  images: boolean
  max_images: number
  video: boolean
  video_requires_cover: boolean
  cover: boolean
  requires_media: boolean
  tags: boolean
  topics: boolean
  visibility: string[]
  options: OptionSpec[]
  notes: string[]
}

export type Platform = {
  id: string
  name: string
  color: string
  login_methods: string[]
  cookie_site: string
  cookie_keys: string
  can_search: boolean
  can_feed: boolean
  can_comments: boolean
  publish: PublishSpec | null
  providers: string[]
}

export type UserRef = { id: string; name: string | null; url: string | null; avatar: string | null }

export type Account = {
  platform: string
  endpoint: string
  account: string
  current: boolean
  user: UserRef | null
  method: string | null
  updated_at: string | null
  providers: string[]
}

export type CatbusError = { code: string; message: string; hint: string | null; detail: unknown }

export type Envelope<T> = {
  ok: boolean
  platform: string | null
  account: string | null
  data: T
  page: { cursor: string | null; has_more: boolean } | null
  error: CatbusError | null
}

export type MediaObj = { id: string; type: string; url: string | null; width: number | null; height: number | null; duration: number | null }

export type Stats = { views?: number | null; likes?: number | null; comments?: number | null; collects?: number | null; shares?: number | null }

export type Item = {
  id: string
  kind: string | null
  url: string | null
  title: string | null
  text: string | null
  author: UserRef | null
  created_at: string | null
  cover: string | null
  media: MediaObj[]
  stats: Stats | null
  price: { amount: number; currency: string } | null
}

export type Comment = {
  id: string
  author: UserRef | null
  text: string | null
  created_at: string | null
  stats: { likes: number | null; replies: number | null } | null
}

export type WorkItem = Item & { _platform?: string; _account?: string }
export type AccountWorks = {
  platform: string
  account: string
  ok: boolean
  error: string | null
  count: number
  capped?: boolean
  likes: number
  views: number
  works: WorkItem[]
}
export type Works = {
  accounts: AccountWorks[]
  totals: { count: number; likes: number; capped: boolean }
  recent: WorkItem[]
  fetched_at: string
}

export type Media = { id: string; filename: string; kind: 'image' | 'video'; mime: string; size: number; url: string }

export type TargetStatus = 'pending' | 'running' | 'success' | 'failed' | 'canceled'

export type Target = {
  id: string
  post_id: string
  platform: string
  account: string
  title: string | null
  text: string | null
  visibility: string | null
  options: Record<string, unknown>
  status: TargetStatus
  run_at: string
  attempts: number
  error_code: string | null
  last_error: string | null
  item_id: string | null
  item_url: string | null
  updated_at: string
  provider: string | null
  force_provider: string | null
  attempt_log: { provider: string; ok: boolean; code: string | null; message: string | null; at: string }[]
}

export type ProviderSettings = {
  order: Record<string, string[]>
  xhs_mcp: { instances: { account: string; url: string; token: string }[] }
  x_api: {
    client_id: string
    client_secret: string
    redirect_uri: string
    accounts: { account: string; user_id: string; username: string; name: string; expires_at: string }[]
  }
  sau: { bin: string; accounts: { platform: string; account: string; sau_account: string }[] }
}

export type ProvidersOverview = {
  providers: { id: string; label: string }[]
  order: Record<string, { order: string[]; supported: string[] }>
  settings: ProviderSettings
  x_redirect_uri: string
  catbus: { ok: boolean; version?: string; error?: string }
}

export const PROVIDER_LABEL: Record<string, string> = {
  catbus: 'catbus',
  'xhs-mcp': 'xiaohongshu-mcp',
  'x-api': 'X 官方 API',
  sau: 'social-auto-upload',
}

export type Post = {
  id: string
  title: string | null
  text: string
  images: Media[]
  video: Media | null
  cover: Media | null
  tags: string[]
  topics: string[]
  scheduled_at: string | null
  created_at: string
  first_comment: string | null
  first_comment_delay: number | null
  targets: Target[]
}

export type NewTarget = {
  platform: string
  account: string
  title?: string | null
  text?: string | null
  visibility?: string | null
  options?: Record<string, unknown>
}

export type NewPost = {
  title: string | null
  text: string
  images: string[]
  video: string | null
  cover: string | null
  tags: string[]
  topics: string[]
  scheduled_at: string | null
  first_comment?: string | null
  first_comment_delay?: number | null
  targets: NewTarget[]
}

export type LoginStatus = {
  id: string
  platform: string
  account: string
  /** verify: second verification required; need_code: waiting for the SMS code */
  state: 'waiting' | 'scanned' | 'verify' | 'need_code' | 'success' | 'failed'
  message: string | null
  qrcode_ready: boolean
  qr_seq: number
  started_at: string
}

// ---- engagement: analytics, inbox, automation ----

/** Platforms where catbus can post comments (mirrors backend engage::CAPS). */
export const COMMENT_PLATFORMS = ['douyin', 'bilibili', 'x']

export const METRICS = ['views', 'likes', 'comments', 'collects', 'shares'] as const
export type Metric = (typeof METRICS)[number]
export const METRIC_LABEL: Record<Metric, string> = { views: '播放/阅读', likes: '点赞', comments: '评论', collects: '收藏', shares: '分享' }
export type Metrics = Record<Metric, number | null>

export type AccountSync = { platform: string; account: string; ok: boolean; error: string | null; items: number; new_inbox: number; at: string }
export type SyncStatus = { running: boolean; started_at: string | null; finished_at: string | null; next_at: string | null; accounts: AccountSync[] }

export type AnalyticsSummary = {
  platforms: (Metrics & { platform: string; works: number; accounts: number; synced_at: string | null })[]
  totals: Metrics
  last7: Metrics
  daily: (Metrics & { date: string })[]
  sync: SyncStatus
}

export type TrackedItem = {
  platform: string
  account: string
  item_id: string
  kind: string | null
  title: string | null
  url: string | null
  cover: string | null
  published_at: string | null
  post_id: string | null
  synced_at: string
  stats: Metrics
  delta24h: Metrics
}

export type InboxKind = 'comment' | 'mention' | 'dm'
export type InboxItem = {
  id: string
  platform: string
  account: string
  kind: InboxKind
  item_id: string | null
  item_url: string | null
  item_title: string | null
  author: { id: string | null; name: string | null; url: string | null; avatar: string | null }
  text: string | null
  created_at: string | null
  fetched_at: string
  sort_key: string
  is_read: boolean
  replied_at: string | null
  reply_text: string | null
  can_reply: boolean
}
export type UnreadCounts = Partial<Record<InboxKind, number>> & { total: number }
export type Message = { id: string; conversation_id: string; from: UserRef | null; type: string; text: string | null; created_at: string | null }

export type AutomationSettings = { sync_minutes: number; window_days: number; daily_limit: number; auto_enabled: boolean }
export type Rule = {
  id: string
  kind: 'keyword' | 'milestone'
  enabled: boolean
  name: string
  platform: string | null
  account: string | null
  keywords: string[]
  metric: Metric | null
  threshold: number | null
  text: string
  created_at: string
}
export type AutoAction = {
  id: string
  source: 'first_comment' | 'milestone' | 'keyword'
  rule_id: string | null
  rule_name: string | null
  platform: string
  account: string
  item_id: string
  item_title: string | null
  item_url: string | null
  reply_to: string | null
  text: string
  status: 'pending' | 'running' | 'success' | 'failed' | 'canceled'
  run_at: string
  error_code: string | null
  error: string | null
  updated_at: string
}
export type Automation = {
  settings: AutomationSettings
  rules: Rule[]
  stats: { pending: number; sent24h: number; failed24h: number }
}

// When the app is served under a path prefix (e.g. /mpf/ behind a reverse
// proxy), root-relative API and media URLs must carry it; external (http...)
// URLs pass through untouched.
/** Prefix a backend root-relative URL ('/media/x', '/api/..') with the base path. */
export const mediaUrl = (u: string | null | undefined): string => (u && u.startsWith('/') && !u.startsWith('//') ? BASE + u : (u ?? ''))

export class ApiError extends Error {
  details: string[]
  constructor(message: string, details: string[] = []) {
    super(message)
    this.details = details
  }
}

async function req<T>(method: string, path: string, body?: unknown): Promise<T> {
  const isForm = body instanceof FormData
  const r = await fetch(BASE + '/api' + path, {
    method,
    headers: body && !isForm ? { 'content-type': 'application/json' } : undefined,
    body: isForm ? body : body !== undefined ? JSON.stringify(body) : undefined,
  })
  if (r.status === 204) return undefined as T
  const j = await r.json().catch(() => null)
  if (!r.ok) throw new ApiError(j?.error ?? `${r.status} ${r.statusText}`, j?.details ?? [])
  return j as T
}

const qs = (o: Record<string, string | number | undefined | null>) =>
  new URLSearchParams(Object.entries(o).filter(([, v]) => v !== undefined && v !== null && v !== '') as [string, string][]).toString()

export const api = {
  platforms: () => req<Platform[]>('GET', '/platforms'),
  accounts: () => req<Account[]>('GET', '/accounts'),
  checkAccount: (p: string, account: string) => req<Envelope<{ logged_in: boolean; user: UserRef | null }>>('GET', `/accounts/${p}/status?${qs({ account })}`),
  logout: (p: string, account: string) => req<Envelope<unknown>>('DELETE', `/accounts/${p}/${encodeURIComponent(account)}`),
  cookieLogin: (p: string, account: string, cookie: string) => req<Envelope<unknown>>('POST', `/accounts/${p}/cookie`, { account, cookie }),
  qrStart: (p: string, account: string) => req<LoginStatus>('POST', `/login/${p}`, { account }),
  qrStatus: (p: string) => req<LoginStatus>('GET', `/login/${p}`),
  qrCode: (p: string, code: string) => req<LoginStatus>('POST', `/login/${p}/code`, { code }),
  qrCancel: (p: string) => req<unknown>('DELETE', `/login/${p}`),
  qrImageUrl: (p: string, bust: string) => `${BASE}/api/login/${p}/qrcode?t=${encodeURIComponent(bust)}`,

  search: (q: string, platforms: string[], sort?: string) =>
    req<{ platform: string; envelope: Envelope<Item[] | null>; via: string }[]>('GET', `/search?${qs({ q, platforms: platforms.join(','), sort, limit: 20 })}`),
  feed: (platform: string, cursor?: string | null) => req<Envelope<Item[] | null> & { via: string }>('GET', `/feed?${qs({ platform, cursor, limit: 20 })}`),
  item: (platform: string, item: string) => req<Envelope<Item | null>>('GET', `/item?${qs({ platform, item })}`),
  works: (refresh = false) => req<Works>('GET', `/works${refresh ? '?refresh=1' : ''}`),
  comments: (platform: string, item: string) => req<Envelope<Comment[] | null>>('GET', `/comments?${qs({ platform, item, limit: 30 })}`),

  upload: (files: File[]) => {
    const fd = new FormData()
    files.forEach((f) => fd.append('file', f))
    return req<Media[]>('POST', '/media', fd)
  },
  posts: (from?: string, to?: string) => req<Post[]>('GET', `/posts?${qs({ from, to, limit: 300 })}`),
  createPost: (p: NewPost) => req<Post>('POST', '/posts', p),
  cancelPost: (id: string) => req<Post>('POST', `/posts/${id}/cancel`),
  deletePost: (id: string) => req<void>('DELETE', `/posts/${id}`),
  reschedule: (id: string, scheduled_at: string | null) => req<Post>('POST', `/posts/${id}/schedule`, { scheduled_at }),
  retryTarget: (id: string, provider?: string) => req<Target>('POST', `/targets/${id}/retry?${qs({ provider })}`),

  analytics: (days: number) => req<AnalyticsSummary>('GET', `/analytics/summary?${qs({ days, tz: -new Date().getTimezoneOffset() })}`),
  trackedItems: (sort: string, platform?: string) => req<TrackedItem[]>('GET', `/analytics/items?${qs({ sort, platform, limit: 200 })}`),
  itemHistory: (platform: string, account: string, id: string) =>
    req<(Metrics & { at: string })[]>('GET', `/analytics/items/${platform}/${encodeURIComponent(account)}/${encodeURIComponent(id)}`),
  syncStatus: () => req<SyncStatus>('GET', '/sync'),
  syncNow: () => req<{ ok: boolean; already_running: boolean }>('POST', '/sync'),

  inbox: (q: { kind?: string; platform?: string; unread?: boolean; before?: string }) =>
    req<{ items: InboxItem[]; has_more: boolean; unread: UnreadCounts }>('GET', `/inbox?${qs({ ...q, unread: q.unread ? 'true' : undefined, limit: 50 })}`),
  inboxUnread: () => req<UnreadCounts>('GET', '/inbox/unread'),
  markRead: (b: { ids?: string[]; all?: boolean; read?: boolean }) => req<{ unread: UnreadCounts }>('POST', '/inbox/read', b),
  reply: (id: string, text: string) => req<InboxItem>('POST', `/inbox/${encodeURIComponent(id)}/reply`, { text }),
  messages: (id: string) => req<Envelope<Message[] | null>>('GET', `/inbox/${encodeURIComponent(id)}/messages`),

  automation: () => req<Automation>('GET', '/automation'),
  setAutomation: (b: AutomationSettings) => req<AutomationSettings>('PUT', '/automation/settings', b),
  createRule: (r: Omit<Rule, 'id' | 'created_at'>) => req<Rule>('POST', '/automation/rules', r),
  updateRule: (r: Rule) => req<Rule>('PUT', `/automation/rules/${r.id}`, r),
  deleteRule: (id: string) => req<void>('DELETE', `/automation/rules/${id}`),
  autoActions: () => req<AutoAction[]>('GET', '/automation/actions?limit=100'),
  cancelAction: (id: string) => req<unknown>('POST', `/automation/actions/${id}/cancel`),

  providers: () => req<ProvidersOverview>('GET', '/providers'),
  setOrder: (platform: string, order: string[]) => req<unknown>('PUT', '/providers/order', { platform, order }),
  setXApp: (b: { client_id: string; client_secret: string; redirect_uri: string }) => req<unknown>('PUT', '/providers/x-api', b),
  removeXAccount: (account: string) => req<void>('DELETE', `/providers/x-api/${encodeURIComponent(account)}`),
  xAuthorizeUrl: (account: string) => `${BASE}/api/oauth/x/start?${qs({ account })}`,
  upsertXhsMcp: (b: { account: string; url: string; token: string }) => req<unknown>('POST', '/providers/xhs-mcp', b),
  removeXhsMcp: (account: string) => req<void>('DELETE', `/providers/xhs-mcp/${encodeURIComponent(account)}`),
  xhsMcpStatus: (account: string) =>
    req<{ reachable: boolean; logged_in: boolean; username: string | null; error: string | null }>('GET', `/providers/xhs-mcp/${encodeURIComponent(account)}/status`),
  xhsMcpQr: (account: string) => req<{ img: string | null; logged_in: boolean }>('GET', `/providers/xhs-mcp/${encodeURIComponent(account)}/qrcode`),
  setSauBin: (bin: string) => req<unknown>('PUT', '/providers/sau/bin', { bin }),
  upsertSau: (b: { platform: string; account: string; sau_account: string }) => req<unknown>('POST', '/providers/sau', b),
  removeSau: (platform: string, account: string) => req<void>('DELETE', `/providers/sau/${platform}/${encodeURIComponent(account)}`),
  sauCheck: (platform: string, account: string) => req<{ ok: boolean; message: string }>('GET', `/providers/sau/${platform}/${encodeURIComponent(account)}/check`),
}

// ---- client-side validation (mirrors backend platforms::validate) ----

export type Shape = {
  title: string
  text: string
  imageCount: number
  hasVideo: boolean
  hasCover: boolean
  visibility: string
  options: Record<string, unknown>
}

/** X counts CJK and other wide characters as 2 (twitter-text v3, approximately). */
export function xWeight(text: string): number {
  let w = 0
  for (const ch of text) {
    const c = ch.codePointAt(0)!
    const narrow = c <= 0x10ff || (c >= 0x2000 && c <= 0x200d) || (c >= 0x2010 && c <= 0x201f) || (c >= 0x2032 && c <= 0x2037)
    w += narrow ? 1 : 2
  }
  return w
}

export const charLen = (s: string) => [...s].length

export function validate(p: Platform, s: Shape): { errors: string[]; warnings: string[] } {
  const errors: string[] = []
  const warnings: string[] = []
  const spec = p.publish
  if (!spec) return { errors: ['不支持发布'], warnings }
  const title = s.title.trim()
  if (spec.title_required && !title) errors.push('标题必填')
  if (spec.title && spec.title_max && charLen(title) > spec.title_max) errors.push(`标题超过 ${spec.title_max} 字`)
  if (!spec.title && title) warnings.push('不支持标题，标题会被忽略')
  if (p.id === 'x') {
    const w = xWeight(s.text)
    if (w > 280) warnings.push(`正文权重 ${w}/280，将以长推发布（需要 Premium）`)
  } else if (spec.text_max && charLen(s.text) > spec.text_max) {
    errors.push(`正文超过 ${spec.text_max} 字`)
  }
  if (!title && !s.text.trim() && !s.imageCount && !s.hasVideo) errors.push('内容为空')
  if (s.imageCount && !spec.images) errors.push('不支持图片')
  if (spec.images && s.imageCount > spec.max_images) errors.push(`最多 ${spec.max_images} 张图片`)
  if (s.hasVideo && !spec.video) errors.push('不支持视频')
  if (s.hasVideo && s.imageCount) errors.push('图片和视频不能同时发')
  if (s.hasVideo && spec.video_requires_cover && !s.hasCover) errors.push('视频需要封面')
  if (spec.requires_media && !s.imageCount && !s.hasVideo) errors.push(spec.images ? '必须至少一张图片或一个视频' : '必须上传视频')
  for (const o of spec.options) {
    const v = s.options[o.key]
    const empty = v === undefined || v === null || v === '' || v === false
    if (o.required && empty && o.kind !== 'bool') errors.push(`${o.label} 必填`)
    if (o.kind === 'number' && !empty && Number.isNaN(Number(v))) errors.push(`${o.label} 必须是数字`)
  }
  return { errors, warnings }
}

// ---- formatting helpers ----

export function fmtNum(n: number | null | undefined): string {
  if (n === null || n === undefined) return '-'
  if (n >= 1e8) return (n / 1e8).toFixed(1) + '亿'
  if (n >= 1e4) return (n / 1e4).toFixed(1) + '万'
  return String(n)
}

export function fmtTime(iso: string | null | undefined): string {
  if (!iso) return ''
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return d.toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' })
}

/** `<input type="datetime-local">` value <-> ISO string with timezone. */
export function toLocalInput(iso: string | null): string {
  if (!iso) return ''
  const d = new Date(iso)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export function fromLocalInput(v: string): string | null {
  if (!v) return null
  const d = new Date(v)
  return Number.isNaN(d.getTime()) ? null : d.toISOString()
}

export const STATUS_LABEL: Record<TargetStatus, string> = {
  pending: '待发布',
  running: '发布中',
  success: '已发布',
  failed: '失败',
  canceled: '已取消',
}
