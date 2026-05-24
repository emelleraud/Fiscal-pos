import { useEffect, useState, type CSSProperties, type ReactNode } from 'react'
import { useParams, useNavigate, Link } from 'react-router-dom'
import { supabase } from '../supabaseClient'

// ---- Types ----

type DestKey = 'visible_caisse' | 'visible_kiosk' | 'visible_delivery' | 'visible_drive' | 'visible_digital'

interface ComboDraft {
  category_id: string
  sku: string
  name: string
  description: string
  base_price: string
  is_active: boolean
  visible_caisse:   boolean
  visible_kiosk:    boolean
  visible_delivery: boolean
  visible_drive:    boolean
  visible_digital:  boolean
}

interface FixedItemDraft {
  _k: string
  target: string      // "product:UUID" | "variant:UUID"
  quantity: number
  display_order: number
}

interface SlotOptionDraft {
  _k: string
  target: string      // "product:UUID" | "variant:UUID"
  price_delta: string // euros, ex: "1.50"
  is_default: boolean
  display_order: number
}

interface SlotDraft {
  _k: string
  name: string
  min_select: number
  max_select: number
  is_required: boolean
  display_order: number
  options: SlotOptionDraft[]
}

interface CatalogProduct { id: string; name: string }
interface CatalogVariant { id: string; name: string; menu_products: { name: string } }
interface Category { id: string; parent_id: string | null; name: string }

// ---- Constants ----

const DESTINATIONS: { key: DestKey; label: string }[] = [
  { key: 'visible_caisse',   label: 'Caisse' },
  { key: 'visible_kiosk',    label: 'Kiosk' },
  { key: 'visible_delivery', label: 'Livraison' },
  { key: 'visible_drive',    label: 'Drive' },
  { key: 'visible_digital',  label: 'Affichage digital' },
]

const INITIAL: ComboDraft = {
  category_id: '', sku: '', name: '', description: '',
  base_price: '0.00', is_active: true,
  visible_caisse: true, visible_kiosk: true, visible_delivery: true,
  visible_drive: true, visible_digital: true,
}

// ---- Helpers ----

const toEuros = (cents: number) => (cents / 100).toFixed(2)
const toCents = (s: string) => Math.round(parseFloat(s.replace(',', '.')) * 100) || 0

function parseTarget(value: string): { product_id: string | null; variant_id: string | null } {
  if (value.startsWith('product:')) return { product_id: value.slice(8), variant_id: null }
  if (value.startsWith('variant:')) return { product_id: null, variant_id: value.slice(8) }
  return { product_id: null, variant_id: null }
}

function toTarget(item: { product_id: string | null; variant_id: string | null }): string {
  if (item.product_id) return `product:${item.product_id}`
  if (item.variant_id) return `variant:${item.variant_id}`
  return ''
}

// ---- Shared styles ----

const inputStyle: CSSProperties = {
  padding: '0.5rem 0.65rem', border: '1px solid #ddd', borderRadius: 6,
  fontSize: '0.875rem', width: '100%', boxSizing: 'border-box',
}

// ---- Component ----

export default function ComboForm() {
  const { id }   = useParams<{ id: string }>()
  const isNew    = !id
  const navigate = useNavigate()

  const [combo, setCombo]        = useState<ComboDraft>(INITIAL)
  const [fixedItems, setFixed]   = useState<FixedItemDraft[]>([])
  const [slots, setSlots]        = useState<SlotDraft[]>([])

  const [categories, setCats]    = useState<Category[]>([])
  const [allProducts, setProds]  = useState<CatalogProduct[]>([])
  const [allVariants, setVars]   = useState<CatalogVariant[]>([])

  const [loading, setLoading]    = useState(!isNew)
  const [saving, setSaving]      = useState(false)
  const [error, setError]        = useState<string | null>(null)

  // Reference data
  useEffect(() => {
    supabase.from('menu_categories').select('id,parent_id,name').order('display_order')
      .then(({ data }) => setCats(data ?? []))
    supabase.from('menu_products').select('id,name').eq('is_active', true).order('name')
      .then(({ data }) => setProds(data ?? []))
    supabase.from('menu_variants').select('id,name,menu_products(name)').eq('is_active', true).order('name')
      .then(({ data }) => setVars((data ?? []) as unknown as CatalogVariant[]))
  }, [])

  // Load combo in edit mode
  useEffect(() => {
    if (isNew) return
    setLoading(true)
    Promise.all([
      supabase.from('menu_combos').select('*').eq('id', id).single(),
      supabase.from('menu_combo_fixed_items').select('*').eq('combo_id', id).order('display_order'),
      supabase.from('menu_combo_slots')
        .select('id,name,min_select,max_select,is_required,display_order,menu_combo_slot_options(id,product_id,variant_id,price_delta_cents,display_order,is_default)')
        .eq('combo_id', id)
        .order('display_order'),
    ]).then(([{ data: c, error: e1 }, { data: fi }, { data: sl }]) => {
      if (e1 || !c) { setError('Combo introuvable'); setLoading(false); return }
      setCombo({
        category_id:      c.category_id ?? '',
        sku:              c.sku ?? '',
        name:             c.name,
        description:      c.description ?? '',
        base_price:       toEuros(c.base_price_cents),
        is_active:        c.is_active,
        visible_caisse:   c.visible_caisse,
        visible_kiosk:    c.visible_kiosk,
        visible_delivery: c.visible_delivery,
        visible_drive:    c.visible_drive,
        visible_digital:  c.visible_digital,
      })
      setFixed((fi ?? []).map((f: Record<string, unknown>) => ({
        _k: f.id as string,
        target: toTarget(f as { product_id: string | null; variant_id: string | null }),
        quantity: f.quantity as number,
        display_order: f.display_order as number,
      })))
      setSlots((sl ?? []).map((s: Record<string, unknown>) => ({
        _k: s.id as string,
        name: s.name as string,
        min_select: s.min_select as number,
        max_select: s.max_select as number,
        is_required: s.is_required as boolean,
        display_order: s.display_order as number,
        options: [...((s.menu_combo_slot_options as Record<string, unknown>[]) ?? [])]
          .sort((a, b) => (a.display_order as number) - (b.display_order as number))
          .map((o) => ({
            _k: o.id as string,
            target: toTarget(o as { product_id: string | null; variant_id: string | null }),
            price_delta: toEuros(o.price_delta_cents as number),
            is_default: o.is_default as boolean,
            display_order: o.display_order as number,
          })),
      })))
      setLoading(false)
    }).catch(err => {
      setError(String(err))
      setLoading(false)
    })
  }, [id, isNew])

  // ---- Fixed item helpers ----

  function addFixedItem() {
    setFixed(prev => [...prev, { _k: `new-${Date.now()}`, target: '', quantity: 1, display_order: prev.length }])
  }

  function updateFixed(k: string, patch: Partial<FixedItemDraft>) {
    setFixed(prev => prev.map(f => f._k === k ? { ...f, ...patch } : f))
  }

  function removeFixed(k: string) {
    setFixed(prev => prev.filter(f => f._k !== k))
  }

  // ---- Slot helpers ----

  function addSlot() {
    setSlots(prev => [...prev, {
      _k: `new-${Date.now()}`, name: '', min_select: 1, max_select: 1,
      is_required: true, display_order: prev.length, options: [],
    }])
  }

  function updateSlot(k: string, patch: Partial<Omit<SlotDraft, 'options'>>) {
    setSlots(prev => prev.map(s => s._k === k ? { ...s, ...patch } : s))
  }

  function removeSlot(k: string) {
    setSlots(prev => prev.filter(s => s._k !== k))
  }

  function addOption(slotKey: string) {
    setSlots(prev => prev.map(s =>
      s._k === slotKey
        ? { ...s, options: [...s.options, { _k: `new-${Date.now()}`, target: '', price_delta: '0.00', is_default: false, display_order: s.options.length }] }
        : s
    ))
  }

  function updateOption(slotKey: string, optKey: string, patch: Partial<SlotOptionDraft>) {
    setSlots(prev => prev.map(s =>
      s._k === slotKey
        ? { ...s, options: s.options.map(o => o._k === optKey ? { ...o, ...patch } : o) }
        : s
    ))
  }

  function removeOption(slotKey: string, optKey: string) {
    setSlots(prev => prev.map(s =>
      s._k === slotKey ? { ...s, options: s.options.filter(o => o._k !== optKey) } : s
    ))
  }

  // ---- Delete ----

  async function handleDelete() {
    if (!confirm(`Supprimer le combo « ${combo.name} » ?`)) return
    const { error } = await supabase.from('menu_combos').delete().eq('id', id!)
    if (error) { setError(error.message); return }
    navigate('/combos')
  }

  // ---- Save ----

  async function handleSave() {
    if (!combo.name.trim()) { setError('Le nom est obligatoire'); return }
    for (const f of fixedItems) {
      if (!f.target) { setError('Chaque item fixe doit avoir un produit ou variante sélectionné'); return }
    }
    for (const s of slots) {
      if (!s.name.trim()) { setError('Chaque slot doit avoir un nom'); return }
      if (s.min_select > s.max_select) {
        setError(`Slot "${s.name}" : le minimum (${s.min_select}) ne peut pas dépasser le maximum (${s.max_select})`)
        return
      }
      if (s.options.length === 0) { setError(`Le slot "${s.name}" doit avoir au moins 1 option`); return }
      for (const o of s.options) {
        if (!o.target) { setError(`Slot "${s.name}" : chaque option doit avoir un produit ou variante sélectionné`); return }
      }
    }

    setSaving(true)
    setError(null)

    const payload = {
      category_id:      combo.category_id || null,
      sku:              combo.sku.trim() || `CMB-${Date.now()}`,
      name:             combo.name,
      description:      combo.description || null,
      base_price_cents: toCents(combo.base_price),
      is_active:        combo.is_active,
      visible_caisse:   combo.visible_caisse,
      visible_kiosk:    combo.visible_kiosk,
      visible_delivery: combo.visible_delivery,
      visible_drive:    combo.visible_drive,
      visible_digital:  combo.visible_digital,
    }

    let comboId = id

    if (isNew) {
      const { data, error } = await supabase.from('menu_combos').insert(payload).select('id').single()
      if (error || !data) { setError(error?.message ?? 'Erreur'); setSaving(false); return }
      comboId = data.id
    } else {
      const { error } = await supabase.from('menu_combos').update(payload).eq('id', id!)
      if (error) { setError(error.message); setSaving(false); return }
    }

    // Delete-all + reinsert fixed items
    const { error: delFixed } = await supabase.from('menu_combo_fixed_items').delete().eq('combo_id', comboId!)
    if (delFixed) { setError(delFixed.message); setSaving(false); return }
    if (fixedItems.length > 0) {
      const { error } = await supabase.from('menu_combo_fixed_items').insert(
        fixedItems.map((f, i) => ({
          combo_id: comboId!,
          ...parseTarget(f.target),
          quantity: f.quantity,
          display_order: i,
        }))
      )
      if (error) { setError(error.message); setSaving(false); return }
    }

    // Delete-all slots (CASCADE removes options), then reinsert sequentially to get IDs
    const { error: delSlots } = await supabase.from('menu_combo_slots').delete().eq('combo_id', comboId!)
    if (delSlots) { setError(delSlots.message); setSaving(false); return }
    for (const [si, slot] of slots.entries()) {
      const { data: slotRow, error: slotErr } = await supabase
        .from('menu_combo_slots')
        .insert({
          combo_id:      comboId!,
          name:          slot.name,
          display_order: si,
          min_select:    slot.min_select,
          max_select:    slot.max_select,
          is_required:   slot.is_required,
        })
        .select('id')
        .single()
      if (slotErr || !slotRow) { setError(slotErr?.message ?? 'Erreur slot'); setSaving(false); return }

      if (slot.options.length > 0) {
        const { error: optErr } = await supabase.from('menu_combo_slot_options').insert(
          slot.options.map((o, oi) => ({
            slot_id:           slotRow.id,
            ...parseTarget(o.target),
            price_delta_cents: toCents(o.price_delta),
            display_order:     oi,
            is_default:        o.is_default,
          }))
        )
        if (optErr) { setError(optErr.message); setSaving(false); return }
      }
    }

    setSaving(false)
    navigate('/combos')
  }

  // ---- Render ----

  if (loading) return <p style={{ color: '#888', padding: '2rem' }}>Chargement…</p>

  const parents = categories.filter(c => !c.parent_id)
  const childOf = (pid: string) => categories.filter(c => c.parent_id === pid)

  return (
    <div style={{ maxWidth: 900 }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: '1rem', marginBottom: '1.5rem' }}>
        <Link to="/combos" style={{ color: '#4f8ef7', textDecoration: 'none', fontSize: '0.85rem' }}>← Combos</Link>
        <h1 style={{ margin: 0, fontSize: '1.25rem', flex: 1 }}>
          {isNew ? 'Nouveau combo' : combo.name}
        </h1>
        {!isNew && (
          <button onClick={handleDelete}
            style={{ padding: '0.5rem 1rem', borderRadius: 6, border: '1px solid #ffc9c9', background: '#fff', color: '#c0392b', fontSize: '0.875rem', cursor: 'pointer' }}>
            Supprimer
          </button>
        )}
        <button onClick={handleSave} disabled={saving}
          style={{ padding: '0.5rem 1.25rem', borderRadius: 6, border: 'none', background: '#4f8ef7', color: '#fff', fontWeight: 600, cursor: saving ? 'not-allowed' : 'pointer', opacity: saving ? 0.7 : 1 }}>
          {saving ? 'Enregistrement…' : 'Enregistrer'}
        </button>
      </div>

      {error && (
        <p style={{ color: '#e53e3e', marginBottom: '1rem', padding: '0.75rem 1rem', background: '#fff5f5', borderRadius: 6, fontSize: '0.875rem' }}>{error}</p>
      )}

      {/* Informations */}
      <Card title="Informations">
        <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr', gap: '1rem', marginBottom: '1rem' }}>
          <Field label="Nom *">
            <input value={combo.name} onChange={e => setCombo(p => ({ ...p, name: e.target.value }))} style={inputStyle} />
          </Field>
          <Field label="Statut">
            <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginTop: '0.3rem', cursor: 'pointer', fontSize: '0.875rem' }}>
              <input type="checkbox" checked={combo.is_active} onChange={e => setCombo(p => ({ ...p, is_active: e.target.checked }))} />
              <span style={{ color: combo.is_active ? '#2d6a4f' : '#c0392b', fontWeight: 500 }}>
                {combo.is_active ? 'Actif' : 'Inactif'}
              </span>
            </label>
          </Field>
        </div>
        <div style={{ marginBottom: '1rem' }}>
          <Field label="Description">
            <textarea value={combo.description} onChange={e => setCombo(p => ({ ...p, description: e.target.value }))}
              rows={2} style={{ ...inputStyle, resize: 'vertical' }} />
          </Field>
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr 1fr', gap: '1rem' }}>
          <Field label="Catégorie">
            <select value={combo.category_id} onChange={e => setCombo(p => ({ ...p, category_id: e.target.value }))} style={inputStyle}>
              <option value="">— Aucune —</option>
              {parents.flatMap(cat => [
                <option key={cat.id} value={cat.id}>{cat.name}</option>,
                ...childOf(cat.id).map(c => <option key={c.id} value={c.id}>&nbsp;&nbsp;↳ {c.name}</option>),
              ])}
            </select>
          </Field>
          <Field label="Prix de base (€)">
            <input type="number" step="0.01" min="0" value={combo.base_price}
              onChange={e => setCombo(p => ({ ...p, base_price: e.target.value }))} style={inputStyle} />
          </Field>
          <Field label="SKU">
            <input value={combo.sku} onChange={e => setCombo(p => ({ ...p, sku: e.target.value }))}
              placeholder="Auto si vide" style={inputStyle} />
          </Field>
        </div>
      </Card>

      {/* Destinations */}
      <Card title="Destinations">
        <div style={{ display: 'flex', gap: '1.5rem', flexWrap: 'wrap' }}>
          {DESTINATIONS.map(({ key, label }) => (
            <label key={key} style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', cursor: 'pointer', fontSize: '0.875rem' }}>
              <input type="checkbox" checked={combo[key]} onChange={e => setCombo(p => ({ ...p, [key]: e.target.checked }))} />
              {label}
            </label>
          ))}
        </div>
      </Card>

      {/* Items fixes */}
      <Card title={`Items fixes (${fixedItems.length})`} action={
        <button onClick={addFixedItem}
          style={{ padding: '0.3rem 0.8rem', borderRadius: 6, border: 'none', background: '#4f8ef7', color: '#fff', fontSize: '0.8rem', fontWeight: 600, cursor: 'pointer' }}>
          + Ajouter
        </button>
      }>
        {fixedItems.length === 0 ? (
          <p style={{ color: '#aaa', fontSize: '0.85rem', margin: 0 }}>Aucun item fixe.</p>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '0.4rem' }}>
            {fixedItems.map(f => (
              <div key={f._k} style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                <TargetSelect
                  value={f.target}
                  onChange={v => updateFixed(f._k, { target: v })}
                  products={allProducts}
                  variants={allVariants}
                  style={{ flex: 1 }}
                />
                <input
                  type="number" min="1" value={f.quantity}
                  onChange={e => updateFixed(f._k, { quantity: parseInt(e.target.value) || 1 })}
                  style={{ ...inputStyle, width: 70 }}
                  title="Quantité"
                />
                <button onClick={() => removeFixed(f._k)}
                  style={{ background: 'none', border: 'none', color: '#c0392b', cursor: 'pointer', fontSize: '1.2rem', lineHeight: 1, padding: '0 0.3rem' }}>×</button>
              </div>
            ))}
          </div>
        )}
      </Card>

      {/* Slots configurables */}
      <Card title={`Slots configurables (${slots.length})`} action={
        <button onClick={addSlot}
          style={{ padding: '0.3rem 0.8rem', borderRadius: 6, border: 'none', background: '#4f8ef7', color: '#fff', fontSize: '0.8rem', fontWeight: 600, cursor: 'pointer' }}>
          + Ajouter slot
        </button>
      }>
        {slots.length === 0 ? (
          <p style={{ color: '#aaa', fontSize: '0.85rem', margin: 0 }}>Aucun slot configurable.</p>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '0.75rem' }}>
            {slots.map(s => (
              <div key={s._k} style={{ border: '1px solid #dee2e6', borderRadius: 6, overflow: 'hidden' }}>
                <div style={{ display: 'flex', gap: '0.5rem', alignItems: 'center', padding: '0.5rem 0.75rem', background: '#f8f9fa' }}>
                  <input
                    value={s.name} onChange={e => updateSlot(s._k, { name: e.target.value })}
                    placeholder="Nom du slot (ex: Choisissez votre boisson)"
                    style={{ ...inputStyle, flex: 1, fontSize: '0.85rem' }}
                  />
                  <label style={{ fontSize: '0.78rem', color: '#666', whiteSpace: 'nowrap' }}>
                    min&nbsp;
                    <input type="number" min="0" value={s.min_select}
                      onChange={e => updateSlot(s._k, { min_select: parseInt(e.target.value) || 0 })}
                      style={{ width: 44, border: '1px solid #ddd', borderRadius: 4, padding: '2px 4px', fontSize: '0.85rem' }}
                    />
                  </label>
                  <label style={{ fontSize: '0.78rem', color: '#666', whiteSpace: 'nowrap' }}>
                    max&nbsp;
                    <input type="number" min="1" value={s.max_select}
                      onChange={e => updateSlot(s._k, { max_select: parseInt(e.target.value) || 1 })}
                      style={{ width: 44, border: '1px solid #ddd', borderRadius: 4, padding: '2px 4px', fontSize: '0.85rem' }}
                    />
                  </label>
                  <label style={{ fontSize: '0.78rem', color: '#666', display: 'flex', alignItems: 'center', gap: 4, cursor: 'pointer' }}>
                    <input type="checkbox" checked={s.is_required}
                      onChange={e => updateSlot(s._k, { is_required: e.target.checked })} />
                    Obligatoire
                  </label>
                  <button onClick={() => addOption(s._k)}
                    style={{ padding: '0.25rem 0.6rem', borderRadius: 5, border: '1px solid #4f8ef7', background: '#fff', color: '#4f8ef7', fontSize: '0.78rem', cursor: 'pointer', whiteSpace: 'nowrap' }}>
                    + Option
                  </button>
                  <button onClick={() => removeSlot(s._k)}
                    style={{ background: 'none', border: 'none', color: '#c0392b', cursor: 'pointer', fontSize: '1.2rem', lineHeight: 1 }}>×</button>
                </div>
                <div style={{ padding: '0.4rem 0.75rem', display: 'flex', flexDirection: 'column', gap: '0.3rem' }}>
                  {s.options.length === 0 && (
                    <p style={{ color: '#aaa', fontSize: '0.8rem', margin: '0.3rem 0' }}>Aucune option — cliquez « + Option ».</p>
                  )}
                  {s.options.map(o => (
                    <div key={o._k} style={{ display: 'flex', gap: '0.5rem', alignItems: 'center' }}>
                      <TargetSelect
                        value={o.target}
                        onChange={v => updateOption(s._k, o._k, { target: v })}
                        products={allProducts}
                        variants={allVariants}
                        style={{ flex: 1 }}
                      />
                      <input
                        type="number" step="0.01" min="0" value={o.price_delta}
                        onChange={e => updateOption(s._k, o._k, { price_delta: e.target.value })}
                        style={{ ...inputStyle, width: 90 }}
                        title="Surcharge (€)"
                        placeholder="+0.00"
                      />
                      <label style={{ fontSize: '0.78rem', color: '#666', display: 'flex', alignItems: 'center', gap: 4, cursor: 'pointer', whiteSpace: 'nowrap' }}>
                        <input type="checkbox" checked={o.is_default}
                          onChange={e => updateOption(s._k, o._k, { is_default: e.target.checked })} />
                        Défaut
                      </label>
                      <button onClick={() => removeOption(s._k, o._k)}
                        style={{ background: 'none', border: 'none', color: '#c0392b', cursor: 'pointer', fontSize: '1.2rem', lineHeight: 1 }}>×</button>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>
    </div>
  )
}

// ---- Sub-components ----

function TargetSelect({ value, onChange, products, variants, style }: {
  value: string
  onChange: (v: string) => void
  products: CatalogProduct[]
  variants: CatalogVariant[]
  style?: CSSProperties
}) {
  return (
    <select value={value} onChange={e => onChange(e.target.value)} style={{ padding: '0.5rem 0.65rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.875rem', boxSizing: 'border-box', ...style }}>
      <option value="">— Choisir produit ou variante —</option>
      {products.length > 0 && (
        <optgroup label="Produits">
          {products.map(p => (
            <option key={p.id} value={`product:${p.id}`}>{p.name}</option>
          ))}
        </optgroup>
      )}
      {variants.length > 0 && (
        <optgroup label="Variantes">
          {variants.map(v => (
            <option key={v.id} value={`variant:${v.id}`}>
              {(v.menu_products as { name: string }).name} — {v.name}
            </option>
          ))}
        </optgroup>
      )}
    </select>
  )
}

function Card({ title, children, action }: { title: string; children: ReactNode; action?: ReactNode }) {
  return (
    <div style={{ background: '#fff', borderRadius: 8, boxShadow: '0 1px 3px rgba(0,0,0,0.08)', padding: '1.25rem', marginBottom: '1rem' }}>
      <div style={{ display: 'flex', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0, fontSize: '0.8rem', fontWeight: 700, color: '#555', textTransform: 'uppercase', letterSpacing: '0.08em', flex: 1 }}>{title}</h2>
        {action}
      </div>
      {children}
    </div>
  )
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div>
      <label style={{ fontSize: '0.73rem', fontWeight: 600, color: '#777', display: 'block', marginBottom: '0.35rem', textTransform: 'uppercase', letterSpacing: '0.04em' }}>{label}</label>
      {children}
    </div>
  )
}
