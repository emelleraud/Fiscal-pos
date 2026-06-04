// backoffice/src/pages/admin/SiteList.tsx
import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { supabase } from '../../supabaseClient'
import { useAuth } from '../../context/AuthContext'

interface Site { id: string; site_code: string; name: string; address: string | null; siret: string | null }

export default function SiteList() {
  const [sites, setSites] = useState<Site[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()
  const { role } = useAuth()
  const canWrite = role === 'pos_admin'

  useEffect(() => {
    setLoading(true)
    supabase.from('sites').select('id,site_code,name,address,siret').order('site_code')
      .then(({ data, error: e }) => {
        if (e) setError(e.message)
        else setSites((data as Site[]) ?? [])
        setLoading(false)
      })
  }, [])

  if (loading) return <p style={{ color: '#888' }}>Chargement…</p>

  return (
    <div style={{ padding: '1.5rem' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0 }}>
          Restaurants{' '}
          <span style={{ color: '#888', fontWeight: 400, fontSize: '0.9rem' }}>{sites.length} site(s)</span>
        </h2>
        {canWrite && (
          <button
            onClick={() => navigate('/admin/sites/new')}
            style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}
          >
            + Nouveau restaurant
          </button>
        )}
      </div>
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}
      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['CODE', 'NOM', 'ADRESSE', 'SIRET', 'ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sites.map(s => (
            <tr key={s.id} style={{ borderBottom: '1px solid #f0f0f0' }}>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 600, fontFamily: 'monospace' }}>{s.site_code}</td>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500 }}>{s.name}</td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#666', fontSize: '0.85rem' }}>{s.address ?? '—'}</td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#888', fontSize: '0.8rem', fontFamily: 'monospace' }}>{s.siret ?? '—'}</td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                {canWrite && (
                  <Link to={`/admin/sites/${s.id}`} style={{ color: '#4f8ef7', textDecoration: 'none', marginRight: 12 }}>Éditer</Link>
                )}
                <Link to={`/admin/sites/${s.id}/config`} style={{ color: '#666', textDecoration: 'none' }}>Config</Link>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
