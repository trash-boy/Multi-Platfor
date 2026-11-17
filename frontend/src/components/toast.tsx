import { CircleAlert, CircleCheck, Info, X } from 'lucide-react'
import { createContext, useCallback, useContext, useState, type ReactNode } from 'react'

type Kind = 'success' | 'error' | 'info'
type Toast = { id: number; kind: Kind; title: string; body?: string }
type Push = (kind: Kind, title: string, body?: string) => void

const Ctx = createContext<Push>(() => undefined)

export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<Toast[]>([])
  const close = (id: number) => setItems((t) => t.filter((x) => x.id !== id))
  const push = useCallback<Push>((kind, title, body) => {
    const id = Date.now() + Math.random()
    setItems((t) => [...t.slice(-3), { id, kind, title, body }])
    window.setTimeout(() => close(id), kind === 'error' ? 7000 : 3500)
  }, [])
  const Icon = { success: CircleCheck, error: CircleAlert, info: Info }
  return (
    <Ctx.Provider value={push}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {items.map((t) => {
          const I = Icon[t.kind]
          return (
            <div key={t.id} className={`toast toast-${t.kind}`}>
              <I size={18} className="toast-icon" />
              <div className="grow">
                <div className="toast-title">{t.title}</div>
                {t.body && <div className="toast-body">{t.body}</div>}
              </div>
              <button className="icon-btn" onClick={() => close(t.id)} aria-label="关闭">
                <X size={14} />
              </button>
            </div>
          )
        })}
      </div>
    </Ctx.Provider>
  )
}

// eslint-disable-next-line react-refresh/only-export-components
export const useToast = () => useContext(Ctx)
