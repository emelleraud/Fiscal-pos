// backoffice/src/pages/admin/TechnicalConfigForm.tsx
import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { supabase } from '../../supabaseClient'

const inputStyle = { padding: '0.5rem 0.75rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.9rem', width: '100%', boxSizing: 'border-box' as const }

function FieldLabel({ txt }: { txt: string }) {
  return <label style={{ display: 'block', fontWeight: 600, marginBottom: 4, fontSize: '0.85rem' }}>{txt}</label>
}

interface SiteTechConfig {
  id: string
  edge_api_port: number
  sync_interval_s: number
  fiscal_key_configured_at: string | null
}

export default function TechnicalConfigForm() {
  const { id: siteId } = useParams<{ id: string }>()
  const navigate = useNavigate()

  const [config, setConfig] = useState<SiteTechConfig | null>(null)
  const [port, setPort] = useState('8080')
  const [syncInterval, setSyncInterval] = useState('300')
  const [keyHex, setKeyHex] = useState('')
  const [saving, setSaving] = useState(false)
  const [savingKey, setSavingKey] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [keyError, setKeyError] = useState<string | null>(null)

  useEffect(() => {
    if (!siteId) return
    supabase
      .from('site_technical_configs')
      .select('id,edge_api_port,sync_interval_s,fiscal_key_configured_at')
      .eq('site_id', siteId)
      .eq('device_type', 'pos')
      .maybeSingle()
      .then(({ data, error: e }) => {
        if (e) { setError(e.message); return }
        if (data) {
          setConfig(data as SiteTechConfig)
          setPort(String((data as SiteTechConfig).edge_api_port))
          setSyncInterval(String((data as SiteTechConfig).sync_interval_s))
        }
      })
  }, [siteId])

  const handleSaveParams = async () => {
    setSaving(true); setError(null)
    try {
      const payload = {
        site_id: siteId!,
        device_type: 'pos',
        edge_api_port: Number(port),
        sync_interval_s: Number(syncInterval),
        updated_at: new Date().toISOString(),
      }
      if (config) {
        const { error: e } = await supabase.from('site_technical_configs').update(payload).eq('id', config.id)
        if (e) throw e
      } else {
        const { error: e } = await supabase.from('site_technical_configs').insert(payload)
        if (e) throw e
      }
      navigate('/admin/sites')
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Erreur')
    } finally { setSaving(false) }
  }

  const handleProvisionKey = async () => {
    setKeyError(null)
    if (!/^[0-9a-fA-F]{64}$/.test(keyHex)) {
      setKeyError('La clé doit contenir exactement 64 caractères hexadécimaux')
      return
    }
    setSavingKey(true)
    try {
      const { data, error: e } = await supabase.functions.invoke<{ configured_at: string }>('config-provision', {
        body: { site_id: siteId, key_hex: keyHex },
      })
      if (e) throw e
      setKeyHex('')
      setConfig(prev => prev
        ? { ...prev, fiscal_key_configured_at: data!.configured_at }
        : { id: '', edge_api_port: Number(port), sync_interval_s: Number(syncInterval), fiscal_key_configured_at: data!.configured_at }
      )
    } catch (e: unknown) {
      setKeyError(e instanceof Error ? e.message : 'Erreur')
    } finally { setSavingKey(false) }
  }

  return (
    <div style={{ padding: '1.5rem', maxWidth: 500 }}>
      <h2 style={{ marginTop: 0 }}>Config technique</h2>
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}

      <div style={{ marginBottom: '1rem' }}>
        <FieldLabel txt="Port edge-api" />
        <input style={inputStyle} type="number" value={port} onChange={e => setPort(e.target.value)} min={1} max={65535} />
      </div>
      <div style={{ marginBottom: '1.5rem' }}>
        <FieldLabel txt="Intervalle sync (secondes)" />
        <input style={inputStyle} type="number" value={syncInterval} onChange={e => setSyncInterval(e.target.value)} min={10} />
      </div>

      <div style={{ display: 'flex', gap: 8, marginBottom: '2rem' }}>
        <button
          onClick={handleSaveParams}
          disabled={saving}
          style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.6rem 1.2rem', cursor: 'pointer', fontWeight: 600 }}
        >
          {saving ? 'Sauvegarde…' : 'Enregistrer'}
        </button>
        <button
          onClick={() => navigate('/admin/sites')}
          style={{ background: '#f5f6fa', border: '1px solid #ddd', borderRadius: 6, padding: '0.6rem 1rem', cursor: 'pointer' }}
        >
          Retour
        </button>
      </div>

      <div style={{ borderTop: '1px solid #eee', paddingTop: '1.5rem' }}>
        <h3 style={{ marginTop: 0, fontSize: '1rem' }}>Clé de signature fiscale (Ed25519)</h3>
        {config?.fiscal_key_configured_at ? (
          <p style={{ color: '#48bb78', marginBottom: '0.75rem', fontSize: '0.9rem' }}>
            ✓ Configurée le {new Date(config.fiscal_key_configured_at).toLocaleDateString('fr-FR')}
          </p>
        ) : (
          <p style={{ color: '#e53e3e', marginBottom: '0.75rem', fontSize: '0.9rem' }}>Non configurée</p>
        )}
        <div style={{ marginBottom: '0.5rem' }}>
          <FieldLabel txt={config?.fiscal_key_configured_at ? 'Remplacer la clé (64 hex)' : 'Clé privée Ed25519 (64 hex) *'} />
          <input
            style={{ ...inputStyle, fontFamily: 'monospace', fontSize: '0.75rem' }}
            type="password"
            value={keyHex}
            onChange={e => setKeyHex(e.target.value)}
            placeholder="0000000000000000000000000000000000000000000000000000000000000000"
            autoComplete="off"
          />
        </div>
        {keyError && <p style={{ color: '#e53e3e', fontSize: '0.85rem', marginBottom: '0.5rem' }}>{keyError}</p>}
        <p style={{ color: '#888', fontSize: '0.8rem', marginBottom: '0.75rem' }}>
          Cette clé ne peut pas être relue après enregistrement.
        </p>
        <button
          onClick={handleProvisionKey}
          disabled={savingKey || !keyHex}
          style={{ background: '#718096', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer' }}
        >
          {savingKey ? 'Configuration…' : 'Configurer la clé'}
        </button>
      </div>
    </div>
  )
}
