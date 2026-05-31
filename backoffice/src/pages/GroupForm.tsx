import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { supabase } from '../supabaseClient'

interface Site { id: string; site_code: string; name: string }

const inputStyle = { padding: '0.5rem 0.75rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.9rem', width: '100%', boxSizing: 'border-box' as const }

function FieldLabel({ txt }: { txt: string }) {
  return <label style={{ display: 'block', fontWeight: 600, marginBottom: 4, fontSize: '0.85rem' }}>{txt}</label>
}

export default function GroupForm() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const isEdit = id !== undefined && id !== 'new'

  const [name, setName] = useState('')
  const [groupType, setGroupType] = useState<'static' | 'dynamic' | 'mixed'>('static')
  const [criteria, setCriteria] = useState('{}')
  const [allSites, setAllSites] = useState<Site[]>([])
  const [selectedSites, setSelectedSites] = useState<string[]>([])
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    supabase.from('sites').select('id, site_code, name').order('site_code')
      .then(({ data, error: e }) => {
        if (e) setError(e.message)
        else setAllSites(data ?? [])
      })

    if (isEdit) {
      supabase.from('restaurant_groups').select('*').eq('id', id!).single()
        .then(({ data, error: e }) => {
          if (e) { setError(e.message); return }
          if (!data) return
          setName(data.name)
          setGroupType(data.group_type)
          setCriteria(JSON.stringify(data.criteria ?? {}, null, 2))
        })
      supabase.from('restaurant_group_members').select('site_id').eq('group_id', id!)
        .then(({ data }) => setSelectedSites((data ?? []).map((r: { site_id: string }) => r.site_id)))
    }
  }, [id, isEdit])

  const toggleSite = (siteId: string) =>
    setSelectedSites(prev => prev.includes(siteId) ? prev.filter(s => s !== siteId) : [...prev, siteId])

  const handleSave = async () => {
    setSaving(true); setError(null)
    try {
      let parsedCriteria: object | null = null
      if (groupType !== 'static') {
        try { parsedCriteria = JSON.parse(criteria) }
        catch { throw new Error('Critères JSON invalides') }
      }

      let groupId = id
      if (!isEdit) {
        const { data, error: e } = await supabase.from('restaurant_groups')
          .insert({ name, group_type: groupType, criteria: parsedCriteria })
          .select('id').single()
        if (e) throw e
        groupId = data.id
      } else {
        const { error: e } = await supabase.from('restaurant_groups')
          .update({ name, group_type: groupType, criteria: parsedCriteria })
          .eq('id', id!)
        if (e) throw e
      }

      if (groupType !== 'dynamic') {
        await supabase.from('restaurant_group_members').delete().eq('group_id', groupId!)
        if (selectedSites.length > 0) {
          const { error: e } = await supabase.from('restaurant_group_members')
            .insert(selectedSites.map(s => ({ group_id: groupId!, site_id: s })))
          if (e) throw e
        }
      }
      navigate('/groups')
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Erreur de sauvegarde')
    } finally { setSaving(false) }
  }

  return (
    <div style={{ padding: '1.5rem', maxWidth: 600 }}>
      <h2 style={{ marginTop: 0 }}>{isEdit ? 'Éditer' : 'Nouveau'} groupe</h2>
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}

      <div style={{ marginBottom: '1rem' }}>
        <FieldLabel txt="Nom" />
        <input style={inputStyle} value={name} onChange={e => setName(e.target.value)} />
      </div>

      <div style={{ marginBottom: '1rem' }}>
        <FieldLabel txt="Type" />
        <select style={inputStyle} value={groupType} onChange={e => setGroupType(e.target.value as 'static' | 'dynamic' | 'mixed')}>
          <option value="static">Statique (liste manuelle)</option>
          <option value="dynamic">Dynamique (critères)</option>
          <option value="mixed">Mixte</option>
        </select>
      </div>

      {groupType !== 'static' && (
        <div style={{ marginBottom: '1rem' }}>
          <FieldLabel txt='Critères JSON (ex: {"ville":"Paris"})' />
          <textarea style={{ ...inputStyle, height: 100, fontFamily: 'monospace', resize: 'vertical' }}
            value={criteria} onChange={e => setCriteria(e.target.value)} />
        </div>
      )}

      {groupType !== 'dynamic' && (
        <div style={{ marginBottom: '1rem' }}>
          <FieldLabel txt="Sites membres" />
          <div style={{ border: '1px solid #ddd', borderRadius: 6, maxHeight: 200, overflowY: 'auto', padding: '0.5rem' }}>
            {allSites.map(s => (
              <label key={s.id} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '0.25rem 0', cursor: 'pointer' }}>
                <input type="checkbox" checked={selectedSites.includes(s.id)} onChange={() => toggleSite(s.id)} />
                {s.name} <span style={{ color: '#888', fontSize: '0.8rem' }}>({s.site_code})</span>
              </label>
            ))}
          </div>
        </div>
      )}

      <div style={{ display: 'flex', gap: 8 }}>
        <button onClick={handleSave} disabled={saving || !name.trim()}
          style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.6rem 1.2rem', cursor: 'pointer', fontWeight: 600 }}>
          {saving ? 'Sauvegarde…' : 'Enregistrer'}
        </button>
        <button onClick={() => navigate('/groups')}
          style={{ background: '#f5f6fa', border: '1px solid #ddd', borderRadius: 6, padding: '0.6rem 1rem', cursor: 'pointer' }}>
          Annuler
        </button>
      </div>
    </div>
  )
}
