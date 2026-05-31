import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { supabase } from '../supabaseClient'
import { useRole } from '../hooks/useRole'

interface Promo {
  id: string; name: string; scope: string; promo_type: string
  trigger: string; status: string; valid_from: string | null
  valid_to: string | null; active: boolean
}

const STATUS_BADGE: Record<string, { bg: string; label: string }> = {
  draft:            { bg: '#f0f0f0', label: 'Brouillon' },
  pending_approval: { bg: '#fff3cd', label: 'En attente' },
  approved:         { bg: '#d4edda', label: 'Approuvée' },
  active:           { bg: '#c3e6cb', label: 'Active' },
  rejected:         { bg: '#f8d7da', label: 'Rejetée' },
}

export default function PromotionList() {
  const { hasRole } = useRole()
  const navigate = useNavigate()
  const [promos, setPromos] = useState<Promo[]>([])
  const [tab, setTab] = useState<'all' | 'pending'>('all')
  const [rejectId, setRejectId] = useState<string | null>(null)
  const [rejectReason, setRejectReason] = useState('')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const load = async () => {
      setLoading(true)
      try {
        const q = supabase.from('promotions').select('id,name,scope,promo_type,trigger,status,valid_from,valid_to,active')
        const { data, error: e } = tab === 'pending'
          ? await q.eq('status', 'pending_approval')
          : await q.order('created_at', { ascending: false })
        setError(e?.message ?? null)
        setPromos(data ?? [])
      } finally {
        setLoading(false)
      }
    }
    load()
  }, [tab])

  const approve = async (id: string) => {
    const { error: e } = await supabase
      .from('promotions')
      .update({ status: 'approved', approved_at: new Date().toISOString() })
      .eq('id', id)
    if (e) { setError(e.message); return }
    setLoading(true)
    try {
      const q = supabase.from('promotions').select('id,name,scope,promo_type,trigger,status,valid_from,valid_to,active')
      const { data, error: le } = tab === 'pending'
        ? await q.eq('status', 'pending_approval')
        : await q.order('created_at', { ascending: false })
      setError(le?.message ?? null)
      setPromos(data ?? [])
    } finally {
      setLoading(false)
    }
  }

  const reject = async () => {
    if (!rejectId || !rejectReason.trim()) return
    const { error: e } = await supabase
      .from('promotions')
      .update({ status: 'rejected', rejection_reason: rejectReason })
      .eq('id', rejectId)
    if (e) { setError(e.message); return }
    setRejectId(null); setRejectReason('')
    setLoading(true)
    try {
      const q = supabase.from('promotions').select('id,name,scope,promo_type,trigger,status,valid_from,valid_to,active')
      const { data, error: le } = tab === 'pending'
        ? await q.eq('status', 'pending_approval')
        : await q.order('created_at', { ascending: false })
      setError(le?.message ?? null)
      setPromos(data ?? [])
    } finally {
      setLoading(false)
    }
  }

  const toggleActive = async (p: Promo) => {
    if (p.status !== 'approved' && !p.active) return
    const { error: e } = await supabase
      .from('promotions')
      .update({ active: !p.active })
      .eq('id', p.id)
    if (e) { setError(e.message); return }
    setLoading(true)
    try {
      const q = supabase.from('promotions').select('id,name,scope,promo_type,trigger,status,valid_from,valid_to,active')
      const { data, error: le } = tab === 'pending'
        ? await q.eq('status', 'pending_approval')
        : await q.order('created_at', { ascending: false })
      setError(le?.message ?? null)
      setPromos(data ?? [])
    } finally {
      setLoading(false)
    }
  }

  const tabStyle = (t: string) => ({
    padding: '0.4rem 1rem', border: 'none', cursor: 'pointer', fontWeight: 600,
    borderBottom: tab === t ? '2px solid #4f8ef7' : '2px solid transparent',
    background: 'none', color: tab === t ? '#4f8ef7' : '#888',
  })

  return (
    <div style={{ padding: '1.5rem' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0 }}>Promotions</h2>
        <button
          onClick={() => navigate('/promotions/new')}
          style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}
        >
          + Nouvelle promo
        </button>
      </div>

      {hasRole('director') && (
        <div style={{ borderBottom: '1px solid #eee', marginBottom: '1rem' }}>
          <button style={tabStyle('all')} onClick={() => setTab('all')}>Toutes</button>
          <button style={tabStyle('pending')} onClick={() => setTab('pending')}>En attente d'approbation</button>
        </div>
      )}

      {loading && <p>Chargement…</p>}
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}

      {/* Modal rejet */}
      {rejectId && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.4)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 100 }}>
          <div style={{ background: '#fff', borderRadius: 12, padding: '1.5rem', width: 400 }}>
            <h3 style={{ margin: '0 0 1rem' }}>Motif de rejet</h3>
            <textarea value={rejectReason} onChange={e => setRejectReason(e.target.value)}
              style={{ width: '100%', border: '1px solid #ddd', borderRadius: 6, padding: '0.5rem', minHeight: 80, boxSizing: 'border-box' }} />
            <div style={{ display: 'flex', gap: 8, marginTop: '0.75rem' }}>
              <button onClick={reject} disabled={!rejectReason.trim()}
                style={{ background: '#e53e3e', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer' }}>Rejeter</button>
              <button onClick={() => { setRejectId(null); setRejectReason('') }} style={{ border: '1px solid #ddd', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', background: '#fff' }}>Annuler</button>
            </div>
          </div>
        </div>
      )}

      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['NOM','TYPE','SCOPE','DÉCLENCHEUR','VALIDITÉ','STATUT','ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {promos.length === 0 && !loading ? (
            <tr>
              <td colSpan={7} style={{ textAlign: 'center', color: '#888', padding: '1.5rem' }}>Aucune promotion.</td>
            </tr>
          ) : (
            promos.map(p => {
              const badge = STATUS_BADGE[p.status] ?? { bg: '#eee', label: p.status }
              return (
                <tr key={p.id} style={{ borderBottom: '1px solid #f0f0f0' }}>
                  <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500 }}>{p.name}</td>
                  <td style={{ padding: '0.7rem 0.8rem', color: '#666' }}>{p.promo_type}</td>
                  <td style={{ padding: '0.7rem 0.8rem', color: '#666', textTransform: 'capitalize' }}>{p.scope}</td>
                  <td style={{ padding: '0.7rem 0.8rem', color: '#666' }}>{p.trigger}</td>
                  <td style={{ padding: '0.7rem 0.8rem', color: '#888', fontSize: '0.8rem' }}>
                    {p.valid_from ?? '—'} → {p.valid_to ?? '∞'}
                  </td>
                  <td style={{ padding: '0.7rem 0.8rem' }}>
                    <span style={{ background: badge.bg, padding: '0.2rem 0.6rem', borderRadius: 12, fontSize: '0.78rem' }}>{badge.label}</span>
                  </td>
                  <td style={{ padding: '0.7rem 0.8rem', whiteSpace: 'nowrap' }}>
                    <Link to={`/promotions/${p.id}`} style={{ color: '#4f8ef7', textDecoration: 'none', marginRight: 8 }}>Éditer</Link>
                    {p.status === 'pending_approval' && hasRole('director') && (
                      <>
                        <button onClick={() => approve(p.id)} style={{ color: '#38a169', border: 'none', background: 'none', cursor: 'pointer', marginRight: 4 }}>✓ Approuver</button>
                        <button onClick={() => setRejectId(p.id)} style={{ color: '#e53e3e', border: 'none', background: 'none', cursor: 'pointer' }}>✗ Rejeter</button>
                      </>
                    )}
                    {p.status === 'approved' && (
                      <button onClick={() => toggleActive(p)} style={{ color: p.active ? '#e53e3e' : '#38a169', border: 'none', background: 'none', cursor: 'pointer' }}>
                        {p.active ? 'Désactiver' : 'Activer'}
                      </button>
                    )}
                  </td>
                </tr>
              )
            })
          )}
        </tbody>
      </table>
    </div>
  )
}
