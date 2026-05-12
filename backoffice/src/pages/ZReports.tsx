import { useEffect, useState } from 'react'
import { supabase } from '../supabaseClient'

interface ZReport {
  id: string
  report_number: number
  generated_at: string
  total_sales: number
  total_refunds: number
  net_revenue: number
  entry_count: number
  tva_5_5_amount: number
  tva_10_amount: number
  tva_20_amount: number
  report_hash: string | null
}

const EUR = (cents: number) =>
  (cents / 100).toLocaleString('fr-FR', { style: 'currency', currency: 'EUR' })

export default function ZReports() {
  const [rows, setRows]       = useState<ZReport[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError]     = useState<string | null>(null)
  const [expanded, setExpanded] = useState<string | null>(null)

  useEffect(() => {
    supabase
      .from('z_reports')
      .select('id,report_number,generated_at,total_sales,total_refunds,net_revenue,entry_count,tva_5_5_amount,tva_10_amount,tva_20_amount,report_hash')
      .order('report_number', { ascending: false })
      .limit(100)
      .then(({ data, error }) => {
        if (error) setError(error.message)
        else setRows(data ?? [])
        setLoading(false)
      })
  }, [])

  if (loading) return <p style={{ color: '#888' }}>Chargement…</p>
  if (error)   return <p style={{ color: '#e53e3e' }}>Erreur : {error}</p>

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: '1rem', marginBottom: '1.5rem' }}>
        <h1 style={{ margin: 0, fontSize: '1.25rem' }}>Rapports Z</h1>
        <span style={{ color: '#888', fontSize: '0.85rem' }}>{rows.length} rapport(s)</span>
      </div>

      {rows.length === 0 ? (
        <p style={{ color: '#888' }}>Aucun rapport Z disponible.</p>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem' }}>
          {rows.map(r => (
            <div key={r.id} style={{ background: '#fff', borderRadius: 8, boxShadow: '0 1px 3px rgba(0,0,0,0.08)', overflow: 'hidden' }}>
              {/* Ligne principale — cliquable */}
              <div
                onClick={() => setExpanded(expanded === r.id ? null : r.id)}
                style={{ display: 'flex', alignItems: 'center', gap: '1.5rem', padding: '0.85rem 1.25rem', cursor: 'pointer', userSelect: 'none' }}
              >
                <span style={{ fontFamily: 'monospace', fontWeight: 700, fontSize: '0.9rem', color: '#4f8ef7', minWidth: 40 }}>
                  Z{r.report_number}
                </span>
                <span style={{ color: '#555', fontSize: '0.85rem' }}>
                  {new Date(r.generated_at).toLocaleString('fr-FR')}
                </span>
                <span style={{ marginLeft: 'auto', fontWeight: 600 }}>{EUR(r.net_revenue)}</span>
                <span style={{ color: '#888', fontSize: '0.8rem' }}>{r.entry_count} entrées</span>
                <span style={{ color: '#aaa', fontSize: '0.75rem' }}>{expanded === r.id ? '▲' : '▼'}</span>
              </div>

              {/* Détail dépliable */}
              {expanded === r.id && (
                <div style={{ borderTop: '1px solid #f1f3f5', padding: '0.85rem 1.25rem', display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: '0.75rem', background: '#fafbfc', fontSize: '0.82rem' }}>
                  {[
                    { label: 'CA brut',     value: EUR(r.total_sales) },
                    { label: 'Remboursé',   value: EUR(r.total_refunds), red: true },
                    { label: 'TVA 5,5 %',   value: EUR(r.tva_5_5_amount) },
                    { label: 'TVA 10 %',    value: EUR(r.tva_10_amount) },
                    { label: 'TVA 20 %',    value: EUR(r.tva_20_amount) },
                    { label: 'Hash rapport', value: r.report_hash ? r.report_hash.slice(0, 16) + '…' : '—', mono: true },
                  ].map(({ label, value, red, mono }) => (
                    <div key={label}>
                      <div style={{ color: '#aaa', fontSize: '0.72rem', textTransform: 'uppercase', marginBottom: 2 }}>{label}</div>
                      <div style={{ fontWeight: 600, color: red ? '#c0392b' : '#222', fontFamily: mono ? 'monospace' : 'inherit' }}>{value}</div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
