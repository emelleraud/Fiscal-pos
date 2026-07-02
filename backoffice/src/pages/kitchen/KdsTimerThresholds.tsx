import { useEffect, useState } from 'react'
import { supabase } from '../../supabaseClient'
import { useAuth } from '../../context/AuthContext'
import { useSite } from '../../context/SiteContext'

interface Threshold {
  station_id: string
  warning_secs: number
  critical_secs: number
}

interface Station { id: string; name: string }

const inputStyle = { padding: '0.3rem 0.5rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem', width: 70 }

export default function KdsTimerThresholds() {
  const [thresholds, setThresholds] = useState<Threshold[]>([])
  const [stations, setStations] = useState<Station[]>([])
  const [edited, setEdited] = useState<Record<string, Threshold>>({})
  const [saving, setSaving] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [newStationId, setNewStationId] = useState('')
  const [newWarn, setNewWarn] = useState('120')
  const [newCrit, setNewCrit] = useState('300')
  const [adding, setAdding] = useState(false)

  const { role } = useAuth()
  const { activeSiteId } = useSite()
  const canWrite = role === 'pos_admin' || role === 'regional_director'

  const load = () => {
    if (!activeSiteId) { setLoading(false); return }
    setLoading(true)
    Promise.all([
      supabase.from('kds_timer_thresholds').select('*').eq('site_id', activeSiteId).order('station_id'),
      supabase.from('kds_station_configs').select('id,name').eq('site_id', activeSiteId).order('sort_order'),
    ]).then(([tRes, sRes]) => {
      if (tRes.error) setError(tRes.error.message)
      else setThresholds((tRes.data as Threshold[]) ?? [])
      if (sRes.error) setError(e => e ?? sRes.error!.message)
      else setStations((sRes.data as Station[]) ?? [])
      setLoading(false)
    })
  }

  useEffect(load, [activeSiteId])

  const getEdited = (t: Threshold): Threshold => edited[t.station_id] ?? t

  const handleChange = (stationId: string, field: 'warning_secs' | 'critical_secs', val: string) => {
    const num = parseInt(val, 10)
    if (isNaN(num)) return
    setEdited(prev => ({
      ...prev,
      [stationId]: { ...(edited[stationId] ?? thresholds.find(t => t.station_id === stationId)!), [field]: num },
    }))
  }

  const handleSave = async (stationId: string) => {
    const t = edited[stationId]
    if (!t || !activeSiteId) return
    setSaving(stationId); setError(null)
    const { error: e } = await supabase.from('kds_timer_thresholds')
      .upsert({ site_id: activeSiteId, ...t }, { onConflict: 'site_id,station_id' })
    if (e) setError(e.message)
    else { setEdited(prev => { const next = { ...prev }; delete next[stationId]; return next }); load() }
    setSaving(null)
  }

  const handleAdd = async () => {
    if (!activeSiteId || !newStationId.trim()) return
    setAdding(true); setError(null)
    const { error: e } = await supabase.from('kds_timer_thresholds').insert({
      site_id: activeSiteId,
      station_id: newStationId.trim(),
      warning_secs: parseInt(newWarn, 10) || 120,
      critical_secs: parseInt(newCrit, 10) || 300,
    })
    if (e) setError(e.message)
    else { setNewStationId(''); setNewWarn('120'); setNewCrit('300'); load() }
    setAdding(false)
  }

  if (!activeSiteId) return <p style={{ padding: '1.5rem', color: '#888' }}>Sélectionner un site</p>
  if (loading) return <p style={{ padding: '1.5rem', color: '#888' }}>Chargement…</p>

  const stationName = (id: string) => stations.find(s => s.id === id)?.name ?? id

  return (
    <div style={{ padding: '1.5rem' }}>
      <h2 style={{ marginTop: 0 }}>Seuils timer par station</h2>
      <p style={{ color: '#666', fontSize: '0.85rem', marginBottom: '1rem' }}>
        Vert &lt; warning, orange &lt; critical, rouge au-delà. Valeurs en secondes.
      </p>
      {error && <p style={{ color: '#e53e3e', marginBottom: '1rem' }}>{error}</p>}

      <table style={{ width: '100%', borderCollapse: 'collapse', marginBottom: '1.5rem' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['STATION', 'WARNING (s)', 'CRITICAL (s)', 'ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {thresholds.map(t => {
            const e = getEdited(t)
            const dirty = JSON.stringify(e) !== JSON.stringify(t)
            return (
              <tr key={t.station_id} style={{ borderBottom: '1px solid #f0f0f0' }}>
                <td style={{ padding: '0.6rem 0.8rem', fontWeight: 500 }}>{stationName(t.station_id)}</td>
                <td style={{ padding: '0.6rem 0.8rem' }}>
                  <input style={inputStyle} type="number" value={e.warning_secs} min="10"
                    onChange={ev => handleChange(t.station_id, 'warning_secs', ev.target.value)}
                    disabled={!canWrite} />
                </td>
                <td style={{ padding: '0.6rem 0.8rem' }}>
                  <input style={inputStyle} type="number" value={e.critical_secs} min="10"
                    onChange={ev => handleChange(t.station_id, 'critical_secs', ev.target.value)}
                    disabled={!canWrite} />
                </td>
                <td style={{ padding: '0.6rem 0.8rem' }}>
                  {canWrite && dirty && (
                    <button
                      onClick={() => handleSave(t.station_id)}
                      disabled={saving === t.station_id}
                      style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 4, padding: '0.3rem 0.7rem', cursor: 'pointer', fontSize: '0.8rem' }}
                    >
                      {saving === t.station_id ? '…' : 'Enregistrer'}
                    </button>
                  )}
                </td>
              </tr>
            )
          })}
          {thresholds.length === 0 && (
            <tr>
              <td colSpan={4} style={{ padding: '1.5rem', textAlign: 'center', color: '#aaa', fontSize: '0.85rem' }}>
                Aucun seuil configuré
              </td>
            </tr>
          )}
        </tbody>
      </table>

      {canWrite && stations.length > 0 && (
        <div style={{ background: '#f9f9fb', border: '1px solid #eee', borderRadius: 8, padding: '1rem' }}>
          <h4 style={{ marginTop: 0, marginBottom: '0.75rem', fontSize: '0.9rem' }}>+ Ajouter un seuil</h4>
          <div style={{ display: 'flex', gap: 8, alignItems: 'flex-end' }}>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Station</label>
              <select style={{ padding: '0.35rem 0.6rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem' }}
                value={newStationId} onChange={e => setNewStationId(e.target.value)}>
                <option value="">— Choisir —</option>
                {stations.filter(s => !thresholds.some(t => t.station_id === s.id)).map(s => (
                  <option key={s.id} value={s.id}>{s.name}</option>
                ))}
              </select>
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Warning (s)</label>
              <input style={inputStyle} type="number" value={newWarn} onChange={e => setNewWarn(e.target.value)} min="10" />
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Critical (s)</label>
              <input style={inputStyle} type="number" value={newCrit} onChange={e => setNewCrit(e.target.value)} min="10" />
            </div>
            <button
              onClick={handleAdd}
              disabled={adding || !newStationId}
              style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.45rem 0.9rem', cursor: 'pointer', fontWeight: 600 }}
            >
              {adding ? '…' : 'Ajouter'}
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
