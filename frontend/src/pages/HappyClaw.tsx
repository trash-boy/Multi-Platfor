import { Bot, ExternalLink, RotateCcw } from 'lucide-react'
import { useRef, useState } from 'react'

// HappyClaw runs as a sibling app under the same origin (/happyclaw/), so it can
// be embedded directly. This page "打通" the two: you reach the agent workspace
// from inside 聚合台 without a separate tab or login flow.
const HAPPYCLAW_URL = '/happyclaw/'

export default function HappyClaw() {
  const ref = useRef<HTMLIFrameElement>(null)
  const [loading, setLoading] = useState(true)

  const reload = () => {
    setLoading(true)
    if (ref.current) ref.current.src = HAPPYCLAW_URL
  }

  return (
    <div className="embed-page">
      <header className="embed-bar">
        <div className="embed-title">
          <span className="embed-mark">
            <Bot size={15} />
          </span>
          HappyClaw
          <span className="muted small">· AI 智能体工作台</span>
        </div>
        <div className="actions">
          <button className="btn small ghost" onClick={reload}>
            <RotateCcw size={13} /> 重新加载
          </button>
          <a className="btn small ghost" href={HAPPYCLAW_URL} target="_blank" rel="noreferrer">
            <ExternalLink size={13} /> 新标签打开
          </a>
        </div>
      </header>
      <div className="embed-frame">
        {loading && <div className="embed-loading">正在加载 HappyClaw…</div>}
        <iframe
          ref={ref}
          src={HAPPYCLAW_URL}
          title="HappyClaw"
          onLoad={() => setLoading(false)}
          allow="clipboard-read; clipboard-write; microphone; camera"
        />
      </div>
    </div>
  )
}
