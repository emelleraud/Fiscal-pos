import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { supabase } from '../supabaseClient'

interface Group { id: string; name: string; group_type: string; created_at: string }

export default function GroupList() {
  const [groups, setGroups] = useState<Group[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()

  useEffect(() => {
    setLoading(true)
    supabase.from('restaurant_groups').select('*').order('name')
      .then(({ data, error: e }) => {
        if (e) setError(e.message)
        else setGroups(data ?? [])
        setLoading(false)
      })
  }, [])

  if (loading) return <p style={{ color: '#888' }}>Chargement…</p>

  return (
    <div style={{ padding: '1.5rem' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0 }}>Groupes de restaurants <span style={{ color: '#888', fontWeight: 400, fontSize: '0.9rem' }}>{groups.length} groupe(s)</span></h2>
        <button
          onClick={() => navigate('/groups/new')}
          style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}>
          + Nouveau groupe
        </button>
      </div>
      {error && <p style={{ color: '#e53e3e', marginBottom: '1rem' }}>{error}</p>}
      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['NOM', 'TYPE', 'DATE CRÉATION', 'ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {groups.map(g => (
            <tr key={g.id} style={{ borderBottom: '1px solid #f0f0f0' }}>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500 }}>{g.name}</td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#666', textTransform: 'capitalize' }}>{g.group_type}</td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#888', fontSize: '0.85rem' }}>{g.created_at?.slice(0, 10)}</td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                <Link to={`/groups/${g.id}`} style={{ color: '#4f8ef7', textDecoration: 'none', marginRight: 8 }}>Éditer</Link>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
