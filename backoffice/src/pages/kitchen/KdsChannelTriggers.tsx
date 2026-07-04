import { useEffect, useState } from 'react'
import { supabase } from '../../supabaseClient'
import { useSite } from '../../context/SiteContext'

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

const TRIGGER_LABEL: Record<string, string> = {
  order: 'Commande',
  payment: 'Paiement',
  both: 'Les deux',
}

export default function KdsChannelTriggers() {
  const [triggers, setTriggers] = useState<ChannelTrigger[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const { activeSiteId } = useSite()

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
              <td style={{ padding: '0.6rem 0.8rem', color: '#555' }}>{TRIGGER_LABEL[t.trigger_on]}</td>
              <td style={{ padding: '0.6rem 0.8rem', color: '#555' }}>{t.orb_type ?? '—'}</td>
              <td style={{ padding: '0.6rem 0.8rem' }}></td>
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
