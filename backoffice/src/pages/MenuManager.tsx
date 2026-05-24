import { useEffect, useState, type CSSProperties } from 'react'
import { supabase } from '../supabaseClient'
import { useSite } from '../context/SiteContext'

interface MenuItem {
  id: string
  name: string
  price_ttc_cents: number
  tva_rate: 'reduit5_5' | 'intermediaire10' | 'normal20'
  category: string
  available: boolean
}

interface SiteConfig {
  id: string
  site_id: string
  version: number
  menu: { items: MenuItem[] }
  updated_at_ms: number
}

const TVA_LABELS: Record<string, string> = {
  reduit5_5:      '5,5 %',
  intermediaire10: '10 %',
  normal20:        '20 %',
}

const EUR = (cents: number) =>
  (cents / 100).toLocaleString('fr-FR', { style: 'currency', currency: 'EUR' })

const EMPTY_ITEM: Omit<MenuItem, 'id'> = {
  name: '', price_ttc_cents: 0, tva_rate: 'intermediaire10', category: '', available: true,
}

const inputStyle: CSSProperties = {
  width: '100%', padding: '0.4rem 0.6rem',
  border: '1px solid #ddd', borderRadius: 5, fontSize: '0.85rem', boxSizing: 'border-box',
}

const btnSecondary: CSSProperties = {
  padding: '0.3rem 0.7rem', borderRadius: 5, border: '1px solid #ddd',
  background: '#fff', fontSize: '0.8rem', cursor: 'pointer', color: '#333',
}

export default function MenuManager() {
  const { activeSiteId } = useSite()

  const [config, setConfig]     = useState<SiteConfig | null>(null)
  const [loading, setLoading]   = useState(true)
  const [error, setError]       = useState<string | null>(null)
  const [saving, setSaving]     = useState(false)

  const [editingId, setEditingId] = useState<string | null>(null)
  const [draft, setDraft]         = useState<Partial<MenuItem>>({})
  const [showAdd, setShowAdd]     = useState(false)
  const [newItem, setNewItem]     = useState<Omit<MenuItem, 'id'>>(EMPTY_ITEM)

  useEffect(() => {
    if (!activeSiteId) return
    setLoading(true)
    setError(null)
    supabase
      .from('site_configs')
      .select('id,site_id,version,menu,updated_at_ms')
      .eq('site_id', activeSiteId)
      .single()
      .then(({ data, error: err }) => {
        if (err && err.code !== 'PGRST116') setError(err.message)
        else setConfig(data)
        setLoading(false)
      })
  }, [activeSiteId])

  async function persistItems(items: MenuItem[]) {
    if (!config || !activeSiteId) return
    setSaving(true)
    const newVersion = config.version + 1
    const now = Date.now()
    const { error: err } = await supabase
      .from('site_configs')
      .upsert({
        id: config.id,
        site_id: activeSiteId,
        version: newVersion,
        menu: { items },
        updated_at_ms: now,
      })
    if (err) {
      setError(err.message)
    } else {
      setConfig({ ...config, version: newVersion, menu: { items }, updated_at_ms: now })
    }
    setSaving(false)
  }

  function handleAdd() {
    if (!newItem.name.trim()) return
    const item: MenuItem = { ...newItem, id: `item-${Date.now()}` }
    persistItems([...(config?.menu.items ?? []), item])
    setNewItem(EMPTY_ITEM)
    setShowAdd(false)
  }

  function handleDelete(id: string) {
    persistItems((config?.menu.items ?? []).filter(i => i.id !== id))
  }

  function handleSaveEdit() {
    persistItems((config?.menu.items ?? []).map(i => i.id === editingId ? { ...i, ...draft } : i))
    setEditingId(null)
    setDraft({})
  }

  function handleToggle(id: string) {
    persistItems((config?.menu.items ?? []).map(i => i.id === id ? { ...i, available: !i.available } : i))
  }

  if (!activeSiteId) return <p style={{ color: '#888' }}>Sélectionnez un site.</p>
  if (loading)       return <p style={{ color: '#888' }}>Chargement…</p>
  if (error)         return <p style={{ color: '#e53e3e' }}>Erreur : {error}</p>
  if (!config)       return <p style={{ color: '#888' }}>Aucune configuration pour ce site.</p>

  const items = config.menu.items ?? []

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: '1rem', marginBottom: '1.5rem' }}>
        <h1 style={{ margin: 0, fontSize: '1.25rem' }}>Gestion menu</h1>
        <span style={{ color: '#888', fontSize: '0.85rem' }}>v{config.version} · {items.length} articles</span>
        <button
          onClick={() => setShowAdd(true)}
          style={{ marginLeft: 'auto', padding: '0.45rem 1rem', borderRadius: 6, border: 'none', background: '#4f8ef7', color: '#fff', fontWeight: 600, fontSize: '0.85rem', cursor: 'pointer' }}
        >
          + Ajouter
        </button>
      </div>

      {showAdd && (
        <div style={{ background: '#fff', borderRadius: 8, padding: '1.25rem', marginBottom: '1rem', boxShadow: '0 1px 3px rgba(0,0,0,0.08)' }}>
          <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr 1fr 1fr', gap: '0.75rem', marginBottom: '0.75rem' }}>
            <input placeholder="Nom" value={newItem.name}
              onChange={e => setNewItem(p => ({ ...p, name: e.target.value }))} style={inputStyle} />
            <input placeholder="Catégorie" value={newItem.category}
              onChange={e => setNewItem(p => ({ ...p, category: e.target.value }))} style={inputStyle} />
            <input type="number" placeholder="Prix (centimes)" value={newItem.price_ttc_cents || ''}
              onChange={e => setNewItem(p => ({ ...p, price_ttc_cents: parseInt(e.target.value) || 0 }))} style={inputStyle} />
            <select value={newItem.tva_rate}
              onChange={e => setNewItem(p => ({ ...p, tva_rate: e.target.value as MenuItem['tva_rate'] }))} style={inputStyle}>
              {Object.entries(TVA_LABELS).map(([k, v]) => <option key={k} value={k}>TVA {v}</option>)}
            </select>
          </div>
          <div style={{ display: 'flex', gap: '0.5rem', justifyContent: 'flex-end' }}>
            <button onClick={() => { setShowAdd(false); setNewItem(EMPTY_ITEM) }} style={btnSecondary}>Annuler</button>
            <button onClick={handleAdd} disabled={saving}
              style={{ padding: '0.4rem 0.9rem', borderRadius: 6, border: 'none', background: '#4f8ef7', color: '#fff', fontWeight: 600, fontSize: '0.85rem', cursor: 'pointer', opacity: saving ? 0.7 : 1 }}>
              {saving ? 'Enregistrement…' : 'Ajouter'}
            </button>
          </div>
        </div>
      )}

      <div style={{ background: '#fff', borderRadius: 8, boxShadow: '0 1px 3px rgba(0,0,0,0.08)', overflow: 'hidden' }}>
        <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.85rem' }}>
          <thead>
            <tr style={{ background: '#f1f3f5', textAlign: 'left' }}>
              {['Nom', 'Catégorie', 'Prix TTC', 'TVA', 'Dispo', 'Actions'].map(h => (
                <th key={h} style={{ padding: '0.65rem 1rem', fontWeight: 600, fontSize: '0.75rem', color: '#555', textTransform: 'uppercase', letterSpacing: '0.05em' }}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {items.length === 0 && (
              <tr>
                <td colSpan={6} style={{ padding: '2rem', textAlign: 'center', color: '#888' }}>
                  Aucun article. Cliquez sur « + Ajouter » pour commencer.
                </td>
              </tr>
            )}
            {items.map(item => (
              <tr key={item.id} style={{ borderTop: '1px solid #f1f3f5' }}>
                {editingId === item.id ? (
                  <>
                    <td style={{ padding: '0.5rem 1rem' }}>
                      <input value={draft.name ?? item.name}
                        onChange={e => setDraft(p => ({ ...p, name: e.target.value }))} style={inputStyle} />
                    </td>
                    <td style={{ padding: '0.5rem 1rem' }}>
                      <input value={draft.category ?? item.category}
                        onChange={e => setDraft(p => ({ ...p, category: e.target.value }))} style={inputStyle} />
                    </td>
                    <td style={{ padding: '0.5rem 1rem' }}>
                      <input type="number" value={draft.price_ttc_cents ?? item.price_ttc_cents}
                        onChange={e => setDraft(p => ({ ...p, price_ttc_cents: parseInt(e.target.value) || 0 }))} style={inputStyle} />
                    </td>
                    <td style={{ padding: '0.5rem 1rem' }}>
                      <select value={draft.tva_rate ?? item.tva_rate}
                        onChange={e => setDraft(p => ({ ...p, tva_rate: e.target.value as MenuItem['tva_rate'] }))} style={inputStyle}>
                        {Object.entries(TVA_LABELS).map(([k, v]) => <option key={k} value={k}>TVA {v}</option>)}
                      </select>
                    </td>
                    <td style={{ padding: '0.5rem 1rem', color: '#888' }}>—</td>
                    <td style={{ padding: '0.5rem 1rem' }}>
                      <div style={{ display: 'flex', gap: '0.4rem' }}>
                        <button onClick={handleSaveEdit} disabled={saving}
                          style={{ padding: '0.3rem 0.7rem', borderRadius: 5, border: 'none', background: '#4f8ef7', color: '#fff', fontSize: '0.8rem', cursor: 'pointer' }}>
                          OK
                        </button>
                        <button onClick={() => { setEditingId(null); setDraft({}) }} style={btnSecondary}>✕</button>
                      </div>
                    </td>
                  </>
                ) : (
                  <>
                    <td style={{ padding: '0.6rem 1rem', fontWeight: 500 }}>{item.name}</td>
                    <td style={{ padding: '0.6rem 1rem', color: '#888' }}>{item.category}</td>
                    <td style={{ padding: '0.6rem 1rem', fontFamily: 'monospace' }}>{EUR(item.price_ttc_cents)}</td>
                    <td style={{ padding: '0.6rem 1rem', color: '#888' }}>{TVA_LABELS[item.tva_rate] ?? item.tva_rate}</td>
                    <td style={{ padding: '0.6rem 1rem' }}>
                      <button onClick={() => handleToggle(item.id)}
                        style={{ padding: '2px 8px', borderRadius: 4, border: 'none', fontSize: '0.75rem', fontWeight: 600, cursor: 'pointer',
                          background: item.available ? '#d4edda' : '#f8d7da',
                          color: item.available ? '#2d6a4f' : '#c0392b' }}>
                        {item.available ? 'Actif' : 'Inactif'}
                      </button>
                    </td>
                    <td style={{ padding: '0.6rem 1rem' }}>
                      <div style={{ display: 'flex', gap: '0.4rem' }}>
                        <button onClick={() => { setEditingId(item.id); setDraft({}) }} style={btnSecondary}>Éditer</button>
                        <button onClick={() => handleDelete(item.id)}
                          style={{ padding: '0.3rem 0.7rem', borderRadius: 5, border: '1px solid #ffc9c9', background: '#fff', fontSize: '0.8rem', cursor: 'pointer', color: '#c0392b' }}>
                          Suppr.
                        </button>
                      </div>
                    </td>
                  </>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
