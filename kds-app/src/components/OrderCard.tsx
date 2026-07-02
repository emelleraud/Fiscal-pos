import { useState } from 'react'
import type { KdsLine, StationRole, TrackedOrder } from '../types'
import { ackLine, ackOrder, markServed } from '../api'
import TimerBadge from './TimerBadge'

const CHANNEL_ICON: Record<string, string> = {
  caisse: '🖥',
  kiosk: '📱',
  drive: '🚗',
  delivery: '🛵',
}

const STAGE_BADGE: Record<string, string> = {
  preparation: 'PREP',
  holding: 'HOLD',
  assembly: 'ASSE',
}

function LineRow({
  line,
  onAck,
}: {
  line: KdsLine
  onAck: (lineId: string) => void
}) {
  const indent = line.parent_line_id !== null ? 'ml-5' : ''
  const isModifier = line.line_type === 'modifier' || line.line_type === 'combo_component'
  const isComment = line.line_type === 'comment'

  return (
    <div className={`flex items-start gap-2 py-0.5 ${indent} ${line.acknowledged ? 'opacity-40 line-through' : ''}`}>
      {!isModifier && !isComment ? (
        <button
          onClick={() => onAck(line.line_id)}
          disabled={line.acknowledged}
          className="mt-0.5 w-5 h-5 flex-shrink-0 border-2 border-gray-400 rounded disabled:opacity-30 hover:border-white transition-colors"
          aria-label={`Ack ${line.product_name}`}
        />
      ) : (
        <span className="w-5 flex-shrink-0 text-gray-500 text-xs mt-0.5">↳</span>
      )}
      <span className={`text-sm ${isComment ? 'italic text-gray-400' : ''}`}>
        {!isModifier && !isComment && line.quantity > 1 && (
          <span className="font-bold">{line.quantity}× </span>
        )}
        {isComment ? `💬 ${line.product_name}` : line.product_name}
        {line.comment && (
          <span className="ml-1 text-xs text-yellow-300 italic">({line.comment})</span>
        )}
      </span>
    </div>
  )
}

interface Props {
  order: TrackedOrder
  stationId: string
  role: StationRole
}

export default function OrderCard({ order, stationId, role }: Props) {
  const [bumping, setBumping] = useState(false)

  const isModified = order.status === 'modified'
  const isCancelled = order.status === 'cancelled'

  const headerBg = isCancelled
    ? 'bg-red-800 animate-pulse-slow'
    : isModified
    ? 'bg-orange-700 animate-pulse-slow'
    : 'bg-gray-700'

  const handleAckLine = async (lineId: string) => {
    try {
      await ackLine(order.order_id, stationId, lineId)
    } catch {
      // Erreur non bloquante — le state SSE se re-synchronisera
    }
  }

  const handleBumpAll = async () => {
    setBumping(true)
    try {
      await ackOrder(order.order_id, stationId)
    } catch {
      // idem
    } finally {
      setBumping(false)
    }
  }

  const handleServed = async () => {
    setBumping(true)
    try {
      await markServed(order.order_id, stationId)
    } catch {
      // idem
    } finally {
      setBumping(false)
    }
  }

  const rootLines = order.lines.filter((l) => l.parent_line_id === null)
  const totalArticles = rootLines.reduce((sum, l) => sum + l.quantity, 0)
  const allAcked = order.lines.every((l) => l.acknowledged)

  const channelIcon = CHANNEL_ICON[order.channel] ?? '?'
  const stageBadge = STAGE_BADGE[order.stage] ?? order.stage.toUpperCase()

  return (
    <div className="w-72 flex-shrink-0 rounded-lg overflow-hidden border border-gray-700 bg-gray-800 shadow-lg">
      {/* En-tête */}
      <div className={`px-3 py-2 flex items-center justify-between gap-2 ${headerBg}`}>
        <div className="flex items-center gap-2 min-w-0">
          <span className="font-bold text-lg">[{order.order_number_short}]</span>
          <span className="text-base">{channelIcon}</span>
          {order.customer_name && (
            <span className="text-sm text-gray-200 truncate">{order.customer_name}</span>
          )}
          {order.external_order_id && (
            <span className="text-xs text-gray-300 font-mono truncate">{order.external_order_id}</span>
          )}
        </div>
        <div className="flex items-center gap-1 flex-shrink-0">
          <TimerBadge arrivedAt={order.arrived_at} thresholds={order.timer_thresholds} />
          <span className="text-xs bg-gray-600 rounded px-1 py-0.5">{stageBadge}</span>
        </div>
      </div>

      {/* Lignes */}
      <div className="px-3 py-2 min-h-[80px]">
        {order.lines.map((line) => (
          <LineRow key={line.line_id} line={line} onAck={handleAckLine} />
        ))}
      </div>

      {/* Station statuses (vue assemblage) */}
      {role === 'assembly' && Object.keys(order.station_statuses).length > 0 && (
        <div className="px-3 py-1 border-t border-gray-700 space-y-0.5">
          {Object.entries(order.station_statuses).map(([name, status]) => (
            <div key={name} className="flex justify-between text-xs text-gray-300">
              <span>{name}</span>
              <span>
                {status === 'ready' ? '✅' : status === 'in_progress' ? '⏳' : '⬜'} {status}
              </span>
            </div>
          ))}
        </div>
      )}

      {/* Pied de page */}
      <div className="px-3 py-2 border-t border-gray-700 flex items-center justify-between">
        <span className="text-xs text-gray-400">{totalArticles} article{totalArticles > 1 ? 's' : ''}</span>
        <div className="flex gap-2">
          {role === 'assembly' ? (
            <>
              {!allAcked && (
                <button
                  onClick={handleBumpAll}
                  disabled={bumping}
                  className="text-xs bg-blue-600 hover:bg-blue-700 text-white px-2 py-1 rounded disabled:opacity-50"
                >
                  PRÊT →
                </button>
              )}
              {allAcked && (
                <button
                  onClick={handleServed}
                  disabled={bumping}
                  className="text-xs bg-green-600 hover:bg-green-700 text-white px-2 py-1 rounded disabled:opacity-50"
                >
                  SERVI ✓
                </button>
              )}
            </>
          ) : (
            <button
              onClick={handleBumpAll}
              disabled={bumping || allAcked}
              className="text-xs bg-blue-600 hover:bg-blue-700 text-white px-2 py-1 rounded disabled:opacity-50"
            >
              TOUT ✓
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
