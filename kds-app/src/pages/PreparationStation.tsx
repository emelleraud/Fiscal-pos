import { useEffect, useState } from 'react'
import { getStations } from '../api'
import ConnectionBanner from '../components/ConnectionBanner'
import OrderCard from '../components/OrderCard'
import { useKdsFeed } from '../hooks/useKdsFeed'
import type { Station, StationRole } from '../types'

interface Props {
  stationId: string
}

export default function PreparationStation({ stationId }: Props) {
  const { orders, connected } = useKdsFeed(stationId)
  const [station, setStation] = useState<Station | null>(null)
  const [stationError, setStationError] = useState<string | null>(null)
  const isFullscreen = new URLSearchParams(window.location.search).get('fullscreen') === 'true'

  useEffect(() => {
    getStations()
      .then((list) => {
        const found = list.find((s) => s.id === stationId) ?? null
        setStation(found)
        if (!found) setStationError(`Station "${stationId}" introuvable`)
      })
      .catch((e: unknown) => {
        setStationError(e instanceof Error ? e.message : 'Erreur API stations')
      })
  }, [stationId])

  const role: StationRole = station?.role ?? 'preparation'
  const stationName = station?.name ?? stationId

  const visibleOrders = orders.filter((o) => o.status !== 'served')

  return (
    <div className={`min-h-screen bg-gray-900 text-white flex flex-col ${isFullscreen ? '' : 'p-4'}`}>
      <ConnectionBanner connected={connected} />

      {/* En-tête station */}
      <div className="flex items-center gap-3 mb-4 px-2 pt-2">
        <h1 className="text-2xl font-bold tracking-wide">{stationName.toUpperCase()}</h1>
        <span className="text-xs bg-gray-700 rounded px-2 py-1 uppercase">{role}</span>
        <span className="ml-auto text-sm text-gray-400">
          {visibleOrders.length} commande{visibleOrders.length !== 1 ? 's' : ''}
        </span>
      </div>

      {stationError && (
        <div className="mx-2 mb-4 p-3 bg-yellow-900 border border-yellow-600 rounded text-yellow-200 text-sm">
          ⚠ {stationError}
        </div>
      )}

      {/* Grille de cartes — scroll horizontal */}
      <div className="flex gap-3 overflow-x-auto pb-4 px-2 flex-1 items-start">
        {visibleOrders.length === 0 ? (
          <div className="flex-1 flex items-center justify-center text-gray-500 text-xl min-h-64">
            Aucune commande en attente
          </div>
        ) : (
          visibleOrders.map((order) => (
            <OrderCard
              key={order.order_id}
              order={order}
              stationId={stationId}
              role={role}
            />
          ))
        )}
      </div>
    </div>
  )
}
