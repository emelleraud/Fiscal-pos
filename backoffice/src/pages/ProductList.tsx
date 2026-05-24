import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { supabase } from '../supabaseClient'

interface Product {
  id: string
  sku: string | null
  name: string
  tva_rate: string
  base_price_cents: number
  is_active: boolean
  category_id: string | null
  menu_categories: { name: string } | null
  menu_variants: { id: string }[]
}

const EUR = (cents: number) =>
  (cents / 100).toLocaleString('fr-FR', { style: 'currency', currency: 'EUR' })

const TVA: Record<string, string> = {
  reduit5_5: '5,5 %', intermediaire10: '10 %', normal20: '20 %',
}

export default function ProductList() {
  const [products, setProducts] = useState<Product[]>([])
  const [loading, setLoading]   = useState(true)
  const [error, setError]       = useState<string | null>(null)
  const [search, setSearch]     = useState('')
  const [catFilter, setCat]     = useState('')
  const [categories, setCategories] = useState<{ id: string; name: string }[]>([])

  async function load() {
    setLoading(true)
    const [{ data, error }, { data: cats }] = await Promise.all([
      supabase
        .from('menu_products')
        .select('id,sku,name,tva_rate,base_price_cents,is_active,category_id,menu_categories(name),menu_variants(id)')
        .order('name'),
      supabase.from('menu_categories').select('id,name').order('name'),
    ])
    if (error) setError(error.message)
    else setProducts((data ?? []) as unknown as Product[])
    setCategories(cats ?? [])
    setLoading(false)
  }

  useEffect(() => { load() }, [])

  async function handleDelete(id: string, name: string) {
    if (!confirm(`Supprimer « ${name} » et toutes ses variantes ?`)) return
    const { error } = await supabase.from('menu_products').delete().eq('id', id)
    if (error) setError(error.message)
    else await load()
  }

  const filtered = products.filter(p => {
    const matchSearch = !search || p.name.toLowerCase().includes(search.toLowerCase()) || (p.sku ?? '').toLowerCase().includes(search.toLowerCase())
    const matchCat = !catFilter || p.category_id === catFilter
    return matchSearch && matchCat
  })

  if (loading) return <p style={{ color: '#888' }}>Chargement…</p>

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', gap: '1rem', marginBottom: '1.5rem', flexWrap: 'wrap' }}>
        <h1 style={{ margin: 0, fontSize: '1.25rem' }}>Produits</h1>
        <span style={{ color: '#888', fontSize: '0.85rem' }}>{filtered.length} / {products.length}</span>
        <input
          placeholder="Rechercher nom, SKU…"
          value={search}
          onChange={e => setSearch(e.target.value)}
          style={{ padding: '0.45rem 0.75rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.85rem', width: 220 }}
        />
        <select value={catFilter} onChange={e => setCat(e.target.value)}
          style={{ padding: '0.45rem 0.75rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.85rem' }}>
          <option value="">Toutes catégories</option>
          {categories.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
        </select>
        <Link to="/products/new"
          style={{ marginLeft: 'auto', padding: '0.45rem 1rem', borderRadius: 6, background: '#4f8ef7', color: '#fff', fontWeight: 600, fontSize: '0.85rem', textDecoration: 'none' }}>
          + Nouveau produit
        </Link>
      </div>

      {error && <p style={{ color: '#e53e3e', marginBottom: '1rem' }}>{error}</p>}

      <div style={{ background: '#fff', borderRadius: 8, boxShadow: '0 1px 3px rgba(0,0,0,0.08)', overflow: 'hidden' }}>
        <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.85rem' }}>
          <thead>
            <tr style={{ background: '#f1f3f5', textAlign: 'left' }}>
              {['Nom', 'SKU', 'Catégorie', 'TVA', 'Prix base', 'Variantes', 'Statut', 'Actions'].map(h => (
                <th key={h} style={{ padding: '0.65rem 1rem', fontWeight: 600, fontSize: '0.75rem', color: '#555', textTransform: 'uppercase', letterSpacing: '0.05em' }}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {filtered.length === 0 && (
              <tr><td colSpan={8} style={{ padding: '2rem', textAlign: 'center', color: '#888' }}>
                {products.length === 0 ? 'Aucun produit. Cliquez sur « + Nouveau produit ».' : 'Aucun résultat.'}
              </td></tr>
            )}
            {filtered.map(p => (
              <tr key={p.id} style={{ borderTop: '1px solid #f1f3f5' }}>
                <td style={{ padding: '0.6rem 1rem', fontWeight: 500 }}>
                  <Link to={`/products/${p.id}`} style={{ color: '#4f8ef7', textDecoration: 'none' }}>{p.name}</Link>
                </td>
                <td style={{ padding: '0.6rem 1rem', color: '#888', fontFamily: 'monospace', fontSize: '0.8rem' }}>{p.sku ?? '—'}</td>
                <td style={{ padding: '0.6rem 1rem', color: '#888' }}>{p.menu_categories?.name ?? '—'}</td>
                <td style={{ padding: '0.6rem 1rem', color: '#888' }}>{TVA[p.tva_rate] ?? p.tva_rate}</td>
                <td style={{ padding: '0.6rem 1rem', fontFamily: 'monospace' }}>{EUR(p.base_price_cents)}</td>
                <td style={{ padding: '0.6rem 1rem', color: '#888' }}>
                  {p.menu_variants.length > 0
                    ? <span style={{ fontWeight: 500, color: '#4f8ef7' }}>{p.menu_variants.length}</span>
                    : <span style={{ color: '#ccc' }}>—</span>}
                </td>
                <td style={{ padding: '0.6rem 1rem' }}>
                  <span style={{ fontSize: '0.75rem', fontWeight: 600, padding: '2px 7px', borderRadius: 4,
                    background: p.is_active ? '#d4edda' : '#f8d7da',
                    color: p.is_active ? '#2d6a4f' : '#c0392b' }}>
                    {p.is_active ? 'Actif' : 'Inactif'}
                  </span>
                </td>
                <td style={{ padding: '0.6rem 1rem' }}>
                  <div style={{ display: 'flex', gap: '0.4rem' }}>
                    <Link to={`/products/${p.id}`}
                      style={{ padding: '0.3rem 0.7rem', borderRadius: 5, border: '1px solid #ddd', background: '#fff', fontSize: '0.8rem', textDecoration: 'none', color: '#333' }}>
                      Éditer
                    </Link>
                    <button onClick={() => handleDelete(p.id, p.name)}
                      style={{ padding: '0.3rem 0.7rem', borderRadius: 5, border: '1px solid #ffc9c9', background: '#fff', fontSize: '0.8rem', cursor: 'pointer', color: '#c0392b' }}>
                      Suppr.
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}
