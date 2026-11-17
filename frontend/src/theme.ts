import { useEffect, useState } from 'react'

export type ThemePref = 'light' | 'dark' | 'system'

const KEY = 'mpf-theme'
const media = () => window.matchMedia('(prefers-color-scheme: dark)')

function apply(pref: ThemePref) {
  const dark = pref === 'dark' || (pref === 'system' && media().matches)
  document.documentElement.dataset.theme = dark ? 'dark' : 'light'
}

/** Call once before render to avoid a flash of the wrong theme. */
export function initTheme() {
  apply((localStorage.getItem(KEY) as ThemePref) ?? 'system')
}

export function useTheme(): [ThemePref, (p: ThemePref) => void] {
  const [pref, setPref] = useState<ThemePref>(() => (localStorage.getItem(KEY) as ThemePref) ?? 'system')
  useEffect(() => {
    apply(pref)
    localStorage.setItem(KEY, pref)
    if (pref !== 'system') return
    const m = media()
    const on = () => apply('system')
    m.addEventListener('change', on)
    return () => m.removeEventListener('change', on)
  }, [pref])
  return [pref, setPref]
}
