import { Bot, CalendarDays, Cable, Compass, LayoutDashboard, ListChecks, PenSquare, Users, type LucideIcon } from 'lucide-react'

export type NavItem = { to: string; label: string; icon: LucideIcon; desc: string }

export const NAV_GROUPS: { label: string; items: NavItem[] }[] = [
  {
    label: '工作台',
    items: [
      { to: '/', label: '概览', icon: LayoutDashboard, desc: '发布数据、即将发布、通道健康' },
      { to: '/discover', label: '聚合浏览', icon: Compass, desc: '多平台搜索与信息流' },
      { to: '/happyclaw', label: 'HappyClaw', icon: Bot, desc: 'AI 智能体工作台' },
    ],
  },
  {
    label: '发布',
    items: [
      { to: '/compose', label: '一键发布', icon: PenSquare, desc: '写一次，发到多个平台' },
      { to: '/posts', label: '发布记录', icon: ListChecks, desc: '每个平台的发布状态' },
      { to: '/calendar', label: '发布日历', icon: CalendarDays, desc: '定时任务月视图' },
    ],
  },
  {
    label: '设置',
    items: [
      { to: '/accounts', label: '账号管理', icon: Users, desc: '登录各平台账号' },
      { to: '/providers', label: '通道设置', icon: Cable, desc: '发布通道与回退顺序' },
    ],
  },
]

export const NAV: NavItem[] = NAV_GROUPS.flatMap((g) => g.items)
