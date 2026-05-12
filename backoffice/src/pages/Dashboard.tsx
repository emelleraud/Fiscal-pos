import { useEffect, useState } from 'react'
import { supabase } from '../supabaseClient'

interface DailyRevenue {
  site_id: string
  site_code: string
  site_name: string
  day: string
  sale_count: number
  refund_count: number
  total_sales: number
  total_refunds: number
  net_revenue: number
}

const EUR = (cents: number) =>
  (cents / 100).toLocaleString('fr-FR', { style: 'currency', currency: 'EUR' })

export default function Dashboard() {
  const [rows, setRows]       = useState<DailyRevenue[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError]     = useState<string | null>(null)

  useEffect(() => {
    supabase
      .from('daily_revenue_by_site')
      .select('*')
      .order('day', { ascending: false })
      .limit(90)
      .then(({ data, error }) => {
        if (error) setError(error.message)
        else setRows(data ?? [])
        setLoading(false)
      })
  }, [])

  const totalNet = rows.reduce((acc, r) => acc + (r.net_revenue ?? 0), 0)

  if (loading) return <p style={{ color: '#888' }}>Chargement…</p>
  if (error)   return <p style={{ color: '#e53e3e' }}>Erreur : {error}</p>

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: '1rem', marginBottom: '1.5rem' }}>
        <h1 style={{ margin: 0, fontSize: '1.25rem' }}>Dashboard CA</h1>
        <span style={{ color: '#888', fontSize: '0.85rem' }}>90 derniers jours</span>
        <span style={{ marginLeft: 'auto', fontWeight: 600, fontSize: '1.1rem' }}>
          Total net : {EUR(totalNet)}
        </span>
      </div>

      {rows.length === 0 ? (
        <p style={{ color: '#888' }}>Aucune donnée disponible.</p>
      ) : (
        <table style={{ width: '100%', borderCollapse: 'collapse', background: '#fff', borderRadius: 8, overflow: 'hidden', boxShadow: '0 1px 3px rgba(0,0,0,0.08)' }}>
          <thead>
            <tr style={{ background: '#f1f3f5', textAlign: 'left' }}>
              {['Date', 'Site', 'Ventes', 'Remboursements', 'CA brut', 'Remboursé', 'CA net'].map(h => (
                <th key={h} style={{ padding: '0.65rem 1rem', fontWeight: 600, fontSize: '0.8rem', color: '#555', textTransform: 'uppercase', letterSpacing: '0.05em' }}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r, i) => (
              <tr key={i} style={{ borderTop: '1px solid #f1f3f5' }}>
                <td style={{ padding: '0.6rem 1rem', fontFamily: 'monospace', fontSize: '0.85rem' }}>{r.day}</td>
                <td style={{ padding: '0.6rem 1rem' }}>
                  <span style={{ fontWeight: 500 }}>{r.site_name}</span>
                  <span style={{ color: '#aaa', fontSize: '0.8rem', marginLeft: '0.4rem' }}>{r.site_code}</span>
                </td>
                <td style={{ padding: '0.6rem 1rem', color: '#2d6a4f' }}>{r.sale_count}</td>
                <td style={{ padding: '0.6rem 1rem', color: '#c0392b' }}>{r.refund_count}</td>
                <td style={{ padding: '0.6rem 1rem' }}>{EUR(r.total_sales)}</td>
                <td style={{ padding: '0.6rem 1rem', color: '#c0392b' }}>{EUR(r.total_refunds)}</td>
                <td style={{ padding: '0.6rem 1rem', fontWeight: 600 }}>{EUR(r.net_revenue)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  )
}
