import { useEffect, useState } from 'react'
import { supabase } from '../../supabaseClient'
import { useAuth } from '../../context/AuthContext'
import { useSite } from '../../context/SiteContext'

interface RoutingProfile { id: string; name: string }
interface RoutingRule {
  id: string
  profile_id: string
  rule_type: string
  match_value: string
  station_ids: string
  priority: number
}

const inputStyle = { padding: '0.35rem 0.6rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem' }
const selectStyle = { ...inputStyle }

export default function KdsRoutingRules() {
  const [profiles, setProfiles] = useState<RoutingProfile[]>([])
  const [rules, setRules] = useState<RoutingRule[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const [newProfileId, setNewProfileId] = useState('')
  const [newRuleType, setNewRuleType] = useState('category')
  const [newMatchValue, setNewMatchValue] = useState('')
  const [newStationIds, setNewStationIds] = useState('')
  const [newPriority, setNewPriority] = useState('0')
  const [adding, setAdding] = useState(false)

  const { role } = useAuth()
  const { activeSiteId } = useSite()
  const canWrite = role === 'pos_admin' || role === 'regional_director'

  const load = () => {
    if (!activeSiteId) { setLoading(false); return }
    setLoading(true)
    Promise.all([
      supabase.from('kds_routing_profiles').select('id,name').eq('site_id', activeSiteId).order('id'),
      supabase.from('kds_routing_configs').select('*').eq('site_id', activeSiteId).order('priority', { ascending: false }),
    ]).then(([pRes, rRes]) => {
      if (pRes.error) setError(pRes.error.message)
      else setProfiles((pRes.data as RoutingProfile[]) ?? [])
      if (rRes.error) setError(e => e ?? rRes.error!.message)
      else setRules((rRes.data as RoutingRule[]) ?? [])
      setLoading(false)
      if (!newProfileId && (pRes.data as RoutingProfile[])?.[0]) {
        setNewProfileId((pRes.data as RoutingProfile[])[0].id)
      }
    })
  }

  useEffect(load, [activeSiteId])

  const handleAdd = async () => {
    if (!activeSiteId || !newMatchValue.trim() || !newStationIds.trim()) return
    setAdding(true); setError(null)
    // station_ids stocké en JSON array
    const stationArr = newStationIds.split(',').map(s => s.trim()).filter(Boolean)
    const id = `${newProfileId}-${newRuleType}-${newMatchValue.trim()}-${Date.now()}`
    const { error: e } = await supabase.from('kds_routing_configs').insert({
      id,
      site_id: activeSiteId,
      profile_id: newProfileId,
      rule_type: newRuleType,
      match_value: newMatchValue.trim(),
      station_ids: JSON.stringify(stationArr),
      priority: parseInt(newPriority, 10) || 0,
    })
    if (e) setError(e.message)
    else { setNewMatchValue(''); setNewStationIds(''); setNewPriority('0'); load() }
    setAdding(false)
  }

  const handleDelete = async (id: string) => {
    if (!window.confirm('Supprimer cette règle ?')) return
    const { error: e } = await supabase.from('kds_routing_configs')
      .delete().eq('site_id', activeSiteId!).eq('id', id)
    if (e) setError(e.message)
    else load()
  }

  if (!activeSiteId) return <p style={{ padding: '1.5rem', color: '#888' }}>Sélectionner un site</p>
  if (loading) return <p style={{ padding: '1.5rem', color: '#888' }}>Chargement…</p>

  const RULE_TYPE_LABEL: Record<string, string> = { category: 'Catégorie', product: 'Produit', tag: 'Tag' }

  return (
    <div style={{ padding: '1.5rem' }}>
      <h2 style={{ marginTop: 0 }}>Règles de routage cuisine</h2>
      {error && <p style={{ color: '#e53e3e', marginBottom: '1rem' }}>{error}</p>}

      {profiles.map(profile => {
        const profileRules = rules.filter(r => r.profile_id === profile.id)
        return (
          <div key={profile.id} style={{ marginBottom: '2rem' }}>
            <h3 style={{ color: '#4f8ef7', marginBottom: '0.5rem', textTransform: 'uppercase', fontSize: '0.9rem' }}>
              Profil : {profile.name}
            </h3>
            <table style={{ width: '100%', borderCollapse: 'collapse', marginBottom: '0.5rem' }}>
              <thead>
                <tr style={{ background: '#f5f6fa' }}>
                  {['TYPE', 'VALEUR', 'STATIONS CIBLES', 'PRIORITÉ', 'ACTIONS'].map(h => (
                    <th key={h} style={{ textAlign: 'left', padding: '0.5rem 0.7rem', fontSize: '0.75rem', color: '#666' }}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {profileRules.map(r => (
                  <tr key={r.id} style={{ borderBottom: '1px solid #f0f0f0' }}>
                    <td style={{ padding: '0.6rem 0.7rem', fontSize: '0.85rem' }}>{RULE_TYPE_LABEL[r.rule_type] ?? r.rule_type}</td>
                    <td style={{ padding: '0.6rem 0.7rem', fontFamily: 'monospace', fontSize: '0.85rem' }}>{r.match_value}</td>
                    <td style={{ padding: '0.6rem 0.7rem', fontSize: '0.8rem', color: '#555' }}>
                      {(JSON.parse(r.station_ids) as string[]).join(', ')}
                    </td>
                    <td style={{ padding: '0.6rem 0.7rem', fontSize: '0.85rem' }}>{r.priority}</td>
                    <td style={{ padding: '0.6rem 0.7rem' }}>
                      {canWrite && (
                        <button
                          onClick={() => handleDelete(r.id)}
                          style={{ background: 'none', border: 'none', color: '#e53e3e', cursor: 'pointer', fontSize: '0.85rem', padding: 0 }}
                        >
                          Supprimer
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
                {profileRules.length === 0 && (
                  <tr>
                    <td colSpan={5} style={{ padding: '1rem', color: '#aaa', fontSize: '0.85rem' }}>
                      Aucune règle pour ce profil
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )
      })}

      {canWrite && (
        <div style={{ background: '#f9f9fb', border: '1px solid #eee', borderRadius: 8, padding: '1rem' }}>
          <h4 style={{ marginTop: 0, marginBottom: '0.75rem', fontSize: '0.9rem' }}>+ Ajouter une règle</h4>
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', alignItems: 'flex-end' }}>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Profil</label>
              <select style={selectStyle} value={newProfileId} onChange={e => setNewProfileId(e.target.value)}>
                {profiles.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
              </select>
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Type</label>
              <select style={selectStyle} value={newRuleType} onChange={e => setNewRuleType(e.target.value)}>
                <option value="category">Catégorie</option>
                <option value="product">Produit (SKU)</option>
                <option value="tag">Tag</option>
              </select>
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Valeur</label>
              <input style={{ ...inputStyle, width: 140 }} value={newMatchValue}
                onChange={e => setNewMatchValue(e.target.value)} placeholder="Burgers" />
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Stations (IDs, virgule)</label>
              <input style={{ ...inputStyle, width: 180 }} value={newStationIds}
                onChange={e => setNewStationIds(e.target.value)} placeholder="grill-01, grill-02" />
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Priorité</label>
              <input style={{ ...inputStyle, width: 60 }} type="number" value={newPriority}
                onChange={e => setNewPriority(e.target.value)} min="0" />
            </div>
            <button
              onClick={handleAdd}
              disabled={adding || !newMatchValue.trim() || !newStationIds.trim()}
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
