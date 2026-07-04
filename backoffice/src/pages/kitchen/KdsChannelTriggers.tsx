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

const CHANNELS = ['caisse', 'kiosk', 'drive', 'delivery']
const ORDER_TYPES = ['eat_in', 'takeaway', 'drive', 'delivery', 'click_and_collect']

export default function KdsChannelTriggers() {
  const [triggers, setTriggers] = useState<ChannelTrigger[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [edited, setEdited] = useState<Record<TriggerKey, ChannelTrigger>>({})
  const [saving, setSaving] = useState<TriggerKey | null>(null)
  const [deleting, setDeleting] = useState<TriggerKey | null>(null)
  const [newChannel, setNewChannel] = useState('')
  const [newOrderType, setNewOrderType] = useState('')
  const [newTriggerOn, setNewTriggerOn] = useState<'order' | 'payment' | 'both'>('order')
  const [newOrbType, setNewOrbType] = useState<'' | 'client' | 'livreur'>('')
  const [adding, setAdding] = useState(false)

  const { activeSiteId } = useSite()
  const { role } = useAuth()
  const canWrite = role === 'pos_admin' || role === 'regional_director'

  const load = () => {
    if (!activeSiteId) { setLoading(false); return }
    setLoading(true)
    setEdited({})
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

  const handleAdd = async () => {
    if (!activeSiteId || !newChannel || !newOrderType) return
    setAdding(true); setError(null)
    const { error: e } = await supabase.from('kds_channel_triggers').insert({
      site_id: activeSiteId,
      channel: newChannel,
      order_type: newOrderType,
      trigger_on: newTriggerOn,
      orb_type: newOrbType || null,
    })
    if (e) setError(e.message)
    else {
      setNewChannel(''); setNewOrderType(''); setNewTriggerOn('order'); setNewOrbType('')
      load()
    }
    setAdding(false)
  }

  const takenKeys = new Set(triggers.map(rowKey))

  const availableChannels = CHANNELS.filter(ch =>
    ORDER_TYPES.some(ot => !takenKeys.has(`${ch}:${ot}`))
  )

  const availableOrderTypes = newChannel
    ? ORDER_TYPES.filter(ot => !takenKeys.has(`${newChannel}:${ot}`))
    : []

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
              <td style={{ padding: '0.6rem 0.8rem' }}>
                <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
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
                </div>
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

      {canWrite && availableChannels.length > 0 && (
        <div style={{ background: '#f9f9fb', border: '1px solid #eee', borderRadius: 8, padding: '1rem' }}>
          <h4 style={{ marginTop: 0, marginBottom: '0.75rem', fontSize: '0.9rem' }}>+ Ajouter un déclencheur</h4>
          <div style={{ display: 'flex', gap: 8, alignItems: 'flex-end', flexWrap: 'wrap' }}>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Canal</label>
              <select
                value={newChannel}
                onChange={e => { setNewChannel(e.target.value); setNewOrderType('') }}
                style={{ padding: '0.35rem 0.6rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem' }}
              >
                <option value="">— Choisir —</option>
                {availableChannels.map(ch => (
                  <option key={ch} value={ch}>{CHANNEL_LABEL[ch] ?? ch}</option>
                ))}
              </select>
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Type commande</label>
              <select
                value={newOrderType}
                onChange={e => setNewOrderType(e.target.value)}
                disabled={!newChannel}
                style={{ padding: '0.35rem 0.6rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem' }}
              >
                <option value="">— Choisir —</option>
                {availableOrderTypes.map(ot => (
                  <option key={ot} value={ot}>{ORDER_TYPE_LABEL[ot] ?? ot}</option>
                ))}
              </select>
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Déclenche sur</label>
              <select
                value={newTriggerOn}
                onChange={e => setNewTriggerOn(e.target.value as 'order' | 'payment' | 'both')}
                style={{ padding: '0.35rem 0.6rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem' }}
              >
                <option value="order">Commande</option>
                <option value="payment">Paiement</option>
                <option value="both">Les deux</option>
              </select>
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>ORB</label>
              <select
                value={newOrbType}
                onChange={e => setNewOrbType(e.target.value as '' | 'client' | 'livreur')}
                style={{ padding: '0.35rem 0.6rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem' }}
              >
                <option value="">—</option>
                <option value="client">client</option>
                <option value="livreur">livreur</option>
              </select>
            </div>
            <button
              onClick={handleAdd}
              disabled={adding || !newChannel || !newOrderType}
              style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.45rem 0.9rem', cursor: 'pointer', fontWeight: 600 }}
            >
              {adding ? '…' : 'Ajouter'}
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
