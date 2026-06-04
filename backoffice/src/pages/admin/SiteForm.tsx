// backoffice/src/pages/admin/SiteForm.tsx
import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { supabase } from '../../supabaseClient'

const inputStyle = { padding: '0.5rem 0.75rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.9rem', width: '100%', boxSizing: 'border-box' as const }

function FieldLabel({ txt }: { txt: string }) {
  return <label style={{ display: 'block', fontWeight: 600, marginBottom: 4, fontSize: '0.85rem' }}>{txt}</label>
}

export default function SiteForm() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const isEdit = id !== undefined

  const [siteCode, setSiteCode] = useState('')
  const [name, setName] = useState('')
  const [address, setAddress] = useState('')
  const [siret, setSiret] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!isEdit) return
    supabase.from('sites').select('*').eq('id', id!).single()
      .then(({ data, error: e }) => {
        if (e) { setError(e.message); return }
        if (!data) return
        setSiteCode(data.site_code as string)
        setName(data.name as string)
        setAddress((data.address as string) ?? '')
        setSiret((data.siret as string) ?? '')
      })
  }, [id, isEdit])

  const handleSave = async () => {
    setSaving(true); setError(null)
    try {
      const payload = {
        site_code: siteCode.trim(),
        name: name.trim(),
        address: address.trim() || null,
        siret: siret.trim() || null,
      }
      if (isEdit) {
        const { error: e } = await supabase.from('sites').update(payload).eq('id', id!)
        if (e) throw e
      } else {
        const { error: e } = await supabase.from('sites').insert(payload)
        if (e) throw e
      }
      navigate('/admin/sites')
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Erreur de sauvegarde')
    } finally { setSaving(false) }
  }

  return (
    <div style={{ padding: '1.5rem', maxWidth: 500 }}>
      <h2 style={{ marginTop: 0 }}>{isEdit ? 'Éditer' : 'Nouveau'} restaurant</h2>
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}

      <div style={{ marginBottom: '1rem' }}>
        <FieldLabel txt="Code site *" />
        <input style={inputStyle} value={siteCode} onChange={e => setSiteCode(e.target.value)} placeholder="PARIS-01" />
      </div>
      <div style={{ marginBottom: '1rem' }}>
        <FieldLabel txt="Nom *" />
        <input style={inputStyle} value={name} onChange={e => setName(e.target.value)} placeholder="Restaurant Paris 1er" />
      </div>
      <div style={{ marginBottom: '1rem' }}>
        <FieldLabel txt="Adresse" />
        <input style={inputStyle} value={address} onChange={e => setAddress(e.target.value)} placeholder="12 rue de Rivoli, 75001 Paris" />
      </div>
      <div style={{ marginBottom: '1.5rem' }}>
        <FieldLabel txt="SIRET (14 chiffres)" />
        <input style={inputStyle} value={siret} onChange={e => setSiret(e.target.value)} placeholder="12345678901234" maxLength={14} />
      </div>

      <div style={{ display: 'flex', gap: 8 }}>
        <button
          onClick={handleSave}
          disabled={saving || !siteCode.trim() || !name.trim()}
          style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.6rem 1.2rem', cursor: 'pointer', fontWeight: 600 }}
        >
          {saving ? 'Sauvegarde…' : 'Enregistrer'}
        </button>
        <button
          onClick={() => navigate('/admin/sites')}
          style={{ background: '#f5f6fa', border: '1px solid #ddd', borderRadius: 6, padding: '0.6rem 1rem', cursor: 'pointer' }}
        >
          Annuler
        </button>
      </div>
    </div>
  )
}
