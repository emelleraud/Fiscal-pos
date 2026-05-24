import { useEffect, useState, type CSSProperties } from 'react'
import { supabase } from '../supabaseClient'

interface ModifierGroup {
  id: string
  name: string
  min_select: number
  max_select: number
  is_required: boolean
}

interface Modifier {
  id: string
  group_id: string
  name: string
  price_delta_cents: number
  display_order: number
  is_active: boolean
}

const inputStyle: CSSProperties = {
  padding: '0.4rem 0.6rem', border: '1px solid #ddd', borderRadius: 5,
  fontSize: '0.85rem', boxSizing: 'border-box',
}

const EUR = (cents: number) =>
  cents === 0 ? '—' : (cents > 0 ? '+' : '') + (cents / 100).toLocaleString('fr-FR', { style: 'currency', currency: 'EUR' })

export default function ModifierGroupManager() {
  const [groups, setGroups]       = useState<ModifierGroup[]>([])
  const [modifiers, setModifiers] = useState<Modifier[]>([])
  const [expanded, setExpanded]   = useState<string | null>(null)
  const [loading, setLoading]     = useState(true)
  const [error, setError]         = useState<string | null>(null)
  const [saving, setSaving]       = useState(false)

  const [showAddGroup, setShowAddGroup] = useState(false)
  const [groupDraft, setGroupDraft]     = useState({ name: '', min_select: 0, max_select: 1, is_required: false })

  const [editGroupId, setEditGroupId]   = useState<string | null>(null)
  const [editGroupDraft, setEditGroup]  = useState<Partial<ModifierGroup>>({})

  const [addModFor, setAddModFor]       = useState<string | null>(null)
  const [modDraft, setModDraft]         = useState({ name: '', price_delta_cents: 0, price_input: '0.00' })
  const [editModId, setEditModId]       = useState<string | null>(null)
  const [editModDraft, setEditMod]      = useState<Partial<Modifier> & { price_input?: string }>({})

  async function load() {
    setLoading(true)
    const [{ data: g }, { data: m }] = await Promise.all([
      supabase.from('menu_modifier_groups').select('*').order('name'),
      supabase.from('menu_modifiers').select('*').order('display_order'),
    ])
    setGroups(g ?? [])
    setModifiers(m ?? [])
    setLoading(false)
  }

  useEffect(() => { load() }, [])

  async function handleAddGroup() {
    if (!groupDraft.name.trim()) return
    setSaving(true)
    const { error } = await supabase.from('menu_modifier_groups').insert(groupDraft)
    if (error) setError(error.message)
    else { setShowAddGroup(false); setGroupDraft({ name: '', min_select: 0, max_select: 1, is_required: false }); await load() }
    setSaving(false)
  }

  async function handleSaveGroup() {
    if (!editGroupId) return
    setSaving(true)
    const { error } = await supabase.from('menu_modifier_groups').update(editGroupDraft).eq('id', editGroupId)
    if (error) setError(error.message)
    else { setEditGroupId(null); await load() }
    setSaving(false)
  }

  async function handleDeleteGroup(id: string) {
    if (!confirm('Supprimer ce groupe et tous ses modificateurs ?')) return
    const { error } = await supabase.from('menu_modifier_groups').delete().eq('id', id)
    if (error) setError(error.message)
    else { if (expanded === id) setExpanded(null); await load() }
  }

  async function handleAddMod(groupId: string) {
    if (!modDraft.name.trim()) return
    setSaving(true)
    const mods = modifiers.filter(m => m.group_id === groupId)
    const { error } = await supabase.from('menu_modifiers').insert({
      group_id: groupId,
      name: modDraft.name,
      price_delta_cents: Math.round(parseFloat(modDraft.price_input.replace(',', '.')) * 100) || 0,
      display_order: mods.length,
      is_active: true,
    })
    if (error) setError(error.message)
    else { setAddModFor(null); setModDraft({ name: '', price_delta_cents: 0, price_input: '0.00' }); await load() }
    setSaving(false)
  }

  async function handleSaveMod() {
    if (!editModId) return
    setSaving(true)
    const { price_input, ...rest } = editModDraft
    const patch = {
      ...rest,
      price_delta_cents: Math.round(parseFloat((price_input ?? '0').replace(',', '.')) * 100) || 0,
    }
    const { error } = await supabase.from('menu_modifiers').update(patch).eq('id', editModId)
    if (error) setError(error.message)
    else { setEditModId(null); await load() }
    setSaving(false)
  }

  async function handleDeleteMod(id: string) {
    const { error } = await supabase.from('menu_modifiers').delete().eq('id', id)
    if (error) setError(error.message)
    else await load()
  }

  if (loading) return <p style={{ color: '#888' }}>Chargement…</p>

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: '1rem', marginBottom: '1.5rem' }}>
        <h1 style={{ margin: 0, fontSize: '1.25rem' }}>Groupes de modificateurs</h1>
        <span style={{ color: '#888', fontSize: '0.85rem' }}>{groups.length} groupe(s)</span>
        <button onClick={() => setShowAddGroup(true)}
          style={{ marginLeft: 'auto', padding: '0.45rem 1rem', borderRadius: 6, border: 'none', background: '#4f8ef7', color: '#fff', fontWeight: 600, fontSize: '0.85rem', cursor: 'pointer' }}>
          + Créer un groupe
        </button>
      </div>

      {error && <p style={{ color: '#e53e3e', marginBottom: '1rem' }}>{error}</p>}

      {showAddGroup && (
        <div style={{ background: '#fff', borderRadius: 8, padding: '1.25rem', marginBottom: '1rem', boxShadow: '0 1px 3px rgba(0,0,0,0.08)' }}>
          <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr 1fr 1fr', gap: '0.75rem', marginBottom: '0.75rem' }}>
            <Lbl label="Nom *">
              <input value={groupDraft.name} onChange={e => setGroupDraft(p => ({ ...p, name: e.target.value }))} style={{ ...inputStyle, width: '100%' }} />
            </Lbl>
            <Lbl label="Min choix">
              <input type="number" min={0} value={groupDraft.min_select} onChange={e => setGroupDraft(p => ({ ...p, min_select: parseInt(e.target.value) || 0 }))} style={{ ...inputStyle, width: '100%' }} />
            </Lbl>
            <Lbl label="Max choix">
              <input type="number" min={1} value={groupDraft.max_select} onChange={e => setGroupDraft(p => ({ ...p, max_select: parseInt(e.target.value) || 1 }))} style={{ ...inputStyle, width: '100%' }} />
            </Lbl>
            <Lbl label="Obligatoire">
              <label style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', marginTop: '0.35rem', cursor: 'pointer', fontSize: '0.85rem' }}>
                <input type="checkbox" checked={groupDraft.is_required} onChange={e => setGroupDraft(p => ({ ...p, is_required: e.target.checked }))} />
                Oui
              </label>
            </Lbl>
          </div>
          <div style={{ display: 'flex', gap: '0.5rem', justifyContent: 'flex-end' }}>
            <button onClick={() => setShowAddGroup(false)} style={{ padding: '0.4rem 0.9rem', borderRadius: 6, border: '1px solid #ddd', background: '#fff', cursor: 'pointer', fontSize: '0.85rem', color: '#555' }}>Annuler</button>
            <button onClick={handleAddGroup} disabled={saving}
              style={{ padding: '0.4rem 0.9rem', borderRadius: 6, border: 'none', background: '#4f8ef7', color: '#fff', fontWeight: 600, fontSize: '0.85rem', cursor: 'pointer' }}>Créer</button>
          </div>
        </div>
      )}

      {groups.length === 0 && !showAddGroup && (
        <p style={{ color: '#888' }}>Aucun groupe de modificateurs.</p>
      )}

      <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem' }}>
        {groups.map(group => {
          const mods = modifiers.filter(m => m.group_id === group.id)
          const isOpen = expanded === group.id
          return (
            <div key={group.id} style={{ background: '#fff', borderRadius: 8, boxShadow: '0 1px 3px rgba(0,0,0,0.08)', overflow: 'hidden' }}>
              {/* Group header */}
              {editGroupId === group.id ? (
                <div style={{ padding: '0.85rem 1.25rem', display: 'grid', gridTemplateColumns: '2fr 1fr 1fr 1fr auto', gap: '0.75rem', alignItems: 'end' }}>
                  <Lbl label="Nom">
                    <input value={editGroupDraft.name ?? ''} onChange={e => setEditGroup(p => ({ ...p, name: e.target.value }))} style={{ ...inputStyle, width: '100%' }} />
                  </Lbl>
                  <Lbl label="Min">
                    <input type="number" min={0} value={editGroupDraft.min_select ?? 0} onChange={e => setEditGroup(p => ({ ...p, min_select: parseInt(e.target.value) || 0 }))} style={{ ...inputStyle, width: '100%' }} />
                  </Lbl>
                  <Lbl label="Max">
                    <input type="number" min={1} value={editGroupDraft.max_select ?? 1} onChange={e => setEditGroup(p => ({ ...p, max_select: parseInt(e.target.value) || 1 }))} style={{ ...inputStyle, width: '100%' }} />
                  </Lbl>
                  <Lbl label="Obligatoire">
                    <label style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', marginTop: '0.35rem', cursor: 'pointer', fontSize: '0.85rem' }}>
                      <input type="checkbox" checked={editGroupDraft.is_required ?? false} onChange={e => setEditGroup(p => ({ ...p, is_required: e.target.checked }))} />
                      Oui
                    </label>
                  </Lbl>
                  <div style={{ display: 'flex', gap: '0.4rem' }}>
                    <button onClick={handleSaveGroup} disabled={saving}
                      style={{ padding: '0.3rem 0.7rem', borderRadius: 5, border: 'none', background: '#4f8ef7', color: '#fff', fontSize: '0.8rem', cursor: 'pointer' }}>OK</button>
                    <button onClick={() => setEditGroupId(null)}
                      style={{ padding: '0.3rem 0.7rem', borderRadius: 5, border: '1px solid #ddd', background: '#fff', fontSize: '0.8rem', cursor: 'pointer' }}>✕</button>
                  </div>
                </div>
              ) : (
                <div style={{ display: 'flex', alignItems: 'center', gap: '1rem', padding: '0.85rem 1.25rem' }}>
                  <span style={{ fontWeight: 600, flex: 1, cursor: 'pointer' }} onClick={() => setExpanded(isOpen ? null : group.id)}>
                    {group.name}
                  </span>
                  <span style={{ color: '#888', fontSize: '0.78rem' }}>
                    {group.is_required ? 'Obligatoire · ' : ''}
                    {group.min_select}–{group.max_select} choix · {mods.length} option(s)
                  </span>
                  <button onClick={() => { setEditGroupId(group.id); setEditGroup(group) }}
                    style={{ padding: '0.3rem 0.7rem', borderRadius: 5, border: '1px solid #ddd', background: '#fff', fontSize: '0.8rem', cursor: 'pointer' }}>Éditer</button>
                  <button onClick={() => handleDeleteGroup(group.id)}
                    style={{ padding: '0.3rem 0.7rem', borderRadius: 5, border: '1px solid #ffc9c9', background: '#fff', fontSize: '0.8rem', cursor: 'pointer', color: '#c0392b' }}>Suppr.</button>
                  <span style={{ color: '#aaa', fontSize: '0.75rem', cursor: 'pointer' }} onClick={() => setExpanded(isOpen ? null : group.id)}>
                    {isOpen ? '▲' : '▼'}
                  </span>
                </div>
              )}

              {/* Modifiers list */}
              {isOpen && (
                <div style={{ borderTop: '1px solid #f1f3f5', background: '#fafbfc' }}>
                  <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.83rem' }}>
                    <thead>
                      <tr style={{ textAlign: 'left' }}>
                        {['Option', 'Supplément', 'Actif', ''].map(h => (
                          <th key={h} style={{ padding: '0.5rem 1rem', fontWeight: 600, fontSize: '0.73rem', color: '#888', textTransform: 'uppercase', letterSpacing: '0.04em' }}>{h}</th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {mods.map(mod => (
                        <tr key={mod.id} style={{ borderTop: '1px solid #f1f3f5' }}>
                          {editModId === mod.id ? (
                            <>
                              <td style={{ padding: '0.4rem 1rem' }}>
                                <input value={editModDraft.name ?? ''} onChange={e => setEditMod(p => ({ ...p, name: e.target.value }))} style={{ ...inputStyle, width: 160 }} />
                              </td>
                              <td style={{ padding: '0.4rem 1rem' }}>
                                <input type="number" step="0.01" value={editModDraft.price_input ?? '0.00'} onChange={e => setEditMod(p => ({ ...p, price_input: e.target.value }))} style={{ ...inputStyle, width: 90 }} />
                              </td>
                              <td style={{ padding: '0.4rem 1rem' }}>
                                <input type="checkbox" checked={editModDraft.is_active ?? true} onChange={e => setEditMod(p => ({ ...p, is_active: e.target.checked }))} />
                              </td>
                              <td style={{ padding: '0.4rem 1rem' }}>
                                <div style={{ display: 'flex', gap: '0.4rem' }}>
                                  <button onClick={handleSaveMod} disabled={saving}
                                    style={{ padding: '0.25rem 0.6rem', borderRadius: 4, border: 'none', background: '#4f8ef7', color: '#fff', fontSize: '0.78rem', cursor: 'pointer' }}>OK</button>
                                  <button onClick={() => setEditModId(null)}
                                    style={{ padding: '0.25rem 0.6rem', borderRadius: 4, border: '1px solid #ddd', background: '#fff', fontSize: '0.78rem', cursor: 'pointer' }}>✕</button>
                                </div>
                              </td>
                            </>
                          ) : (
                            <>
                              <td style={{ padding: '0.5rem 1rem', color: mod.is_active ? '#222' : '#aaa' }}>{mod.name}</td>
                              <td style={{ padding: '0.5rem 1rem', fontFamily: 'monospace', color: mod.price_delta_cents > 0 ? '#2d6a4f' : mod.price_delta_cents < 0 ? '#c0392b' : '#aaa' }}>
                                {EUR(mod.price_delta_cents)}
                              </td>
                              <td style={{ padding: '0.5rem 1rem' }}>
                                <span style={{ fontSize: '0.75rem', fontWeight: 600, padding: '2px 6px', borderRadius: 4,
                                  background: mod.is_active ? '#d4edda' : '#f8d7da',
                                  color: mod.is_active ? '#2d6a4f' : '#c0392b' }}>
                                  {mod.is_active ? 'Actif' : 'Inactif'}
                                </span>
                              </td>
                              <td style={{ padding: '0.5rem 1rem' }}>
                                <div style={{ display: 'flex', gap: '0.4rem' }}>
                                  <button onClick={() => { setEditModId(mod.id); setEditMod({ ...mod, price_input: (mod.price_delta_cents / 100).toFixed(2) }) }}
                                    style={{ padding: '0.25rem 0.6rem', borderRadius: 4, border: '1px solid #ddd', background: '#fff', fontSize: '0.78rem', cursor: 'pointer' }}>Éditer</button>
                                  <button onClick={() => handleDeleteMod(mod.id)}
                                    style={{ padding: '0.25rem 0.6rem', borderRadius: 4, border: '1px solid #ffc9c9', background: '#fff', fontSize: '0.78rem', cursor: 'pointer', color: '#c0392b' }}>×</button>
                                </div>
                              </td>
                            </>
                          )}
                        </tr>
                      ))}

                      {/* Add modifier row */}
                      {addModFor === group.id ? (
                        <tr style={{ borderTop: '1px solid #f1f3f5', background: '#fff' }}>
                          <td style={{ padding: '0.4rem 1rem' }}>
                            <input autoFocus placeholder="Nom de l'option" value={modDraft.name}
                              onChange={e => setModDraft(p => ({ ...p, name: e.target.value }))} style={{ ...inputStyle, width: 160 }} />
                          </td>
                          <td style={{ padding: '0.4rem 1rem' }}>
                            <input type="number" step="0.01" placeholder="0.00" value={modDraft.price_input}
                              onChange={e => setModDraft(p => ({ ...p, price_input: e.target.value }))} style={{ ...inputStyle, width: 90 }} />
                          </td>
                          <td />
                          <td style={{ padding: '0.4rem 1rem' }}>
                            <div style={{ display: 'flex', gap: '0.4rem' }}>
                              <button onClick={() => handleAddMod(group.id)} disabled={saving}
                                style={{ padding: '0.25rem 0.6rem', borderRadius: 4, border: 'none', background: '#4f8ef7', color: '#fff', fontSize: '0.78rem', cursor: 'pointer' }}>OK</button>
                              <button onClick={() => setAddModFor(null)}
                                style={{ padding: '0.25rem 0.6rem', borderRadius: 4, border: '1px solid #ddd', background: '#fff', fontSize: '0.78rem', cursor: 'pointer' }}>✕</button>
                            </div>
                          </td>
                        </tr>
                      ) : (
                        <tr style={{ borderTop: '1px solid #f1f3f5' }}>
                          <td colSpan={4} style={{ padding: '0.5rem 1rem' }}>
                            <button onClick={() => { setAddModFor(group.id); setModDraft({ name: '', price_delta_cents: 0, price_input: '0.00' }) }}
                              style={{ background: 'none', border: 'none', color: '#4f8ef7', cursor: 'pointer', fontSize: '0.82rem', padding: 0 }}>
                              + Ajouter une option
                            </button>
                          </td>
                        </tr>
                      )}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )
        })}
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
