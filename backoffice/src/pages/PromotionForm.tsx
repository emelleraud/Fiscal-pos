import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { supabase } from '../supabaseClient'
import { useAuth } from '../context/AuthContext'
import { useRole } from '../hooks/useRole'

const PROMO_TYPES = ['fixed_amount','percentage','item_discount','bogo','happy_hour']
const DAYS = [{ v: 1, l: 'Lun' },{ v: 2, l: 'Mar' },{ v: 3, l: 'Mer' },
              { v: 4, l: 'Jeu' },{ v: 5, l: 'Ven' },{ v: 6, l: 'Sam' },{ v: 7, l: 'Dim' }]

const inp = { padding:'0.5rem 0.75rem',border:'1px solid #ddd',borderRadius:6,fontSize:'0.9rem',width:'100%',boxSizing:'border-box' as const }

function FieldLabel({ children }: { children: string }) {
  return <label style={{ display:'block', fontWeight:600, marginBottom:4, fontSize:'0.85rem' }}>{children}</label>
}
function SectionHeader({ children }: { children: string }) {
  return <h3 style={{ marginTop:'1.5rem', marginBottom:'0.75rem', fontSize:'0.95rem', color:'#444', borderBottom:'1px solid #eee', paddingBottom:4 }}>{children}</h3>
}

export default function PromotionForm() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const { session } = useAuth()
  const { hasRole } = useRole()
  const isEdit = Boolean(id)

  const [name, setName] = useState('')
  const [scope, setScope] = useState<'site'|'group'|'chain'>('site')
  const [siteId, setSiteId] = useState('')
  const [groupId, setGroupId] = useState('')
  const [trigger, setTrigger] = useState<'auto'|'manual'>('manual')
  const [promoType, setPromoType] = useState('fixed_amount')
  const [valueCents, setValueCents] = useState('')
  const [valueBps, setValueBps] = useState('')
  const [targetSku, setTargetSku] = useState('')
  const [exclusionGroup, setExclusionGroup] = useState('')
  const [priority, setPriority] = useState('0')
  const [validFrom, setValidFrom] = useState('')
  const [validTo, setValidTo] = useState('')
  const [daysOfWeek, setDaysOfWeek] = useState<number[]>([])
  const [timeFrom, setTimeFrom] = useState('')
  const [timeTo, setTimeTo] = useState('')
  const [status, setStatus] = useState('draft')

  const [sites, setSites] = useState<{id:string;name:string;site_code:string}[]>([])
  const [groups, setGroups] = useState<{id:string;name:string}[]>([])
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string|null>(null)

  useEffect(() => {
    supabase.from('sites').select('id,name,site_code').then(({data, error: e}) => {
      if (e) setError(e.message); else setSites(data ?? [])
    })
    supabase.from('restaurant_groups').select('id,name').then(({data, error: e}) => {
      if (e) setError(e.message); else setGroups(data ?? [])
    })
    if (isEdit) {
      supabase.from('promotions').select('*').eq('id', id!).single()
        .then(({ data: d, error: e }) => {
          if (e) { setError(e.message); return }
          if (!d) return
          setName(d.name); setScope(d.scope); setSiteId(d.site_id??'')
          setGroupId(d.group_id??''); setTrigger(d.trigger); setPromoType(d.promo_type)
          setValueCents(d.value_cents?.toString()??''); setValueBps(d.value_bps?.toString()??'')
          setTargetSku(d.target_sku??''); setExclusionGroup(d.exclusion_group??'')
          setPriority(d.priority?.toString()??'0'); setValidFrom(d.valid_from??'')
          setValidTo(d.valid_to??''); setDaysOfWeek(d.days_of_week??[])
          setTimeFrom(d.time_from??''); setTimeTo(d.time_to??''); setStatus(d.status)
        })
    }
  }, [id, isEdit])

  const toggleDay = (v: number) =>
    setDaysOfWeek(prev => prev.includes(v) ? prev.filter(d => d!==v) : [...prev, v])

  const handleSave = async (submitForApproval = false) => {
    setSaving(true); setError(null)
    try {
      if (!name.trim()) throw new Error('Le nom est obligatoire')
      if (scope === 'site' && !siteId) throw new Error('Veuillez sélectionner un site')
      if (scope === 'group' && !groupId) throw new Error('Veuillez sélectionner un groupe')

      const payload: Record<string, unknown> = {
        name, scope, trigger, promo_type: promoType,
        value_cents: valueCents ? parseInt(valueCents,10) : null,
        value_bps: valueBps ? parseInt(valueBps,10) : null,
        target_sku: targetSku || null,
        exclusion_group: exclusionGroup || null,
        priority: parseInt(priority,10) || 0,
        valid_from: validFrom || null, valid_to: validTo || null,
        days_of_week: daysOfWeek.length ? daysOfWeek : null,
        time_from: timeFrom || null, time_to: timeTo || null,
        site_id: scope === 'site' ? siteId : null,
        group_id: scope === 'group' ? groupId : null,
        status: submitForApproval ? 'pending_approval' : status,
      }
      if (!isEdit) payload.created_by = session?.user?.id

      const { error: e } = isEdit
        ? await supabase.from('promotions').update(payload).eq('id', id!)
        : await supabase.from('promotions').insert(payload)
      if (e) throw e
      navigate('/promotions')
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Erreur de sauvegarde')
    } finally { setSaving(false) }
  }

  const handleApprove = async () => {
    const { error: e } = await supabase.from('promotions').update({
      status: 'approved', approved_by: session?.user?.id, approved_at: new Date().toISOString()
    }).eq('id', id!)
    if (e) { setError(e.message); return }
    navigate('/promotions')
  }

  const needsSku = ['item_discount','bogo'].includes(promoType)
  const needsBps = ['percentage','happy_hour'].includes(promoType)
  const needsCents = ['fixed_amount','item_discount','happy_hour'].includes(promoType)

  return (
    <div style={{ padding:'1.5rem', maxWidth:640 }}>
      <h2 style={{ marginTop:0 }}>{isEdit ? 'Éditer' : 'Nouvelle'} promotion</h2>
      {error && <p style={{ color:'#e53e3e' }}>{error}</p>}

      <SectionHeader>1. Identité</SectionHeader>
      <div style={{ marginBottom:'0.75rem' }}><FieldLabel>Nom</FieldLabel><input style={inp} value={name} onChange={e=>setName(e.target.value)} /></div>
      <div style={{ display:'grid',gridTemplateColumns:'1fr 1fr',gap:12,marginBottom:'0.75rem' }}>
        <div><FieldLabel>Portée</FieldLabel><select style={inp} value={scope} onChange={e=>setScope(e.target.value as 'site'|'group'|'chain')}>
          <option value="site">Site</option><option value="group">Groupe</option><option value="chain">Chaîne</option>
        </select></div>
        <div><FieldLabel>Déclencheur</FieldLabel><select style={inp} value={trigger} onChange={e=>setTrigger(e.target.value as 'auto'|'manual')}>
          <option value="manual">Manuel (caissier)</option><option value="auto">Automatique</option>
        </select></div>
      </div>
      {scope==='site' && <div style={{ marginBottom:'0.75rem' }}><FieldLabel>Site</FieldLabel>
        <select style={inp} value={siteId} onChange={e=>setSiteId(e.target.value)}>
          <option value="">— choisir —</option>
          {sites.map(s=><option key={s.id} value={s.id}>{s.name} ({s.site_code})</option>)}
        </select></div>}
      {scope==='group' && <div style={{ marginBottom:'0.75rem' }}><FieldLabel>Groupe</FieldLabel>
        <select style={inp} value={groupId} onChange={e=>setGroupId(e.target.value)}>
          <option value="">— choisir —</option>
          {groups.map(g=><option key={g.id} value={g.id}>{g.name}</option>)}
        </select></div>}

      <SectionHeader>2. Mécanique</SectionHeader>
      <div style={{ marginBottom:'0.75rem' }}><FieldLabel>Type de remise</FieldLabel><select style={inp} value={promoType} onChange={e=>setPromoType(e.target.value)}>
        {PROMO_TYPES.map(t=><option key={t} value={t}>{t}</option>)}
      </select></div>
      {needsCents && <div style={{ marginBottom:'0.75rem' }}><FieldLabel>Montant fixe (centimes)</FieldLabel>
        <input style={inp} type="number" value={valueCents} onChange={e=>setValueCents(e.target.value)} placeholder="ex: 200 = 2,00 €" /></div>}
      {needsBps && <div style={{ marginBottom:'0.75rem' }}><FieldLabel>Pourcentage (basis points, 1000 = 10%)</FieldLabel>
        <input style={inp} type="number" value={valueBps} onChange={e=>setValueBps(e.target.value)} placeholder="ex: 1000 = 10%" /></div>}
      {needsSku && <div style={{ marginBottom:'0.75rem' }}><FieldLabel>SKU cible</FieldLabel>
        <input style={inp} value={targetSku} onChange={e=>setTargetSku(e.target.value)} placeholder="ex: BUR-001" /></div>}

      <SectionHeader>3. Cumul</SectionHeader>
      <div style={{ display:'grid',gridTemplateColumns:'1fr 1fr',gap:12 }}>
        <div><FieldLabel>{"Groupe d'exclusion"}</FieldLabel><input style={inp} value={exclusionGroup} onChange={e=>setExclusionGroup(e.target.value)} placeholder="ex: remise_panier" /></div>
        <div><FieldLabel>Priorité</FieldLabel><input style={inp} type="number" value={priority} onChange={e=>setPriority(e.target.value)} /></div>
      </div>

      <SectionHeader>4. Validité</SectionHeader>
      <div style={{ display:'grid',gridTemplateColumns:'1fr 1fr',gap:12,marginBottom:'0.75rem' }}>
        <div><FieldLabel>Début</FieldLabel><input style={inp} type="date" value={validFrom} onChange={e=>setValidFrom(e.target.value)} /></div>
        <div><FieldLabel>Fin</FieldLabel><input style={inp} type="date" value={validTo} onChange={e=>setValidTo(e.target.value)} /></div>
      </div>
      <div style={{ marginBottom:'0.75rem' }}><FieldLabel>Jours de la semaine</FieldLabel>
        <div style={{ display:'flex',gap:8,flexWrap:'wrap',marginTop:4 }}>
          {DAYS.map(d=>(
            <label key={d.v} style={{ display:'flex',alignItems:'center',gap:4,cursor:'pointer',
              padding:'0.25rem 0.6rem',border:'1px solid #ddd',borderRadius:20,
              background: daysOfWeek.includes(d.v) ? '#4f8ef7' : '#fff',
              color: daysOfWeek.includes(d.v) ? '#fff' : '#333', fontSize:'0.85rem' }}>
              <input type="checkbox" style={{ display:'none' }} checked={daysOfWeek.includes(d.v)} onChange={()=>toggleDay(d.v)} />{d.l}
            </label>
          ))}
        </div>
      </div>
      <div style={{ display:'grid',gridTemplateColumns:'1fr 1fr',gap:12 }}>
        <div><FieldLabel>Heure début</FieldLabel><input style={inp} type="time" value={timeFrom} onChange={e=>setTimeFrom(e.target.value)} /></div>
        <div><FieldLabel>Heure fin</FieldLabel><input style={inp} type="time" value={timeTo} onChange={e=>setTimeTo(e.target.value)} /></div>
      </div>

      <div style={{ display:'flex',gap:8,marginTop:'1.5rem',flexWrap:'wrap' }}>
        <button onClick={() => handleSave(false)} disabled={saving||!name.trim()}
          style={{ background:'#4f8ef7',color:'#fff',border:'none',borderRadius:6,padding:'0.6rem 1.2rem',cursor:'pointer',fontWeight:600 }}>
          {saving ? 'Sauvegarde…' : 'Enregistrer (brouillon)'}
        </button>
        {status === 'draft' && (
          <button onClick={() => handleSave(true)} disabled={saving||!name.trim()}
            style={{ background:'#ed8936',color:'#fff',border:'none',borderRadius:6,padding:'0.6rem 1.2rem',cursor:'pointer',fontWeight:600 }}>
            Soumettre pour approbation
          </button>
        )}
        {status === 'pending_approval' && hasRole('director') && (
          <button onClick={handleApprove}
            style={{ background:'#38a169',color:'#fff',border:'none',borderRadius:6,padding:'0.6rem 1.2rem',cursor:'pointer',fontWeight:600 }}>
            Approuver directement
          </button>
        )}
        <button onClick={() => navigate('/promotions')}
          style={{ background:'#f5f6fa',border:'1px solid #ddd',borderRadius:6,padding:'0.6rem 1rem',cursor:'pointer' }}>
          Annuler
        </button>
      </div>
    </div>
  )
}
