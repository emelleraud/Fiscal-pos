// backoffice/src/pages/admin/UserForm.tsx
import { useEffect, useState } from 'react'
import { useLocation, useNavigate, useParams } from 'react-router-dom'
import { supabase } from '../../supabaseClient'

const inputStyle = { padding: '0.5rem 0.75rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.9rem', width: '100%', boxSizing: 'border-box' as const }
const btnPrimary = { background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.6rem 1.2rem', cursor: 'pointer', fontWeight: 600 } as const
const btnSecondary = { background: '#f5f6fa', border: '1px solid #ddd', borderRadius: 6, padding: '0.6rem 1rem', cursor: 'pointer' } as const

function FieldLabel({ txt }: { txt: string }) {
  return <label style={{ display: 'block', fontWeight: 600, marginBottom: 4, fontSize: '0.85rem' }}>{txt}</label>
}

const ROLES = ['pos_admin', 'regional_director', 'director', 'manager', 'pos_caissier', 'pos_auditeur']
const SITE_ROLES = ['pos_caissier', 'manager']

interface Site { id: string; site_code: string; name: string }

interface AdminUser {
  id: string; email: string; role: string | null
  site_id: string | null; display_name: string | null
}

export default function UserForm() {
  const { id } = useParams<{ id: string }>()
  const { state } = useLocation()
  const navigate = useNavigate()
  const isEdit = id !== undefined
  const existing = (state as { user?: AdminUser } | null)?.user ?? null

  const [mode, setMode] = useState<'invite' | 'direct'>('invite')
  const [email, setEmail] = useState(existing?.email ?? '')
  const [displayName, setDisplayName] = useState(existing?.display_name ?? '')
  const [employeeId, setEmployeeId] = useState('')
  const [role, setRole] = useState(existing?.role ?? 'pos_caissier')
  const [siteId, setSiteId] = useState(existing?.site_id ?? '')
  const [sites, setSites] = useState<Site[]>([])
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [createdCreds, setCreatedCreds] = useState<{ email: string; temp_password: string } | null>(null)

  const needsSite = SITE_ROLES.includes(role)

  useEffect(() => {
    supabase.from('sites').select('id,site_code,name').order('site_code')
      .then(({ data }) => setSites((data as Site[]) ?? []))
  }, [])

  const handleSubmit = async () => {
    setSaving(true); setError(null)
    try {
      if (isEdit) {
        const payload: Record<string, string> = { user_id: id!, role }
        if (needsSite && siteId) payload.site_id = siteId
        const { error: e } = await supabase.functions.invoke('user-admin', {
          body: { action: 'update_role', payload },
        })
        if (e) throw e
        navigate('/admin/users')
        return
      }

      if (mode === 'invite') {
        const payload: Record<string, string> = { email, role, display_name: displayName }
        if (needsSite && siteId) payload.site_id = siteId
        const { error: e } = await supabase.functions.invoke('user-admin', {
          body: { action: 'invite', payload },
        })
        if (e) throw e
        navigate('/admin/users')
      } else {
        const payload: Record<string, string> = { display_name: displayName, employee_id: employeeId, role }
        if (needsSite && siteId) payload.site_id = siteId
        const { data, error: e } = await supabase.functions.invoke<{ email: string; temp_password: string }>('user-admin', {
          body: { action: 'create_direct', payload },
        })
        if (e) throw e
        setCreatedCreds({ email: data!.email, temp_password: data!.temp_password })
      }
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Erreur')
    } finally { setSaving(false) }
  }

  if (createdCreds) {
    return (
      <div style={{ padding: '1.5rem', maxWidth: 460 }}>
        <div style={{ background: '#fffbeb', border: '1px solid #f6c90e', borderRadius: 8, padding: '1.5rem' }}>
          <h3 style={{ marginTop: 0, color: '#92400e' }}>Utilisateur créé — copiez ce mot de passe</h3>
          <p style={{ marginBottom: 4, color: '#666', fontSize: '0.85rem' }}>Email de connexion :</p>
          <code style={{ display: 'block', background: '#f5f6fa', padding: '0.4rem 0.6rem', borderRadius: 4, marginBottom: '1rem', fontSize: '0.85rem' }}>
            {createdCreds.email}
          </code>
          <p style={{ marginBottom: 4, color: '#666', fontSize: '0.85rem' }}>Mot de passe temporaire :</p>
          <code style={{ display: 'block', background: '#f5f6fa', padding: '0.6rem', borderRadius: 4, fontSize: '1.2rem', fontWeight: 700, letterSpacing: '0.1em', marginBottom: '0.75rem' }}>
            {createdCreds.temp_password}
          </code>
          <p style={{ color: '#e53e3e', fontSize: '0.85rem', marginBottom: '1rem' }}>
            Ce mot de passe ne sera plus affiché après fermeture.
          </p>
          <button onClick={() => navigate('/admin/users')} style={btnPrimary}>
            J'ai copié le mot de passe
          </button>
        </div>
      </div>
    )
  }

  return (
    <div style={{ padding: '1.5rem', maxWidth: 500 }}>
      <h2 style={{ marginTop: 0 }}>{isEdit ? 'Éditer utilisateur' : 'Nouvel utilisateur'}</h2>
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}

      {!isEdit && (
        <div style={{ display: 'flex', gap: 4, marginBottom: '1.5rem', background: '#f5f6fa', borderRadius: 8, padding: 4 }}>
          {(['invite', 'direct'] as const).map(m => (
            <button
              key={m}
              onClick={() => setMode(m)}
              style={{ flex: 1, padding: '0.5rem', borderRadius: 6, border: 'none', cursor: 'pointer',
                background: mode === m ? '#fff' : 'transparent',
                fontWeight: mode === m ? 600 : 400,
                boxShadow: mode === m ? '0 1px 3px rgba(0,0,0,0.1)' : 'none' }}
            >
              {m === 'invite' ? 'Invitation email' : 'Création directe'}
            </button>
          ))}
        </div>
      )}

      {mode === 'invite' && !isEdit && (
        <div style={{ marginBottom: '1rem' }}>
          <FieldLabel txt="Email *" />
          <input style={inputStyle} type="email" value={email} onChange={e => setEmail(e.target.value)} />
        </div>
      )}

      {(mode === 'direct' || isEdit) && (
        <div style={{ marginBottom: '1rem' }}>
          <FieldLabel txt="Nom affiché *" />
          <input style={inputStyle} value={displayName} onChange={e => setDisplayName(e.target.value)} placeholder="Jean Martin" />
        </div>
      )}

      {mode === 'direct' && !isEdit && (
        <div style={{ marginBottom: '1rem' }}>
          <FieldLabel txt="ID employé *" />
          <input style={inputStyle} value={employeeId} onChange={e => setEmployeeId(e.target.value)} placeholder="EMP-0042" />
        </div>
      )}

      <div style={{ marginBottom: '1rem' }}>
        <FieldLabel txt="Rôle *" />
        <select style={inputStyle} value={role} onChange={e => setRole(e.target.value)}>
          {ROLES.map(r => <option key={r} value={r}>{r}</option>)}
        </select>
      </div>

      {needsSite && (
        <div style={{ marginBottom: '1.5rem' }}>
          <FieldLabel txt="Site *" />
          <select style={inputStyle} value={siteId} onChange={e => setSiteId(e.target.value)}>
            <option value="">— Sélectionner un site —</option>
            {sites.map(s => <option key={s.id} value={s.id}>{s.name} ({s.site_code})</option>)}
          </select>
        </div>
      )}

      <div style={{ display: 'flex', gap: 8 }}>
        <button
          onClick={handleSubmit}
          disabled={saving
            || (!isEdit && mode === 'invite' && !email.trim())
            || (!isEdit && mode === 'direct' && (!displayName.trim() || !employeeId.trim()))
          }
          style={btnPrimary}
        >
          {saving ? '…' : isEdit ? 'Mettre à jour' : mode === 'invite' ? "Envoyer l'invitation" : 'Créer'}
        </button>
        <button onClick={() => navigate('/admin/users')} style={btnSecondary}>Annuler</button>
      </div>
    </div>
  )
}
