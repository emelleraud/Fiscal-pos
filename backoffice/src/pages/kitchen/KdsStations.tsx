import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { supabase } from '../../supabaseClient'
import { useAuth } from '../../context/AuthContext'
import { useSite } from '../../context/SiteContext'

interface KdsStation {
  id: string
  name: string
  role: string
  output_type: string
  enabled: number
  sort_order: number
}

export default function KdsStations() {
  const [stations, setStations] = useState<KdsStation[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()
  const { role } = useAuth()
  const { activeSiteId } = useSite()
  const canWrite = role === 'pos_admin' || role === 'regional_director'

  const load = () => {
    if (!activeSiteId) { setLoading(false); return }
    setLoading(true)
    supabase
      .from('kds_station_configs')
      .select('id,name,role,output_type,enabled,sort_order')
      .eq('site_id', activeSiteId)
      .order('sort_order')
      .then(({ data, error: e }) => {
        if (e) setError(e.message)
        else setStations((data as KdsStation[]) ?? [])
        setLoading(false)
      })
  }

  useEffect(load, [activeSiteId])

  const handleDelete = async (id: string, name: string) => {
    if (!window.confirm(`Supprimer la station "${name}" ?`)) return
    const { error: e } = await supabase
      .from('kds_station_configs')
      .delete()
      .eq('site_id', activeSiteId!)
      .eq('id', id)
    if (e) { setError(e.message); return }
    load()
  }

  if (!activeSiteId) return <p style={{ padding: '1.5rem', color: '#888' }}>Sélectionner un site</p>
  if (loading) return <p style={{ padding: '1.5rem', color: '#888' }}>Chargement…</p>

  const ROLE_LABEL: Record<string, string> = {
    preparation: 'Préparation',
    holding: 'Rassemblement',
    assembly: 'Assemblage Expo',
    ready_board: 'ORB',
  }

  return (
    <div style={{ padding: '1.5rem' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0 }}>
          Stations cuisine{' '}
          <span style={{ color: '#888', fontWeight: 400, fontSize: '0.9rem' }}>{stations.length} station(s)</span>
        </h2>
        {canWrite && (
          <button
            onClick={() => navigate('/kitchen/stations/new')}
            style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}
          >
            + Nouvelle station
          </button>
        )}
      </div>

      {error && <p style={{ color: '#e53e3e', marginBottom: '1rem' }}>{error}</p>}

      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['ID', 'NOM', 'RÔLE', 'OUTPUT', 'ÉTAT', 'ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {stations.map(s => (
            <tr key={s.id} style={{ borderBottom: '1px solid #f0f0f0' }}>
              <td style={{ padding: '0.7rem 0.8rem', fontFamily: 'monospace', fontSize: '0.85rem', color: '#666' }}>{s.id}</td>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500 }}>{s.name}</td>
              <td style={{ padding: '0.7rem 0.8rem', fontSize: '0.85rem' }}>{ROLE_LABEL[s.role] ?? s.role}</td>
              <td style={{ padding: '0.7rem 0.8rem', fontSize: '0.85rem' }}>{s.output_type}</td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                <span style={{
                  display: 'inline-block', padding: '0.2rem 0.5rem', borderRadius: 4,
                  fontSize: '0.75rem', fontWeight: 600,
                  background: s.enabled ? '#d4edda' : '#f8d7da',
                  color: s.enabled ? '#155724' : '#721c24',
                }}>
                  {s.enabled ? 'Actif' : 'Inactif'}
                </span>
              </td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                {canWrite && (
                  <>
                    <Link to={`/kitchen/stations/${s.id}`} style={{ color: '#4f8ef7', textDecoration: 'none', marginRight: 12 }}>
                      Éditer
                    </Link>
                    <button
                      onClick={() => handleDelete(s.id, s.name)}
                      style={{ background: 'none', border: 'none', color: '#e53e3e', cursor: 'pointer', fontSize: '0.85rem', padding: 0 }}
                    >
                      Supprimer
                    </button>
                  </>
                )}
              </td>
            </tr>
          ))}
          {stations.length === 0 && (
            <tr>
              <td colSpan={6} style={{ padding: '2rem', textAlign: 'center', color: '#888' }}>
                Aucune station configurée. Créez la première station.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  )
}
