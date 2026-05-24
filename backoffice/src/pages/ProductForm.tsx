import { useEffect, useState, type CSSProperties, type ReactNode } from 'react'
import { useParams, useNavigate, Link } from 'react-router-dom'
import { supabase } from '../supabaseClient'

// ---- Types ----

interface Category { id: string; parent_id: string | null; name: string }
interface ModGroup  { id: string; name: string; min_select: number; max_select: number; is_required: boolean }

type TvaRate = 'reduit5_5' | 'intermediaire10' | 'normal20'
type DestKey = 'visible_caisse' | 'visible_kiosk' | 'visible_delivery' | 'visible_drive' | 'visible_digital'

interface ProductDraft {
  category_id: string
  sku: string
  name: string
  description: string
  tva_rate: TvaRate
  price: string
  image_url: string
  is_active: boolean
  visible_caisse:   boolean
  visible_kiosk:    boolean
  visible_delivery: boolean
  visible_drive:    boolean
  visible_digital:  boolean
}

interface VariantDraft {
  _k: string
  id?: string
  name: string
  sku: string
  price: string        // '' = hérite
  display_order: number
  is_active: boolean
}

interface LinkedGroup {
  _k: string
  link_id?: string
  group_id: string
  group_name: string
  min_select: number
  max_select: number
  is_required: boolean
  display_order: number
}

// ---- Constants ----

const TVA_LABELS: Record<string, string> = {
  reduit5_5: '5,5 %', intermediaire10: '10 %', normal20: '20 %',
}

const DESTINATIONS: { key: DestKey; label: string }[] = [
  { key: 'visible_caisse',   label: 'Caisse' },
  { key: 'visible_kiosk',    label: 'Kiosk' },
  { key: 'visible_delivery', label: 'Livraison' },
  { key: 'visible_drive',    label: 'Drive' },
  { key: 'visible_digital',  label: 'Affichage digital' },
]

const INITIAL: ProductDraft = {
  category_id: '', sku: '', name: '', description: '',
  tva_rate: 'intermediaire10', price: '0.00', image_url: '', is_active: true,
  visible_caisse: true, visible_kiosk: true, visible_delivery: true,
  visible_drive: true, visible_digital: true,
}

const toEuros = (cents: number) => (cents / 100).toFixed(2)
const toCents = (s: string)     => Math.round(parseFloat(s.replace(',', '.')) * 100) || 0

// ---- Shared styles ----

const inputStyle: CSSProperties = {
  padding: '0.5rem 0.65rem', border: '1px solid #ddd', borderRadius: 6,
  fontSize: '0.875rem', width: '100%', boxSizing: 'border-box',
}

// ---- Component ----

export default function ProductForm() {
  const { id }  = useParams<{ id: string }>()
  const isNew   = !id
  const navigate = useNavigate()

  const [product, setProduct]     = useState<ProductDraft>(INITIAL)
  const [variants, setVariants]   = useState<VariantDraft[]>([])
  const [deletedIds, setDeleted]  = useState<string[]>([])
  const [linked, setLinked]       = useState<LinkedGroup[]>([])

  const [categories, setCats]     = useState<Category[]>([])
  const [allGroups, setAllGroups] = useState<ModGroup[]>([])

  const [loading, setLoading] = useState(!isNew)
  const [saving, setSaving]   = useState(false)
  const [error, setError]     = useState<string | null>(null)

  // Reference data (categories + modifier groups)
  useEffect(() => {
    supabase.from('menu_categories').select('id,parent_id,name').order('display_order')
      .then(({ data }) => setCats(data ?? []))
    supabase.from('menu_modifier_groups').select('id,name,min_select,max_select,is_required').order('name')
      .then(({ data }) => setAllGroups(data ?? []))
  }, [])

  // Load product in edit mode
  useEffect(() => {
    if (isNew) return
    setLoading(true)
    Promise.all([
      supabase.from('menu_products').select('*').eq('id', id).single(),
      supabase.from('menu_variants').select('*').eq('product_id', id).order('display_order'),
      supabase.from('menu_product_modifier_groups')
        .select('id,display_order,group_id,menu_modifier_groups(id,name,min_select,max_select,is_required)')
        .eq('product_id', id)
        .order('display_order'),
    ]).then(([{ data: p, error: e1 }, { data: v }, { data: g }]) => {
      if (e1 || !p) { setError('Produit introuvable'); setLoading(false); return }
      setProduct({
        category_id:      p.category_id ?? '',
        sku:              p.sku ?? '',
        name:             p.name,
        description:      p.description ?? '',
        tva_rate:         p.tva_rate,
        price:            toEuros(p.base_price_cents),
        image_url:        p.image_url ?? '',
        is_active:        p.is_active,
        visible_caisse:   p.visible_caisse,
        visible_kiosk:    p.visible_kiosk,
        visible_delivery: p.visible_delivery,
        visible_drive:    p.visible_drive,
        visible_digital:  p.visible_digital,
      })
      setVariants((v ?? []).map(row => ({
        _k:           row.id,
        id:           row.id,
        name:         row.name,
        sku:          row.sku ?? '',
        price:        row.price_override_cents !== null ? toEuros(row.price_override_cents) : '',
        display_order: row.display_order,
        is_active:    row.is_active,
      })))
      setLinked((g ?? []).map((row: any) => ({
        _k:           row.id,
        link_id:      row.id,
        group_id:     row.group_id,
        group_name:   row.menu_modifier_groups.name,
        min_select:   row.menu_modifier_groups.min_select,
        max_select:   row.menu_modifier_groups.max_select,
        is_required:  row.menu_modifier_groups.is_required,
        display_order: row.display_order,
      })))
      setLoading(false)
    })
  }, [id, isNew])

  // ---- Variant helpers ----

  function addVariant() {
    setVariants(prev => [...prev, {
      _k: `new-${Date.now()}`, name: '', sku: '', price: '', display_order: prev.length, is_active: true,
    }])
  }

  function removeVariant(k: string) {
    const v = variants.find(x => x._k === k)
    if (v?.id) setDeleted(prev => [...prev, v.id!])
    setVariants(prev => prev.filter(x => x._k !== k))
  }

  function updateVariant(k: string, patch: Partial<VariantDraft>) {
    setVariants(prev => prev.map(v => v._k === k ? { ...v, ...patch } : v))
  }

  // ---- Modifier group helpers ----

  function addGroup(groupId: string) {
    const g = allGroups.find(x => x.id === groupId)
    if (!g || linked.some(x => x.group_id === groupId)) return
    setLinked(prev => [...prev, {
      _k: `new-${Date.now()}`,
      group_id: g.id, group_name: g.name,
      min_select: g.min_select, max_select: g.max_select, is_required: g.is_required,
      display_order: prev.length,
    }])
  }

  // ---- Save ----

  async function handleSave() {
    if (!product.name.trim()) { setError('Le nom est obligatoire'); return }
    setSaving(true)
    setError(null)

    const payload = {
      category_id:      product.category_id || null,
      sku:              product.sku.trim() || `PRD-${Date.now()}`,
      name:             product.name,
      description:      product.description || null,
      tva_rate:         product.tva_rate,
      base_price_cents: toCents(product.price),
      image_url:        product.image_url || null,
      is_active:        product.is_active,
      visible_caisse:   product.visible_caisse,
      visible_kiosk:    product.visible_kiosk,
      visible_delivery: product.visible_delivery,
      visible_drive:    product.visible_drive,
      visible_digital:  product.visible_digital,
    }

    let productId = id

    if (isNew) {
      const { data, error } = await supabase.from('menu_products').insert(payload).select('id').single()
      if (error || !data) { setError(error?.message ?? 'Erreur'); setSaving(false); return }
      productId = data.id
    } else {
      const { error } = await supabase.from('menu_products').update(payload).eq('id', id!)
      if (error) { setError(error.message); setSaving(false); return }
    }

    // Delete removed variants
    if (deletedIds.length > 0)
      await supabase.from('menu_variants').delete().in('id', deletedIds)

    // Upsert variants
    for (const [i, v] of variants.entries()) {
      const vp = {
        product_id:           productId!,
        name:                 v.name,
        sku:                  v.sku || null,
        price_override_cents: v.price ? toCents(v.price) : null,
        display_order:        i,
        is_active:            v.is_active,
      }
      if (v.id) await supabase.from('menu_variants').update(vp).eq('id', v.id)
      else      await supabase.from('menu_variants').insert(vp)
    }

    // Replace modifier group links
    await supabase.from('menu_product_modifier_groups').delete().eq('product_id', productId!)
    if (linked.length > 0)
      await supabase.from('menu_product_modifier_groups').insert(
        linked.map((g, idx) => ({ product_id: productId!, group_id: g.group_id, display_order: idx }))
      )

    setSaving(false)
    navigate('/products')
  }

  // ---- Render ----

  if (loading) return <p style={{ color: '#888', padding: '2rem' }}>Chargement…</p>

  const parents      = categories.filter(c => !c.parent_id)
  const childOf      = (pid: string) => categories.filter(c => c.parent_id === pid)
  const availGroups  = allGroups.filter(g => !linked.some(l => l.group_id === g.id))

  return (
    <div style={{ maxWidth: 860 }}>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '1rem', marginBottom: '1.5rem' }}>
        <Link to="/products" style={{ color: '#4f8ef7', textDecoration: 'none', fontSize: '0.85rem' }}>← Produits</Link>
        <h1 style={{ margin: 0, fontSize: '1.25rem', flex: 1 }}>
          {isNew ? 'Nouveau produit' : product.name}
        </h1>
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
            <input value={product.name} onChange={e => setProduct(p => ({ ...p, name: e.target.value }))} style={inputStyle} />
          </Field>
          <Field label="Statut">
            <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginTop: '0.3rem', cursor: 'pointer', fontSize: '0.875rem' }}>
              <input type="checkbox" checked={product.is_active} onChange={e => setProduct(p => ({ ...p, is_active: e.target.checked }))} />
              <span style={{ color: product.is_active ? '#2d6a4f' : '#c0392b', fontWeight: 500 }}>
                {product.is_active ? 'Actif' : 'Inactif'}
              </span>
            </label>
          </Field>
        </div>
        <div style={{ marginBottom: '1rem' }}>
          <Field label="Description">
            <textarea value={product.description} onChange={e => setProduct(p => ({ ...p, description: e.target.value }))}
              rows={2} style={{ ...inputStyle, resize: 'vertical' }} />
          </Field>
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr 1fr', gap: '1rem', marginBottom: '1rem' }}>
          <Field label="Catégorie">
            <select value={product.category_id} onChange={e => setProduct(p => ({ ...p, category_id: e.target.value }))} style={inputStyle}>
              <option value="">— Aucune —</option>
              {parents.flatMap(cat => [
                <option key={cat.id} value={cat.id}>{cat.name}</option>,
                ...childOf(cat.id).map(c => <option key={c.id} value={c.id}>&nbsp;&nbsp;↳ {c.name}</option>),
              ])}
            </select>
          </Field>
          <Field label="TVA">
            <select value={product.tva_rate} onChange={e => setProduct(p => ({ ...p, tva_rate: e.target.value as TvaRate }))} style={inputStyle}>
              {Object.entries(TVA_LABELS).map(([k, v]) => <option key={k} value={k}>{v}</option>)}
            </select>
          </Field>
          <Field label="Prix de base (€)">
            <input type="number" step="0.01" min="0" value={product.price}
              onChange={e => setProduct(p => ({ ...p, price: e.target.value }))} style={inputStyle} />
          </Field>
          <Field label="SKU">
            <input value={product.sku} onChange={e => setProduct(p => ({ ...p, sku: e.target.value }))}
              placeholder="Auto si vide" style={inputStyle} />
          </Field>
        </div>
        <Field label="URL image">
          <input value={product.image_url} onChange={e => setProduct(p => ({ ...p, image_url: e.target.value }))}
            placeholder="https://…" style={inputStyle} />
        </Field>
      </Card>

      {/* Destinations */}
      <Card title="Destinations">
        <div style={{ display: 'flex', gap: '1.5rem', flexWrap: 'wrap' }}>
          {DESTINATIONS.map(({ key, label }) => (
            <label key={key} style={{ display: 'flex', alignItems: 'center', gap: '0.4rem', cursor: 'pointer', fontSize: '0.875rem' }}>
              <input type="checkbox" checked={product[key]} onChange={e => setProduct(p => ({ ...p, [key]: e.target.checked }))} />
              {label}
            </label>
          ))}
        </div>
      </Card>

      {/* Variantes */}
      <Card title={`Variantes (${variants.length})`} action={
        <button onClick={addVariant}
          style={{ padding: '0.3rem 0.8rem', borderRadius: 6, border: 'none', background: '#4f8ef7', color: '#fff', fontSize: '0.8rem', fontWeight: 600, cursor: 'pointer' }}>
          + Ajouter
        </button>
      }>
        {variants.length === 0 ? (
          <p style={{ color: '#aaa', fontSize: '0.85rem', margin: 0 }}>
            Aucune variante — le produit est vendu tel quel au prix de base.
          </p>
        ) : (
          <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.85rem' }}>
            <thead>
              <tr>
                {['Nom *', 'SKU', `Prix (€) — hérite si vide (${product.price} €)`, 'Actif', ''].map(h => (
                  <th key={h} style={{ padding: '0.4rem 0.5rem', fontWeight: 600, fontSize: '0.73rem', color: '#888', textAlign: 'left', textTransform: 'uppercase', letterSpacing: '0.04em' }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {variants.map((v) => (
                <tr key={v._k} style={{ borderTop: '1px solid #f1f3f5' }}>
                  <td style={{ padding: '0.35rem 0.5rem' }}>
                    <input value={v.name} onChange={e => updateVariant(v._k, { name: e.target.value })}
                      placeholder={`ex: Seul, Menu, Large…`}
                      style={{ ...inputStyle, minWidth: 130 }} />
                  </td>
                  <td style={{ padding: '0.35rem 0.5rem' }}>
                    <input value={v.sku} onChange={e => updateVariant(v._k, { sku: e.target.value })}
                      style={{ ...inputStyle, width: 90 }} />
                  </td>
                  <td style={{ padding: '0.35rem 0.5rem' }}>
                    <input type="number" step="0.01" min="0" value={v.price}
                      onChange={e => updateVariant(v._k, { price: e.target.value })}
                      placeholder={`${product.price} (du produit)`}
                      style={{ ...inputStyle, width: 120 }} />
                  </td>
                  <td style={{ padding: '0.35rem 0.5rem', textAlign: 'center' }}>
                    <input type="checkbox" checked={v.is_active} onChange={e => updateVariant(v._k, { is_active: e.target.checked })} />
                  </td>
                  <td style={{ padding: '0.35rem 0.5rem' }}>
                    <button onClick={() => removeVariant(v._k)}
                      style={{ background: 'none', border: 'none', color: '#c0392b', cursor: 'pointer', fontSize: '1.1rem', lineHeight: 1 }}>×</button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Card>

      {/* Groupes de modificateurs */}
      <Card title="Groupes de modificateurs" action={
        availGroups.length > 0 ? (
          <select defaultValue="" onChange={e => { if (e.target.value) addGroup(e.target.value); (e.target as HTMLSelectElement).value = '' }}
            style={{ ...inputStyle, width: 'auto', fontSize: '0.8rem' }}>
            <option value="">+ Associer un groupe…</option>
            {availGroups.map(g => <option key={g.id} value={g.id}>{g.name}</option>)}
          </select>
        ) : undefined
      }>
        {linked.length === 0 ? (
          <p style={{ color: '#aaa', fontSize: '0.85rem', margin: 0 }}>
            {allGroups.length === 0
              ? <>Créez d'abord des groupes dans <Link to="/modifiers" style={{ color: '#4f8ef7' }}>Modificateurs</Link>.</>
              : 'Aucun groupe associé. Sélectionnez-en un ci-dessus.'}
          </p>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '0.4rem' }}>
            {linked.map((g) => (
              <div key={g._k} style={{ display: 'flex', alignItems: 'center', gap: '0.75rem', padding: '0.6rem 0.75rem', background: '#f8f9fa', borderRadius: 6, fontSize: '0.85rem' }}>
                <span style={{ color: '#888', fontFamily: 'monospace', fontSize: '0.75rem', minWidth: 16 }}>{linked.indexOf(g) + 1}</span>
                <span style={{ fontWeight: 500, flex: 1 }}>{g.group_name}</span>
                <span style={{ color: '#888', fontSize: '0.78rem' }}>
                  {g.is_required ? 'Obligatoire · ' : ''}{g.min_select}–{g.max_select} choix
                </span>
                <button onClick={() => setLinked(prev => prev.filter(x => x._k !== g._k))}
                  style={{ background: 'none', border: 'none', color: '#c0392b', cursor: 'pointer', fontSize: '1.1rem', lineHeight: 1 }}>×</button>
              </div>
            ))}
          </div>
        )}
      </Card>
    </div>
  )
}

// ---- Sub-components ----

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
