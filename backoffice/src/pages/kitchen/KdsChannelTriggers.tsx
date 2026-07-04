import { useEffect, useState } from 'react'
import { supabase } from '../../supabaseClient'
import { useSite } from '../../context/SiteContext'
import { useAuth } from '../../context/AuthContext'

interface ChannelTrigger {
  channel: string
  order_type: string
  trigger_on: 'order' | 'payment' | 'both'
  orb_type: 'client' | 'livreur' | null
}

type TriggerKey = string // `${channel}:${order_type}`

const rowKey = (t: ChannelTrigger): TriggerKey => `${t.channel}:${t.order_type}`

const CHANNEL_LABEL: Record<string, string> = {
  caisse: 'Caisse',
  kiosk: 'Kiosk',
  drive: 'Drive',
  delivery: 'Livraison',
}

const ORDER_TYPE_LABEL: Record<string, string> = {
  eat_in: 'Sur place',
  takeaway: 'À emporter',
  drive: 'Drive',
  delivery: 'Livraison',
  click_and_collect: 'Click & Collect',
}

export default function KdsChannelTriggers() {
  const [triggers, setTriggers] = useState<ChannelTrigger[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [edited, setEdited] = useState<Record<TriggerKey, ChannelTrigger>>({})
  const [saving, setSaving] = useState<TriggerKey | null>(null)
  const [deleting, setDeleting] = useState<TriggerKey | null>(null)

  const { activeSiteId } = useSite()
  const { role } = useAuth()
  const canWrite = role === 'pos_admin' || role === 'regional_director'

  const load = () => {
    if (!activeSiteId) { setLoading(false); return }
    setLoading(true)
    supabase
      .from('kds_channel_triggers')
      .select('channel,order_type,trigger_on,orb_type')
      .eq('site_id', activeSiteId)
      .order('channel').order('order_type')
      .then(({ data, error: e }) => {
        if (e) setError(e.message)
        else setTriggers((data as ChannelTrigger[]) ?? [])
        setLoading(false)
      })
  }

  useEffect(load, [activeSiteId])

  const getEdited = (t: ChannelTrigger): ChannelTrigger => edited[rowKey(t)] ?? t
  const isDirty = (t: ChannelTrigger): boolean =>
    JSON.stringify(getEdited(t)) !== JSON.stringify(t)

  const handleChange = (
    key: TriggerKey,
    base: ChannelTrigger,
    field: 'trigger_on' | 'orb_type',
    val: string,
  ) => {
    setEdited(prev => ({
      ...prev,
      [key]: { ...(prev[key] ?? base), [field]: val || null },
    }))
  }

  const handleSave = async (key: TriggerKey) => {
    const t = edited[key]
    if (!t || !activeSiteId) return
    setSaving(key); setError(null)
    const { error: e } = await supabase
      .from('kds_channel_triggers')
      .upsert({ site_id: activeSiteId, ...t }, { onConflict: 'site_id,channel,order_type' })
    if (e) setError(e.message)
    else {
      setEdited(prev => { const n = { ...prev }; delete n[key]; return n })
      load()
    }
    setSaving(null)
  }

  const handleDelete = async (t: ChannelTrigger) => {
    const label = `${CHANNEL_LABEL[t.channel] ?? t.channel} × ${ORDER_TYPE_LABEL[t.order_type] ?? t.order_type}`
    if (!window.confirm(`Supprimer le déclencheur "${label}" ?`)) return
    const key = rowKey(t)
    setDeleting(key); setError(null)
    const { error: e } = await supabase
      .from('kds_channel_triggers')
      .delete()
      .eq('site_id', activeSiteId!)
      .eq('channel', t.channel)
      .eq('order_type', t.order_type)
    if (e) setError(e.message)
    else load()
    setDeleting(null)
  }

  if (!activeSiteId) return <p style={{ padding: '1.5rem', color: '#888' }}>Sélectionner un site</p>
  if (loading) return <p style={{ padding: '1.5rem', color: '#888' }}>Chargement…</p>

  return (
    <div style={{ padding: '1.5rem' }}>
      <h2 style={{ marginTop: 0 }}>Déclencheurs canal KDS</h2>
      <p style={{ color: '#666', fontSize: '0.85rem', marginBottom: '1rem' }}>
        Définit quand et comment dispatcher les commandes vers le KDS selon le canal de vente.
      </p>
      {error && <p style={{ color: '#e53e3e', marginBottom: '1rem' }}>{error}</p>}

      <table style={{ width: '100%', borderCollapse: 'collapse', marginBottom: '1.5rem' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['CANAL', 'TYPE COMMANDE', 'DÉCLENCHE SUR', 'ORB', 'ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {triggers.map(t => (
            <tr key={rowKey(t)} style={{ borderBottom: '1px solid #f0f0f0' }}>
              <td style={{ padding: '0.6rem 0.8rem', fontWeight: 500 }}>{CHANNEL_LABEL[t.channel] ?? t.channel}</td>
              <td style={{ padding: '0.6rem 0.8rem' }}>{ORDER_TYPE_LABEL[t.order_type] ?? t.order_type}</td>
              <td style={{ padding: '0.6rem 0.8rem' }}>
                <select
                  value={getEdited(t).trigger_on}
                  disabled={!canWrite}
                  onChange={ev => handleChange(rowKey(t), t, 'trigger_on', ev.target.value)}
                  style={{ padding: '0.3rem 0.5rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem' }}
                >
                  <option value="order">Commande</option>
                  <option value="payment">Paiement</option>
                  <option value="both">Les deux</option>
                </select>
              </td>
              <td style={{ padding: '0.6rem 0.8rem' }}>
                <select
                  value={getEdited(t).orb_type ?? ''}
                  disabled={!canWrite}
                  onChange={ev => handleChange(rowKey(t), t, 'orb_type', ev.target.value)}
                  style={{ padding: '0.3rem 0.5rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem' }}
                >
                  <option value="">—</option>
                  <option value="client">client</option>
                  <option value="livreur">livreur</option>
                </select>
              </td>
              <td style={{ padding: '0.6rem 0.8rem', display: 'flex', gap: 8, alignItems: 'center' }}>
                {canWrite && isDirty(t) && (
                  <button
                    onClick={() => handleSave(rowKey(t))}
                    disabled={saving === rowKey(t)}
                    style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 4, padding: '0.3rem 0.7rem', cursor: 'pointer', fontSize: '0.8rem' }}
                  >
                    {saving === rowKey(t) ? '…' : 'Enregistrer'}
                  </button>
                )}
                {canWrite && (
                  <button
                    onClick={() => handleDelete(t)}
                    disabled={deleting === rowKey(t)}
                    style={{ background: 'none', border: 'none', color: '#e53e3e', cursor: 'pointer', fontSize: '0.85rem', padding: 0 }}
                  >
                    {deleting === rowKey(t) ? '…' : 'Supprimer'}
                  </button>
                )}
              </td>
            </tr>
          ))}
          {triggers.length === 0 && (
            <tr>
              <td colSpan={5} style={{ padding: '1.5rem', textAlign: 'center', color: '#aaa', fontSize: '0.85rem' }}>
                Aucun déclencheur configuré
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  )
}
