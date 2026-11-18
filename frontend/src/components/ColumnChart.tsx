import { useState } from 'react'
import { fmtNum } from '../api'

export type Column = { key: string; label: string; tip: string; value: number }

/**
 * Single-series column chart: one hue (--chart), no legend (the card title
 * names the series), recessive grid, hover/focus tooltip, peak value labelled.
 * Pair it with a table view toggle in the card header.
 */
export function ColumnChart({ data, unit, label, empty }: { data: Column[]; unit: string; label: string; empty: string }) {
  const [hover, setHover] = useState<number | null>(null)
  const max = Math.max(...data.map((d) => d.value), 0)
  const top = niceTop(max)
  const peak = max > 0 ? data.findIndex((d) => d.value === max) : -1
  // Thin out axis labels on long ranges (≈ 7 labels max).
  const step = Math.max(1, Math.ceil(data.length / 7))
  const edge = (i: number) => (i < 2 ? ' tip-left' : i > data.length - 3 ? ' tip-right' : '')

  return (
    <div className="chart wide-axis" role="img" aria-label={`${label}：${data.map((d) => `${d.tip} ${d.value}${unit}`).join('，')}`}>
      <div className="chart-grid">
        {[top, top / 2, 0].map((v) => (
          <div key={v} className="grid-line">
            <span>{fmtNum(v)}</span>
          </div>
        ))}
      </div>
      <div className="chart-cols">
        {data.map((d, i) => (
          <div
            key={d.key}
            className={'chart-col' + (hover === i ? ' hover' : '')}
            onMouseEnter={() => setHover(i)}
            onMouseLeave={() => setHover(null)}
            tabIndex={0}
            onFocus={() => setHover(i)}
            onBlur={() => setHover(null)}
            aria-label={`${d.tip}：${d.value}${unit}`}
          >
            <div className="bar-area">
              {i === peak && hover === null && (
                <span className="bar-value" style={{ bottom: `${(d.value / top) * 100}%` }}>
                  {fmtNum(d.value)}
                </span>
              )}
              <div className="bar" style={{ height: d.value ? `max(${(d.value / top) * 100}%, 3px)` : 0 }} />
              {hover === i && (
                <div className={'chart-tip' + edge(i)} style={{ bottom: `calc(${(d.value / top) * 100}% + 10px)` }}>
                  <b>
                    {d.value.toLocaleString('zh-CN')}
                    {unit}
                  </b>
                  <span>{d.tip}</span>
                </div>
              )}
            </div>
            <div className="col-label">{i % step === 0 || i === data.length - 1 ? d.label : ''}</div>
          </div>
        ))}
      </div>
      {max === 0 && <div className="chart-empty">{empty}</div>}
    </div>
  )
}

/** Round the axis top up to 1/2/5 × 10^n so the midline is a clean number. */
function niceTop(max: number): number {
  if (max <= 4) return 4
  const pow = 10 ** Math.floor(Math.log10(max))
  for (const m of [1, 2, 5, 10]) if (m * pow >= max) return m * pow
  return 10 * pow
}
