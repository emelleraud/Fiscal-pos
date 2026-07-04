import { useCallback, useEffect, useRef, useState } from 'react'
import type { KdsAckPayload, KdsOrderPayload, KdsOrderUpdate, TrackedOrder } from '../types'

const BASE = import.meta.env.VITE_EDGE_API_URL as string

type OrderMap = Map<string, TrackedOrder>

function applyOrderNew(prev: OrderMap, payload: KdsOrderPayload): OrderMap {
  const next = new Map(prev)
  const existing = next.get(payload.order_id)
  // Garde la plus récente version (même order_id peut arriver plusieurs fois, une par station)
  if (!existing) {
    next.set(payload.order_id, { ...payload, status: 'new' })
  }
  return next
}

function applyOrderUpdated(prev: OrderMap, update: KdsOrderUpdate): OrderMap {
  const existing = prev.get(update.order_id)
  if (!existing) return prev
  const next = new Map(prev)
  next.set(update.order_id, {
    ...existing,
    status: update.status,
    stage: update.stage,
    station_statuses: update.station_statuses,
  })
  return next
}

function applyOrderAcked(prev: OrderMap, ack: KdsAckPayload): OrderMap {
  const existing = prev.get(ack.order_id)
  if (!existing) return prev
  const next = new Map(prev)
  const newLines = existing.lines.map((l) =>
    ack.line_id === null || l.line_id === ack.line_id
      ? { ...l, acknowledged: true }
      : l
  )
  next.set(ack.order_id, { ...existing, lines: newLines })
  return next
}

/**
 * Démarre un interval qui signale la présence de l'écran au edge-api toutes les 10 s.
 * Retourne une fonction d'arrêt à appeler au unmount (clearInterval).
 */
export function startHeartbeat(stationId: string, baseUrl: string): () => void {
  const id = setInterval(() => {
    fetch(`${baseUrl}/api/v1/kds/heartbeat/${stationId}`, { method: 'POST' }).catch(() => {})
  }, 10_000)
  return () => clearInterval(id)
}

/**
 * Ouvre un flux SSE vers `/api/v1/kds/feed/:stationId`.
 * Reconnexion automatique avec backoff exponentiel (1 s → 30 s max).
 * Bandeau rouge si déconnecté > 3 s (via `connected = false`).
 */
export function useKdsFeed(stationId: string): { orders: TrackedOrder[]; connected: boolean } {
  const [orderMap, setOrderMap] = useState<OrderMap>(new Map())
  const [connected, setConnected] = useState(false)
  const retryTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const attempts = useRef(0)
  const esRef = useRef<EventSource | null>(null)
  const stationIdRef = useRef(stationId)
  stationIdRef.current = stationId

  const connect = useCallback(() => {
    const url = `${BASE}/api/v1/kds/feed/${stationIdRef.current}`
    const es = new EventSource(url)
    esRef.current = es

    es.addEventListener('order_new', (e: MessageEvent<string>) => {
      const wrapper = JSON.parse(e.data) as { data: KdsOrderPayload }
      setOrderMap((prev) => applyOrderNew(prev, wrapper.data))
      attempts.current = 0
    })

    es.addEventListener('order_updated', (e: MessageEvent<string>) => {
      const wrapper = JSON.parse(e.data) as { data: KdsOrderUpdate }
      setOrderMap((prev) => applyOrderUpdated(prev, wrapper.data))
    })

    es.addEventListener('order_acked', (e: MessageEvent<string>) => {
      const wrapper = JSON.parse(e.data) as { data: KdsAckPayload }
      setOrderMap((prev) => applyOrderAcked(prev, wrapper.data))
    })

    es.onopen = () => {
      setConnected(true)
      attempts.current = 0
    }

    es.onerror = () => {
      setConnected(false)
      es.close()
      esRef.current = null
      const delay = Math.min(1000 * 2 ** attempts.current, 30_000)
      attempts.current += 1
      retryTimer.current = setTimeout(connect, delay)
    }
  }, [])

  useEffect(() => {
    connect()
    const stopHeartbeat = startHeartbeat(stationIdRef.current, BASE)
    return () => {
      esRef.current?.close()
      if (retryTimer.current !== null) clearTimeout(retryTimer.current)
      stopHeartbeat()
    }
  }, [connect, stationId])

  const orders = Array.from(orderMap.values()).sort((a, b) => a.arrived_at - b.arrived_at)
  return { orders, connected }
}
