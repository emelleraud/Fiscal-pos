import { useEffect, useState, type CSSProperties } from 'react'
import { supabase } from '../supabaseClient'

interface Category {
  id: string
  parent_id: string | null
  name: string
  display_order: number
  kds_station: string | null
  printer_station: string | null
}

const EMPTY: Omit<Category, 'id'> = {
  parent_id: null, name: '', display_order: 0, kds_station: null, printer_station: null,
}

const inputStyle: CSSProperties = {
  padding: '0.4rem 0.6rem', border: '1px solid #ddd', borderRadius: 5,
  fontSize: '0.85rem', width: '100%', boxSizing: 'border-box',
}

const btnSecondary: CSSProperties = {
  padding: '0.3rem 0.7rem', borderRadius: 5, border: '1px solid #ddd',
  background: '#fff', fontSize: '0.8rem', cursor: 'pointer', color: '#333',
}

export default function CategoryManager() {
  const [cats, setCats]       = useState<Category[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError]     = useState<string | null>(null)
  const [saving, setSaving]   = useState(false)
  const [showAdd, setShowAdd] = useState(false)
  const [draft, setDraft]     = useState<Omit<Category, 'id'>>(EMPTY)
  const [editId, setEditId]   = useState<string | null>(null)
  const [editDraft, setEdit]  = useState<Partial<Category>>({})

  async function load() {
    setLoading(true)
    const { data, error } = await supabase
      .from('menu_categories')
      .select('id,parent_id,name,display_order,kds_station,printer_station')
      .order('display_order')
    if (error) setError(error.message)
    else setCats(data ?? [])
    setLoading(false)
  }

  useEffect(() => { load() }, [])

  async function handleAdd() {
    if (!draft.name.trim()) return
    setSaving(true)
    const { error } = await supabase.from('menu_categories').insert({
      ...draft,
      kds_station:     draft.kds_station || null,
      printer_station: draft.printer_station || null,
    })
    if (error) setError(error.message)
    else { setShowAdd(false); setDraft(EMPTY); await load() }
    setSaving(false)
  }

  async function handleSaveEdit() {
    if (!editId) return
    setSaving(true)
    const { error } = await supabase.from('menu_categories').update({
      ...editDraft,
      kds_station:     editDraft.kds_station || null,
      printer_station: editDraft.printer_station || null,
    }).eq('id', editId)
    if (error) setError(error.message)
    else { setEditId(null); await load() }
    setSaving(false)
  }

  async function handleDelete(id: string) {
    if (!confirm('Supprimer cette catégorie ?')) return
    const { error } = await supabase.from('menu_categories').delete().eq('id', id)
    if (error) setError(error.message)
    else await load()
  }

  function buildTree(parentId: string | null = null, depth = 0): { cat: Category; depth: number }[] {
    return cats
      .filter(c => c.parent_id === parentId)
      .sort((a, b) => a.display_order - b.display_order)
      .flatMap(cat => [{ cat, depth }, ...buildTree(cat.id, depth + 1)])
  }

  function buildOptions(parentId: string | null = null, depth = 0): React.ReactNode[] {
    return cats
      .filter(c => c.parent_id === parentId)
      .sort((a, b) => a.display_order - b.display_order)
      .flatMap(cat => [
        <option key={cat.id} value={cat.id}>{'  '.repeat(depth)}{depth > 0 ? '↳ ' : ''}{cat.name}</option>,
        ...buildOptions(cat.id, depth + 1),
      ])
  }

  const orderedRows = buildTree()

  if (loading) return <p style={{ color: '#888' }}>Chargement…</p>

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: '1rem', marginBottom: '1.5rem' }}>
        <h1 style={{ margin: 0, fontSize: '1.25rem' }}>Catégories</h1>
        <span style={{ color: '#888', fontSize: '0.85rem' }}>{cats.length} catégorie(s)</span>
        <button onClick={() => setShowAdd(true)}
          style={{ marginLeft: 'auto', padding: '0.45rem 1rem', borderRadius: 6, border: 'none', background: '#4f8ef7', color: '#fff', fontWeight: 600, fontSize: '0.85rem', cursor: 'pointer' }}>
          + Ajouter
        </button>
      </div>

      {error && <p style={{ color: '#e53e3e', marginBottom: '1rem' }}>{error}</p>}

      {showAdd && (
        <div style={{ background: '#fff', borderRadius: 8, padding: '1.25rem', marginBottom: '1rem', boxShadow: '0 1px 3px rgba(0,0,0,0.08)' }}>
          <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr 1fr 1fr', gap: '0.75rem', marginBottom: '0.75rem' }}>
            <Lbl label="Nom *"><input value={draft.name} onChange={e => setDraft(p => ({ ...p, name: e.target.value }))} style={inputStyle} /></Lbl>
            <Lbl label="Catégorie parente">
              <select value={draft.parent_id ?? ''} onChange={e => setDraft(p => ({ ...p, parent_id: e.target.value || null }))} style={inputStyle}>
                <option value="">— Racine —</option>
                {buildOptions()}
              </select>
            </Lbl>
            <Lbl label="Station KDS"><input value={draft.kds_station ?? ''} onChange={e => setDraft(p => ({ ...p, kds_station: e.target.value || null }))} style={inputStyle} /></Lbl>
            <Lbl label="Imprimante"><input value={draft.printer_station ?? ''} onChange={e => setDraft(p => ({ ...p, printer_station: e.target.value || null }))} style={inputStyle} /></Lbl>
          </div>
          <div style={{ display: 'flex', gap: '0.5rem', justifyContent: 'flex-end' }}>
            <button onClick={() => { setShowAdd(false); setDraft(EMPTY) }} style={btnSecondary}>Annuler</button>
            <button onClick={handleAdd} disabled={saving}
              style={{ padding: '0.4rem 0.9rem', borderRadius: 6, border: 'none', background: '#4f8ef7', color: '#fff', fontWeight: 600, fontSize: '0.85rem', cursor: 'pointer' }}>
              Créer
            </button>
          </div>
        </div>
      )}

      <div style={{ background: '#fff', borderRadius: 8, boxShadow: '0 1px 3px rgba(0,0,0,0.08)', overflow: 'hidden' }}>
        <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.85rem' }}>
          <thead>
            <tr style={{ background: '#f1f3f5', textAlign: 'left' }}>
              {['Catégorie', 'Station KDS', 'Imprimante', 'Ordre', 'Actions'].map(h => (
                <th key={h} style={{ padding: '0.65rem 1rem', fontWeight: 600, fontSize: '0.75rem', color: '#555', textTransform: 'uppercase', letterSpacing: '0.05em' }}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {orderedRows.length === 0 && (
              <tr><td colSpan={5} style={{ padding: '2rem', textAlign: 'center', color: '#888' }}>Aucune catégorie.</td></tr>
            )}
            {orderedRows.map(({ cat, depth }) => (
              <tr key={cat.id} style={{ borderTop: '1px solid #f1f3f5' }}>
                {editId === cat.id ? (
                  <>
                    <td style={{ padding: '0.4rem 0.75rem' }}>
                      <input value={editDraft.name ?? ''} onChange={e => setEdit(p => ({ ...p, name: e.target.value }))} style={{ ...inputStyle, width: 'auto', minWidth: 160 }} />
                    </td>
                    <td style={{ padding: '0.4rem 0.75rem' }}>
                      <input value={editDraft.kds_station ?? ''} onChange={e => setEdit(p => ({ ...p, kds_station: e.target.value || null }))} style={{ ...inputStyle, width: 120 }} />
                    </td>
                    <td style={{ padding: '0.4rem 0.75rem' }}>
                      <input value={editDraft.printer_station ?? ''} onChange={e => setEdit(p => ({ ...p, printer_station: e.target.value || null }))} style={{ ...inputStyle, width: 120 }} />
                    </td>
                    <td style={{ padding: '0.4rem 0.75rem' }}>
                      <input type="number" value={editDraft.display_order ?? 0} onChange={e => setEdit(p => ({ ...p, display_order: parseInt(e.target.value) || 0 }))} style={{ ...inputStyle, width: 60 }} />
                    </td>
                    <td style={{ padding: '0.4rem 0.75rem' }}>
                      <div style={{ display: 'flex', gap: '0.4rem' }}>
                        <button onClick={handleSaveEdit} disabled={saving}
                          style={{ padding: '0.3rem 0.7rem', borderRadius: 5, border: 'none', background: '#4f8ef7', color: '#fff', fontSize: '0.8rem', cursor: 'pointer' }}>OK</button>
                        <button onClick={() => setEditId(null)} style={btnSecondary}>✕</button>
                      </div>
                    </td>
                  </>
                ) : (
                  <>
                    <td style={{ padding: '0.6rem 1rem' }}>
                      <span style={{ display: 'inline-block', paddingLeft: `${depth * 20}px`, fontWeight: depth === 0 ? 600 : 400 }}>
                        {depth > 0 && <span style={{ color: '#ccc', marginRight: '0.4rem' }}>↳</span>}
                        {cat.name}
                      </span>
                    </td>
                    <td style={{ padding: '0.6rem 1rem', color: '#888' }}>{cat.kds_station ?? '—'}</td>
                    <td style={{ padding: '0.6rem 1rem', color: '#888' }}>{cat.printer_station ?? '—'}</td>
                    <td style={{ padding: '0.6rem 1rem', color: '#888', fontFamily: 'monospace' }}>{cat.display_order}</td>
                    <td style={{ padding: '0.6rem 1rem' }}>
                      <div style={{ display: 'flex', gap: '0.4rem' }}>
                        <button onClick={() => { setEditId(cat.id); setEdit(cat) }} style={btnSecondary}>Éditer</button>
                        <button onClick={() => handleDelete(cat.id)}
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

function Lbl({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <label style={{ fontSize: '0.73rem', fontWeight: 600, color: '#777', display: 'block', marginBottom: 3, textTransform: 'uppercase', letterSpacing: '0.04em' }}>{label}</label>
      {children}
    </div>
  )
}
