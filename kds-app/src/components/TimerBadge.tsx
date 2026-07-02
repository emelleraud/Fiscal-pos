import { useEffect, useState } from 'react'
import type { TimerThresholds } from '../types'

function formatElapsed(secs: number): string {
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return m > 0 ? `${m}m${String(s).padStart(2, '0')}` : `${s}s`
}

interface Props {
  arrivedAt: number
  thresholds: TimerThresholds
}

export default function TimerBadge({ arrivedAt, thresholds }: Props) {
  const [now, setNow] = useState(Date.now())

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(id)
  }, [])

  const elapsed = Math.floor((now - arrivedAt) / 1000)
  const label = formatElapsed(elapsed)

  let colorClass: string
  if (elapsed >= thresholds.critical_secs) {
    colorClass = 'bg-red-600 text-white animate-pulse'
  } else if (elapsed >= thresholds.warning_secs) {
    colorClass = 'bg-orange-500 text-white'
  } else {
    colorClass = 'bg-green-600 text-white'
  }

  return (
    <span className={`inline-block px-2 py-0.5 rounded text-sm font-mono font-bold ${colorClass}`}>
      ⏱ {label}
    </span>
  )
}
