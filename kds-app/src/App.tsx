import { useEffect, useState } from 'react'
import { getStations } from './api'
import ConfigPage from './pages/ConfigPage'
import ExpoStation from './pages/ExpoStation'
import OrderReadyBoard from './pages/OrderReadyBoard'
import PreparationStation from './pages/PreparationStation'

function getStationId(): string {
  const segments = window.location.pathname.split('/').filter(Boolean)
  return segments[segments.length - 1] ?? ''
}

export default function App() {
  const stationId = getStationId()
  const params = new URLSearchParams(window.location.search)
  const orbType = params.get('orb') as 'client' | 'livreur' | null

  const [stationRole, setStationRole] = useState<string | null>(null)
  const [roleLoaded, setRoleLoaded] = useState(false)

  useEffect(() => {
    if (!stationId || stationId === 'config' || stationId === 'ready_board') {
      setRoleLoaded(true)
      return
    }
    getStations()
      .then((list) => {
        setStationRole(list.find((s) => s.id === stationId)?.role ?? null)
      })
      .catch(() => {})
      .finally(() => setRoleLoaded(true))
  }, [stationId])

  if (stationId === 'config') return <ConfigPage />
  if (stationId === 'ready_board') return <OrderReadyBoard orbType={orbType} />

  if (!stationId) {
    return (
      <div className="min-h-screen bg-gray-900 text-white flex items-center justify-center">
        <div className="text-center">
          <p className="text-2xl mb-4 text-gray-400">KDS — Aucune station configurée</p>
          <p className="text-sm text-gray-600">
            Accéder à{' '}
            <code className="bg-gray-800 px-2 py-1 rounded">/kds/[station_id]</code>
          </p>
        </div>
      </div>
    )
  }

  if (!roleLoaded) return null

  if (stationRole === 'expo') return <ExpoStation stationId={stationId} />
  return <PreparationStation stationId={stationId} />
}
