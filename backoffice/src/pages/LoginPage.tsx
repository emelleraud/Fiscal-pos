import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { supabase } from '../supabaseClient'

export default function LoginPage() {
  const [email, setEmail]       = useState('')
  const [password, setPassword] = useState('')
  const [error, setError]       = useState<string | null>(null)
  const [loading, setLoading]   = useState(false)
  const navigate = useNavigate()

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    setLoading(true)
    setError(null)
    const { error } = await supabase.auth.signInWithPassword({ email, password })
    if (error) {
      setError(error.message)
      setLoading(false)
    } else {
      navigate('/dashboard', { replace: true })
    }
  }

  return (
    <div style={{ display: 'flex', height: '100vh', alignItems: 'center', justifyContent: 'center', background: '#f8f9fa' }}>
      <div style={{ background: '#fff', padding: '2rem', borderRadius: 12, boxShadow: '0 4px 24px rgba(0,0,0,0.08)', width: 340 }}>
        <h1 style={{ margin: '0 0 0.25rem', fontSize: '1.25rem' }}>POS Back-office</h1>
        <p style={{ margin: '0 0 1.5rem', color: '#888', fontSize: '0.85rem' }}>Connexion administrateur</p>
        <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: '0.75rem' }}>
          <input
            type="email"
            value={email}
            onChange={e => setEmail(e.target.value)}
            placeholder="Email"
            required
            autoComplete="email"
            style={{ padding: '0.6rem 0.8rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.9rem' }}
          />
          <input
            type="password"
            value={password}
            onChange={e => setPassword(e.target.value)}
            placeholder="Mot de passe"
            required
            autoComplete="current-password"
            style={{ padding: '0.6rem 0.8rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.9rem' }}
          />
          {error && <p style={{ color: '#e53e3e', fontSize: '0.85rem', margin: 0 }}>{error}</p>}
          <button
            type="submit"
            disabled={loading}
            style={{ padding: '0.65rem', borderRadius: 6, border: 'none', background: '#4f8ef7', color: '#fff', fontWeight: 600, fontSize: '0.9rem', cursor: loading ? 'not-allowed' : 'pointer', opacity: loading ? 0.7 : 1 }}
          >
            {loading ? 'Connexion…' : 'Se connecter'}
          </button>
        </form>
      </div>
    </div>
  )
}
