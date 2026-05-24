import { useEffect, useState } from 'react'
import { supabase } from '../supabaseClient'
import { useSite } from '../context/SiteContext'

interface FiscalEntry {
  id: string
  sequence: number
  operation_type: string
  amount: number
  tva_rate: string
  signed_at: string
  site_code: string
  site_name: string
  session_ref: string
  entry_hash: string
  order_ref: string | null
}

const PAGE_SIZE = 50

const EUR = (cents: number) =>
  (cents / 100).toLocaleString('fr-FR', { style: 'currency', currency: 'EUR' })

const OP_COLORS: Record<string, string> = {
  SALE:       '#2d6a4f',
  REFUND:     '#c0392b',
  DISCOUNT:   '#e67e22',
  VOID:       '#7f8c8d',
  CORRECTION: '#8e44ad',
}

export default function FiscalJournal() {
  const { activeSiteId } = useSite()
  const [rows, setRows]       = useState<FiscalEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError]     = useState<string | null>(null)
  const [page, setPage]       = useState(0)

  useEffect(() => {
    setPage(0)
  }, [activeSiteId])

  useEffect(() => {
    if (!activeSiteId) return
    setLoading(true)
    supabase
      .from('fiscal_entries_enriched')
      .select('id,sequence,operation_type,amount,tva_rate,signed_at,site_code,site_name,session_ref,entry_hash,order_ref')
      .eq('site_id', activeSiteId)
      .order('sequence', { ascending: false })
      .range(page * PAGE_SIZE, (page + 1) * PAGE_SIZE - 1)
      .then(({ data, error }) => {
        if (error) setError(error.message)
        else setRows(data ?? [])
        setLoading(false)
      })
  }, [activeSiteId, page])

  if (error) return <p style={{ color: '#e53e3e' }}>Erreur : {error}</p>

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: '1rem', marginBottom: '1.5rem' }}>
        <h1 style={{ margin: 0, fontSize: '1.25rem' }}>Journal fiscal</h1>
        <span style={{ color: '#888', fontSize: '0.85rem' }}>Page {page + 1} · {PAGE_SIZE} lignes</span>
      </div>

      <table style={{ width: '100%', borderCollapse: 'collapse', background: '#fff', borderRadius: 8, overflow: 'hidden', boxShadow: '0 1px 3px rgba(0,0,0,0.08)', fontSize: '0.82rem' }}>
        <thead>
          <tr style={{ background: '#f1f3f5', textAlign: 'left' }}>
            {['#', 'Type', 'Montant', 'TVA', 'Site', 'Session', 'Date', 'Hash'].map(h => (
              <th key={h} style={{ padding: '0.65rem 0.75rem', fontWeight: 600, fontSize: '0.75rem', color: '#555', textTransform: 'uppercase', letterSpacing: '0.05em' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {loading ? (
            <tr><td colSpan={8} style={{ padding: '2rem', textAlign: 'center', color: '#888' }}>Chargement…</td></tr>
          ) : rows.map(r => (
            <tr key={r.id} style={{ borderTop: '1px solid #f1f3f5' }}>
              <td style={{ padding: '0.5rem 0.75rem', fontFamily: 'monospace', color: '#888' }}>{r.sequence}</td>
              <td style={{ padding: '0.5rem 0.75rem' }}>
                <span style={{
                  padding: '2px 7px', borderRadius: 4, fontSize: '0.75rem', fontWeight: 600,
                  background: `${OP_COLORS[r.operation_type] ?? '#555'}22`,
                  color: OP_COLORS[r.operation_type] ?? '#555',
                }}>
                  {r.operation_type}
                </span>
              </td>
              <td style={{ padding: '0.5rem 0.75rem', fontWeight: 500, color: r.amount < 0 ? '#c0392b' : '#2d6a4f' }}>
                {EUR(r.amount)}
              </td>
              <td style={{ padding: '0.5rem 0.75rem', color: '#888' }}>{r.tva_rate} %</td>
              <td style={{ padding: '0.5rem 0.75rem' }}>{r.site_code}</td>
              <td style={{ padding: '0.5rem 0.75rem', color: '#888', fontFamily: 'monospace', fontSize: '0.75rem' }}>
                {r.session_ref ?? '—'}
              </td>
              <td style={{ padding: '0.5rem 0.75rem', color: '#888', fontSize: '0.75rem' }}>
                {new Date(r.signed_at).toLocaleString('fr-FR')}
              </td>
              <td style={{ padding: '0.5rem 0.75rem', fontFamily: 'monospace', fontSize: '0.7rem', color: '#aaa' }}>
                {r.entry_hash?.slice(0, 10)}…
              </td>
            </tr>
          ))}
        </tbody>
      </table>

      <div style={{ marginTop: '1rem', display: 'flex', gap: '0.75rem', alignItems: 'center' }}>
        <button onClick={() => setPage(p => Math.max(0, p - 1))} disabled={page === 0}
          style={{ padding: '0.4rem 0.9rem', borderRadius: 6, border: '1px solid #ddd', background: '#fff', cursor: page === 0 ? 'not-allowed' : 'pointer', color: page === 0 ? '#ccc' : '#333' }}>
          ← Précédent
        </button>
        <span style={{ color: '#888', fontSize: '0.85rem' }}>Page {page + 1}</span>
        <button onClick={() => setPage(p => p + 1)} disabled={rows.length < PAGE_SIZE}
          style={{ padding: '0.4rem 0.9rem', borderRadius: 6, border: '1px solid #ddd', background: '#fff', cursor: rows.length < PAGE_SIZE ? 'not-allowed' : 'pointer', color: rows.length < PAGE_SIZE ? '#ccc' : '#333' }}>
          Suivant →
        </button>
      </div>
    </div>
  )
}
