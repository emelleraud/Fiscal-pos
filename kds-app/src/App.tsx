import PreparationStation from './pages/PreparationStation'
import OrderReadyBoard from './pages/OrderReadyBoard'
import ConfigPage from './pages/ConfigPage'

function getStationId(): string {
  // Extrait le dernier segment de /kds/grill-01 → 'grill-01'
  // ou /kds/ready_board → 'ready_board'
  const segments = window.location.pathname.split('/').filter(Boolean)
  return segments[segments.length - 1] ?? ''
}

export default function App() {
  const stationId = getStationId()
  const params = new URLSearchParams(window.location.search)
  const orbType = params.get('orb') as 'client' | 'livreur' | null

  if (stationId === 'config') return <ConfigPage />
  if (stationId === 'ready_board') return <OrderReadyBoard orbType={orbType} />
  if (stationId) return <PreparationStation stationId={stationId} />

  return (
    <div className="min-h-screen bg-gray-900 text-white flex items-center justify-center">
      <div className="text-center">
        <p className="text-2xl mb-4 text-gray-400">KDS — Aucune station configurée</p>
        <p className="text-sm text-gray-600">
          Accéder à <code className="bg-gray-800 px-2 py-1 rounded">/kds/[station_id]</code>
        </p>
      </div>
    </div>
  )
}
