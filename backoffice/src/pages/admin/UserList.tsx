// backoffice/src/pages/admin/UserList.tsx
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { supabase } from '../../supabaseClient'

interface AdminUser {
  id: string
  email: string
  role: string | null
  site_id: string | null
  display_name: string | null
  is_banned: boolean
  created_at: string
}

export default function UserList() {
  const [users, setUsers] = useState<AdminUser[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [revoking, setRevoking] = useState<string | null>(null)
  const navigate = useNavigate()

  const load = () => {
    setLoading(true)
    supabase.rpc('list_admin_users')
      .then(({ data, error: e }) => {
        if (e) setError(e.message)
        else setUsers((data as AdminUser[]) ?? [])
        setLoading(false)
      })
  }

  useEffect(() => { load() }, [])

  const handleRevoke = async (userId: string) => {
    if (!confirm('Révoquer cet utilisateur ? Il ne pourra plus se connecter.')) return
    setRevoking(userId)
    const { error: e } = await supabase.functions.invoke('user-admin', {
      body: { action: 'revoke', payload: { user_id: userId } },
    })
    if (e) { alert('Erreur : ' + e.message) }
    else { load() }
    setRevoking(null)
  }

  if (loading) return <p style={{ color: '#888' }}>Chargement…</p>

  return (
    <div style={{ padding: '1.5rem' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0 }}>
          Utilisateurs{' '}
          <span style={{ color: '#888', fontWeight: 400, fontSize: '0.9rem' }}>{users.length} user(s)</span>
        </h2>
        <button
          onClick={() => navigate('/admin/users/new')}
          style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}
        >
          + Ajouter
        </button>
      </div>
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}
      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['NOM', 'EMAIL', 'RÔLE', 'SITE', 'STATUT', 'ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {users.map(u => (
            <tr key={u.id} style={{ borderBottom: '1px solid #f0f0f0', opacity: u.is_banned ? 0.5 : 1 }}>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500 }}>{u.display_name ?? '—'}</td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#666', fontSize: '0.85rem' }}>{u.email}</td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                <span style={{ background: '#e8f0fe', color: '#1a56db', borderRadius: 4, padding: '2px 6px', fontSize: '0.78rem' }}>
                  {u.role ?? '—'}
                </span>
              </td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#888', fontSize: '0.8rem', fontFamily: 'monospace' }}>{u.site_id ?? '—'}</td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                {u.is_banned
                  ? <span style={{ color: '#e53e3e', fontSize: '0.8rem' }}>Révoqué</span>
                  : <span style={{ color: '#48bb78', fontSize: '0.8rem' }}>Actif</span>}
              </td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                <button
                  onClick={() => navigate(`/admin/users/${u.id}`, { state: { user: u } })}
                  style={{ color: '#4f8ef7', background: 'none', border: 'none', cursor: 'pointer', marginRight: 8, fontSize: '0.85rem' }}
                >
                  Éditer
                </button>
                {!u.is_banned && (
                  <button
                    onClick={() => handleRevoke(u.id)}
                    disabled={revoking === u.id}
                    style={{ color: '#e53e3e', background: 'none', border: 'none', cursor: 'pointer', fontSize: '0.85rem' }}
                  >
                    {revoking === u.id ? '…' : 'Révoquer'}
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
