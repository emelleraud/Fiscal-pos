import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { supabase } from '../../supabaseClient'
import { useSite } from '../../context/SiteContext'

const inputStyle = { padding: '0.5rem 0.75rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.9rem', width: '100%', boxSizing: 'border-box' as const }
const selectStyle = { ...inputStyle }

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={{ marginBottom: '1rem' }}>
      <label style={{ display: 'block', fontWeight: 600, marginBottom: 4, fontSize: '0.85rem' }}>{label}</label>
      {children}
    </div>
  )
}

const ALL_PROFILES = ['normal', 'rush']

export default function KdsStationForm() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const { activeSiteId } = useSite()
  const isEdit = id !== undefined

  const [stationId, setStationId] = useState('')
  const [name, setName] = useState('')
  const [role, setRole] = useState('preparation')
  const [tempGroup, setTempGroup] = useState('')
  const [outputType, setOutputType] = useState('screen')
  const [printerAddress, setPrinterAddress] = useState('')
  const [printerType, setPrinterType] = useState('')
  const [printerMode, setPrinterMode] = useState('')
  const [paperWidth, setPaperWidth] = useState('')
  const [fallbackId, setFallbackId] = useState('')
  const [activeProfiles, setActiveProfiles] = useState<string[]>(['normal'])
  const [sortOrder, setSortOrder] = useState('0')
  const [enabled, setEnabled] = useState(true)

  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!isEdit || !activeSiteId) return
    supabase.from('kds_station_configs').select('*').eq('site_id', activeSiteId).eq('id', id!).single()
      .then(({ data, error: e }) => {
        if (e) { setError(e.message); return }
        if (!data) return
        const d = data as Record<string, unknown>
        setStationId(d.id as string)
        setName(d.name as string)
        setRole(d.role as string)
        setTempGroup((d.temperature_group as string) ?? '')
        setOutputType(d.output_type as string)
        setPrinterAddress((d.printer_address as string) ?? '')
        setPrinterType((d.printer_type as string) ?? '')
        setPrinterMode((d.printer_mode as string) ?? '')
        setPaperWidth(d.paper_width_mm !== null && d.paper_width_mm !== undefined ? String(d.paper_width_mm) : '')
        setFallbackId((d.fallback_station_id as string) ?? '')
        const profiles = JSON.parse((d.active_in_profiles as string) ?? '["normal"]') as string[]
        setActiveProfiles(profiles)
        setSortOrder(String(d.sort_order ?? 0))
        setEnabled((d.enabled as number) !== 0)
      })
  }, [id, isEdit, activeSiteId])

  const toggleProfile = (p: string) => {
    setActiveProfiles(prev =>
      prev.includes(p) ? prev.filter(x => x !== p) : [...prev, p]
    )
  }

  const handleSave = async () => {
    if (!activeSiteId) { setError('Sélectionner un site'); return }
    setSaving(true); setError(null)
    try {
      const payload = {
        id: stationId.trim(),
        site_id: activeSiteId,
        name: name.trim(),
        role,
        temperature_group: tempGroup.trim() || null,
        output_type: outputType,
        printer_address: printerAddress.trim() || null,
        printer_type: printerType || null,
        printer_mode: printerMode || null,
        paper_width_mm: paperWidth ? parseInt(paperWidth, 10) : null,
        fallback_station_id: fallbackId.trim() || null,
        active_in_profiles: JSON.stringify(activeProfiles),
        sort_order: parseInt(sortOrder, 10) || 0,
        enabled: enabled ? 1 : 0,
      }

      if (isEdit) {
        const { error: e } = await supabase.from('kds_station_configs')
          .update(payload).eq('site_id', activeSiteId).eq('id', id!)
        if (e) throw e
      } else {
        const { error: e } = await supabase.from('kds_station_configs').insert(payload)
        if (e) throw e
      }
      navigate('/kitchen/stations')
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : (e as { message?: string })?.message ?? 'Erreur de sauvegarde')
    } finally { setSaving(false) }
  }

  const showPrinter = outputType === 'printer' || outputType === 'both'

  return (
    <div style={{ padding: '1.5rem', maxWidth: 600 }}>
      <h2 style={{ marginTop: 0 }}>{isEdit ? 'Éditer' : 'Nouvelle'} station cuisine</h2>
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}

      <Field label="ID station *">
        <input style={inputStyle} value={stationId} onChange={e => setStationId(e.target.value)}
          placeholder="grill-01" disabled={isEdit} />
        <small style={{ color: '#888' }}>Identifiant unique (URL-safe, ex: grill-01, friture, boissons)</small>
      </Field>

      <Field label="Nom affiché *">
        <input style={inputStyle} value={name} onChange={e => setName(e.target.value)} placeholder="Grill" />
      </Field>

      <Field label="Rôle *">
        <select style={selectStyle} value={role} onChange={e => setRole(e.target.value)}>
          <option value="preparation">Préparation</option>
          <option value="holding">Rassemblement / Chaud-Froid</option>
          <option value="assembly">Assemblage Expo</option>
          <option value="ready_board">Order Ready Board (ORB)</option>
        </select>
      </Field>

      <Field label="Groupe de température">
        <select style={selectStyle} value={tempGroup} onChange={e => setTempGroup(e.target.value)}>
          <option value="">— Aucun —</option>
          <option value="hot">Chaud</option>
          <option value="cold">Froid</option>
          <option value="other">Autre</option>
        </select>
      </Field>

      <Field label="Type de sortie *">
        <select style={selectStyle} value={outputType} onChange={e => setOutputType(e.target.value)}>
          <option value="screen">Écran uniquement</option>
          <option value="printer">Imprimante uniquement</option>
          <option value="both">Écran + Imprimante</option>
        </select>
      </Field>

      {showPrinter && (
        <>
          <Field label="Adresse imprimante">
            <input style={inputStyle} value={printerAddress} onChange={e => setPrinterAddress(e.target.value)}
              placeholder="192.168.1.100:9100 | /dev/usb/lp0 | /tmp/tickets" />
          </Field>
          <Field label="Type connexion imprimante">
            <select style={selectStyle} value={printerType} onChange={e => setPrinterType(e.target.value)}>
              <option value="">— Choisir —</option>
              <option value="tcpip">TCP/IP (réseau)</option>
              <option value="usb">USB (via kds-print-agent)</option>
              <option value="file">Fichier (test)</option>
            </select>
          </Field>
          <Field label="Mode impression">
            <select style={selectStyle} value={printerMode} onChange={e => setPrinterMode(e.target.value)}>
              <option value="">— Choisir —</option>
              <option value="receipt">Ticket continu (receipt)</option>
              <option value="linerless_label">Labels adhésifs (linerless)</option>
            </select>
          </Field>
          <Field label="Largeur papier (mm)">
            <select style={selectStyle} value={paperWidth} onChange={e => setPaperWidth(e.target.value)}>
              <option value="">— Choisir —</option>
              <option value="80">80 mm</option>
              <option value="50">50 mm</option>
            </select>
          </Field>
        </>
      )}

      <Field label="Station de fallback (ID)">
        <input style={inputStyle} value={fallbackId} onChange={e => setFallbackId(e.target.value)}
          placeholder="grill-02 (optionnel)" />
      </Field>

      <Field label="Profils actifs">
        <div style={{ display: 'flex', gap: '1.5rem' }}>
          {ALL_PROFILES.map(p => (
            <label key={p} style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
              <input type="checkbox" checked={activeProfiles.includes(p)}
                onChange={() => toggleProfile(p)} />
              <span style={{ fontWeight: 500 }}>{p.charAt(0).toUpperCase() + p.slice(1)}</span>
            </label>
          ))}
        </div>
      </Field>

      <Field label="Ordre d'affichage">
        <input style={{ ...inputStyle, width: 80 }} type="number" value={sortOrder}
          onChange={e => setSortOrder(e.target.value)} min="0" />
      </Field>

      <Field label="État">
        <label style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer' }}>
          <input type="checkbox" checked={enabled} onChange={e => setEnabled(e.target.checked)} />
          <span>Station active</span>
        </label>
      </Field>

      <div style={{ display: 'flex', gap: 8, marginTop: '0.5rem' }}>
        <button
          onClick={handleSave}
          disabled={saving || !stationId.trim() || !name.trim()}
          style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.6rem 1.2rem', cursor: 'pointer', fontWeight: 600 }}
        >
          {saving ? 'Sauvegarde…' : 'Enregistrer'}
        </button>
        <button
          onClick={() => navigate('/kitchen/stations')}
          style={{ background: '#f5f6fa', border: '1px solid #ddd', borderRadius: 6, padding: '0.6rem 1rem', cursor: 'pointer' }}
        >
          Annuler
        </button>
      </div>
    </div>
  )
}
