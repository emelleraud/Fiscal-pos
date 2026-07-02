import ConnectionBanner from '../components/ConnectionBanner'
import { useKdsFeed } from '../hooks/useKdsFeed'
import type { OrderType, TrackedOrder } from '../types'

const CLIENT_TYPES: OrderType[] = ['takeaway', 'click_and_collect']
const LIVREUR_TYPES: OrderType[] = ['delivery']

function filterByOrbType(orders: TrackedOrder[], orbType: 'client' | 'livreur' | null): TrackedOrder[] {
  if (!orbType) return orders
  const types = orbType === 'client' ? CLIENT_TYPES : LIVREUR_TYPES
  return orders.filter((o) => types.includes(o.order_type))
}

function OrbOrderItem({ order }: { order: TrackedOrder }) {
  return (
    <div className="flex flex-col items-center p-3 rounded-lg bg-gray-800 border border-gray-600">
      <span className="text-4xl font-black tracking-widest">{order.order_number_short}</span>
      {order.customer_name && (
        <span className="text-lg text-gray-300 mt-1">{order.customer_name}</span>
      )}
      {order.external_order_id && (
        <span className="text-sm text-gray-400 font-mono">{order.external_order_id}</span>
      )}
    </div>
  )
}

interface Props {
  orbType: 'client' | 'livreur' | null
}

export default function OrderReadyBoard({ orbType }: Props) {
  const { orders, connected } = useKdsFeed('ready_board')
  const isFullscreen = new URLSearchParams(window.location.search).get('fullscreen') === 'true'

  const filtered = filterByOrbType(orders, orbType)

  // En préparation = tout sauf 'ready' et 'served'
  const inProgress = filtered.filter((o) => o.status !== 'ready' && o.status !== 'served')
  // Prêt = status 'ready' (set quand l'expo bumpe en-tête)
  const ready = filtered.filter((o) => o.status === 'ready')

  const title = orbType === 'livreur' ? 'ORB LIVREUR' : 'ORDER READY BOARD'

  return (
    <div className={`min-h-screen bg-gray-950 text-white flex flex-col ${isFullscreen ? '' : 'p-4'}`}>
      <ConnectionBanner connected={connected} />

      <h1 className="text-center text-xl font-bold tracking-widest text-gray-400 py-3 border-b border-gray-700">
        {title}
      </h1>

      <div className="flex flex-1 overflow-hidden">
        {/* Colonne gauche — En préparation */}
        <div className="flex-1 flex flex-col border-r border-gray-700">
          <div className="bg-gray-800 text-center py-3 font-bold text-lg tracking-wide text-yellow-400">
            EN PRÉPARATION
          </div>
          <div className="flex-1 overflow-y-auto p-4 grid grid-cols-2 gap-3 content-start">
            {inProgress.map((o) => (
              <OrbOrderItem key={o.order_id} order={o} />
            ))}
            {inProgress.length === 0 && (
              <div className="col-span-2 text-center text-gray-600 text-2xl pt-8">—</div>
            )}
          </div>
        </div>

        {/* Colonne droite — Prêt */}
        <div className="flex-1 flex flex-col">
          <div className="bg-green-800 text-center py-3 font-bold text-lg tracking-wide text-white">
            PRÊT ✓
          </div>
          <div className="flex-1 overflow-y-auto p-4 grid grid-cols-2 gap-3 content-start">
            {ready.map((o) => (
              <OrbOrderItem key={o.order_id} order={o} />
            ))}
            {ready.length === 0 && (
              <div className="col-span-2 text-center text-gray-600 text-2xl pt-8">—</div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
