import { Menu, Monitor, Moon, Plus, Search, Sun, Zap } from 'lucide-react'
import { useEffect, useState } from 'react'
import { Link, NavLink, Navigate, Route, Routes, useLocation } from 'react-router-dom'
import { StoreProvider, useStore } from './store'
import { ToastProvider } from './components/toast'
import CommandPalette from './components/CommandPalette'
import { NAV, NAV_GROUPS } from './nav'
import { useTheme, type ThemePref } from './theme'
import Accounts from './pages/Accounts'
import Calendar from './pages/Calendar'
import Compose from './pages/Compose'
import Dashboard from './pages/Dashboard'
import Discover from './pages/Discover'
import HappyClaw from './pages/HappyClaw'
import Posts from './pages/Posts'
import Providers from './pages/Providers'

const THEMES: { id: ThemePref; icon: typeof Sun; label: string }[] = [
  { id: 'light', icon: Sun, label: '浅色' },
  { id: 'dark', icon: Moon, label: '深色' },
  { id: 'system', icon: Monitor, label: '跟随系统' },
]

function Shell() {
  const { error, accounts } = useStore()
  const [theme, setTheme] = useTheme()
  const [cmdk, setCmdk] = useState(false)
  const [menu, setMenu] = useState(false)
  const loc = useLocation()
  const current = NAV.find((n) => (n.to === '/' ? loc.pathname === '/' : loc.pathname.startsWith(n.to)))

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        setCmdk((v) => !v)
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  // Close the mobile drawer on navigation.
  useEffect(() => setMenu(false), [loc.pathname])

  const isMac = navigator.platform.toLowerCase().includes('mac')

  return (
    <div className={'shell' + (menu ? ' menu-open' : '')}>
      <aside className="sidebar">
        <Link to="/" className="brand">
          <span className="brand-mark">
            <Zap size={16} strokeWidth={2.5} />
          </span>
          <span>
            聚合台
            <small>Multi-platform studio</small>
          </span>
        </Link>

        <Link to="/compose" className="btn primary block new-post">
          <Plus size={16} /> 新发布
        </Link>

        <nav>
          {NAV_GROUPS.map((g) => (
            <div key={g.label} className="nav-group">
              <div className="nav-label">{g.label}</div>
              {g.items.map((n) => {
                const I = n.icon
                return (
                  <NavLink key={n.to} to={n.to} end={n.to === '/'}>
                    <I size={17} />
                    <span className="grow">{n.label}</span>
                    {n.to === '/accounts' && accounts.length > 0 && <span className="count">{accounts.length}</span>}
                  </NavLink>
                )
              })}
            </div>
          ))}
        </nav>

        <div className="sidebar-foot">
          <div className="seg" role="radiogroup" aria-label="主题">
            {THEMES.map((t) => {
              const I = t.icon
              return (
                <button key={t.id} className={theme === t.id ? 'on' : ''} onClick={() => setTheme(t.id)} title={t.label} aria-label={t.label}>
                  <I size={14} />
                </button>
              )
            })}
          </div>
          <p>数据与登录态仅保存在本机</p>
        </div>
      </aside>
      <div className="scrim" onClick={() => setMenu(false)} />

      <div className="main-wrap">
        <header className="topbar">
          <button className="icon-btn mobile-only" onClick={() => setMenu(true)} aria-label="菜单">
            <Menu size={18} />
          </button>
          <div className="crumb">
            <span className="muted">聚合台</span>
            <span className="muted">/</span>
            <b>{current?.label ?? ''}</b>
          </div>
          <button className="search-trigger" onClick={() => setCmdk(true)}>
            <Search size={15} />
            <span>搜索内容或跳转…</span>
            <kbd>{isMac ? '⌘' : 'Ctrl'} K</kbd>
          </button>
          <span className={'conn ' + (error ? 'bad' : 'ok')} title={error ?? '后端已连接'}>
            <i /> {error ? '后端未连接' : '已连接'}
          </span>
        </header>
        <main className="main">
          {error && <div className="banner error">后端连接失败：{error}（确认 backend 已启动）</div>}
          <div className="route-fade" key={loc.pathname}>
            <Routes>
              <Route path="/" element={<Dashboard />} />
              <Route path="/discover" element={<Discover />} />
              <Route path="/happyclaw" element={<HappyClaw />} />
              <Route path="/compose" element={<Compose />} />
              <Route path="/posts" element={<Posts />} />
              <Route path="/calendar" element={<Calendar />} />
              <Route path="/accounts" element={<Accounts />} />
              <Route path="/providers" element={<Providers />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
          </div>
        </main>
      </div>
      <CommandPalette open={cmdk} onClose={() => setCmdk(false)} setTheme={setTheme} />
    </div>
  )
}

export default function App() {
  return (
    <StoreProvider>
      <ToastProvider>
        <Shell />
      </ToastProvider>
    </StoreProvider>
  )
}
