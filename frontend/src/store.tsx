import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react'
import { api, type Account, type Platform } from './api'

type Store = {
  platforms: Platform[]
  accounts: Account[]
  loading: boolean
  error: string | null
  refreshAccounts: () => Promise<void>
  platform: (id: string) => Platform | undefined
}

const Ctx = createContext<Store | null>(null)

export function StoreProvider({ children }: { children: ReactNode }) {
  const [platforms, setPlatforms] = useState<Platform[]>([])
  const [accounts, setAccounts] = useState<Account[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const refreshAccounts = useCallback(async () => {
    try {
      setAccounts(await api.accounts())
      setError(null)
    } catch (e) {
      setError((e as Error).message)
    }
  }, [])

  useEffect(() => {
    Promise.all([api.platforms().then(setPlatforms), refreshAccounts()])
      .catch((e) => setError((e as Error).message))
      .finally(() => setLoading(false))
  }, [refreshAccounts])

  const platform = useCallback((id: string) => platforms.find((p) => p.id === id), [platforms])

  return <Ctx.Provider value={{ platforms, accounts, loading, error, refreshAccounts, platform }}>{children}</Ctx.Provider>
}

// eslint-disable-next-line react-refresh/only-export-components
export function useStore(): Store {
  const s = useContext(Ctx)
  if (!s) throw new Error('StoreProvider missing')
  return s
}
