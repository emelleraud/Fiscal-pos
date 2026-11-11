# KdsChannelTriggers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Créer la page `/kitchen/triggers` dans le backoffice pour gérer les règles de déclenchement KDS par canal × type de commande.

**Architecture:** Un seul composant React `KdsChannelTriggers.tsx` avec édition inline (pattern identique à `KdsTimerThresholds.tsx`). Deux fichiers existants modifiés pour le routing et la navigation. Aucun impact backend — la table `kds_channel_triggers` existe déjà (migration 021) avec RLS en place.

**Tech Stack:** React 19, TypeScript, Supabase JS client, inline styles (pas de Tailwind — le backoffice utilise des inline styles partout)

## Global Constraints

- Backoffice : zéro test automatisé — les vérifications sont manuelles dans le navigateur (`cd backoffice && npm run dev` sur port 5173)
- Inline styles uniquement (pas de classes Tailwind ni de CSS modules — le backoffice n'en utilise pas)
- Pas de React Router interne dans le composant — le routing est géré par App.tsx
- `canWrite = role === 'pos_admin' || role === 'regional_director'` — identique aux autres pages kitchen
- `activeSiteId` vient de `useSite()` — toujours filtrer les requêtes Supabase avec `.eq('site_id', activeSiteId)`
- Clé composite de la table : `(site_id, channel, order_type)` — TriggerKey dans l'app = `"${channel}:${order_type}"`
- Ne jamais toucher fiscal-engine, edge-api, kds-engine, sync-client

---

## File Map

| Fichier | Action | Responsabilité |
|---|---|---|
| `backoffice/src/pages/kitchen/KdsChannelTriggers.tsx` | Créer | Composant complet |
| `backoffice/src/App.tsx` | Modifier | Ajouter import + route `/kitchen/triggers` |
| `backoffice/src/components/Layout.tsx` | Modifier | Ajouter lien nav "🎯 Déclencheurs canal" |

---

## Task 1: Routing + lien nav + stub

**Files:**
- Modify: `backoffice/src/App.tsx`
- Modify: `backoffice/src/components/Layout.tsx`
- Create: `backoffice/src/pages/kitchen/KdsChannelTriggers.tsx` (stub)

**Interfaces:**
- Produces: composant exporté par défaut `KdsChannelTriggers`, route `/kitchen/triggers` accessible

- [ ] **Step 1: Créer le stub du composant**

Créer `backoffice/src/pages/kitchen/KdsChannelTriggers.tsx` avec ce contenu minimal :

```tsx
export default function KdsChannelTriggers() {
  return <div style={{ padding: '1.5rem' }}><h2 style={{ marginTop: 0 }}>Déclencheurs canal</h2></div>
}
```

- [ ] **Step 2: Ajouter l'import et la route dans App.tsx**

Dans `backoffice/src/App.tsx`, ajouter l'import avec les autres imports kitchen (après la ligne `import KdsTimerThresholds`) :

```tsx
import KdsChannelTriggers from './pages/kitchen/KdsChannelTriggers'
```

Ajouter la route après la ligne `<Route path="/kitchen/thresholds" element={<KdsTimerThresholds />} />` :

```tsx
<Route path="/kitchen/triggers" element={<KdsChannelTriggers />} />
```

- [ ] **Step 3: Ajouter le lien dans Layout.tsx**

Dans `backoffice/src/components/Layout.tsx`, dans le tableau `navItems`, ajouter après `{ to: '/kitchen/thresholds', label: '⏱ Seuils timer' }` :

```ts
{ to: '/kitchen/triggers', label: '🎯 Déclencheurs canal' },
```

- [ ] **Step 4: Vérifier en navigateur**

```bash
cd /home/angelo/PROJ_POS_QSR/pos-fiscal/backoffice && npm run dev
```

Vérifier :
- Le lien "🎯 Déclencheurs canal" apparaît dans la sidebar sous "Cuisine"
- Cliquer dessus navigue vers `/kitchen/triggers`
- La page affiche le titre "Déclencheurs canal"
- Aucune erreur TypeScript ni dans la console

- [ ] **Step 5: Commit**

```bash
git add backoffice/src/pages/kitchen/KdsChannelTriggers.tsx backoffice/src/App.tsx backoffice/src/components/Layout.tsx
git commit -m "feat(backoffice): add /kitchen/triggers route and nav link"
```

---

## Task 2: Data loading + tableau en lecture seule

**Files:**
- Modify: `backoffice/src/pages/kitchen/KdsChannelTriggers.tsx`

**Interfaces:**
- Consumes: `useSite()` → `activeSiteId: string | null`, `useAuth()` → `role: string`
- Produces: tableau affiché avec les 7 lignes de `kds_channel_triggers` pour le site actif

- [ ] **Step 1: Remplacer le stub par le composant complet avec data loading**

Remplacer entièrement `backoffice/src/pages/kitchen/KdsChannelTriggers.tsx` :

```tsx
import { useEffect, useState } from 'react'
import { supabase } from '../../supabaseClient'
import { useAuth } from '../../context/AuthContext'
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

  const { role } = useAuth()
  const { activeSiteId } = useSite()
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
```

- [ ] **Step 2: Vérifier en navigateur**

Aller sur `/kitchen/triggers` :
- Le tableau affiche 7 lignes (seeds migration 021) pour le site de test
- Les labels sont lisibles : "Caisse / Sur place / Paiement / —", etc.
- Changer de site dans le sélecteur recharge les données
- Aucune erreur console

- [ ] **Step 3: Commit**

```bash
git add backoffice/src/pages/kitchen/KdsChannelTriggers.tsx
git commit -m "feat(backoffice): load and display kds_channel_triggers table"
```

---

## Task 3: Édition inline (selects + save per-ligne)

**Files:**
- Modify: `backoffice/src/pages/kitchen/KdsChannelTriggers.tsx`

**Interfaces:**
- Consumes: `triggers: ChannelTrigger[]`, `rowKey(t): TriggerKey`
- Produces: `edited` state, `handleChange`, `handleSave`, bouton "Enregistrer" visible quand dirty

- [ ] **Step 1: Ajouter les imports de state et les handlers**

Dans `KdsChannelTriggers.tsx`, ajouter les états suivants après `const [error, ...]` :

```tsx
const [edited, setEdited] = useState<Record<TriggerKey, ChannelTrigger>>({})
const [saving, setSaving] = useState<TriggerKey | null>(null)
```

Ajouter les helpers et handlers après `useEffect(load, [activeSiteId])` :

```tsx
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
```

- [ ] **Step 2: Remplacer les cellules DÉCLENCHE SUR et ORB par des selects**

Dans le `tbody`, remplacer les deux cellules en lecture seule par :

```tsx
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
```

- [ ] **Step 3: Ajouter le bouton Enregistrer dans la colonne ACTIONS**

Dans la cellule ACTIONS (actuellement vide), remplacer par :

```tsx
<td style={{ padding: '0.6rem 0.8rem' }}>
  {canWrite && isDirty(t) && (
    <button
      onClick={() => handleSave(rowKey(t))}
      disabled={saving === rowKey(t)}
      style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 4, padding: '0.3rem 0.7rem', cursor: 'pointer', fontSize: '0.8rem' }}
    >
      {saving === rowKey(t) ? '…' : 'Enregistrer'}
    </button>
  )}
</td>
```

- [ ] **Step 4: Vérifier en navigateur**

- Changer le select "Déclenche sur" d'une ligne → bouton "Enregistrer" apparaît
- Cliquer "Enregistrer" → le bouton affiche "…" puis disparaît
- Recharger la page → la valeur modifiée est persistée
- Utilisateur sans rôle `pos_admin`/`regional_director` → selects désactivés, pas de bouton

- [ ] **Step 5: Commit**

```bash
git add backoffice/src/pages/kitchen/KdsChannelTriggers.tsx
git commit -m "feat(backoffice): inline editing for kds_channel_triggers rows"
```

---

## Task 4: Suppression per-ligne

**Files:**
- Modify: `backoffice/src/pages/kitchen/KdsChannelTriggers.tsx`

**Interfaces:**
- Consumes: `canWrite`, `activeSiteId`, `load()`
- Produces: `deleting` state, `handleDelete`, bouton "Supprimer" per-ligne

- [ ] **Step 1: Ajouter le state `deleting`**

Après `const [saving, setSaving] = ...` :

```tsx
const [deleting, setDeleting] = useState<TriggerKey | null>(null)
```

- [ ] **Step 2: Ajouter le handler `handleDelete`**

Après `handleSave` :

```tsx
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
```

- [ ] **Step 3: Ajouter le bouton Supprimer dans la colonne ACTIONS**

Dans la cellule ACTIONS, après le bouton "Enregistrer" existant :

```tsx
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
```

Note : remplace la `<td>` ACTIONS entière (y compris le bouton "Enregistrer" de la tâche précédente) par ce bloc.

- [ ] **Step 4: Vérifier en navigateur**

- Cliquer "Supprimer" → dialog de confirmation apparaît avec le label lisible
- Annuler → rien ne change
- Confirmer → la ligne disparaît du tableau
- Pendant la suppression → le bouton affiche "…" et est désactivé

- [ ] **Step 5: Commit**

```bash
git add backoffice/src/pages/kitchen/KdsChannelTriggers.tsx
git commit -m "feat(backoffice): delete kds_channel_trigger with confirmation"
```

---

## Task 5: Formulaire d'ajout

**Files:**
- Modify: `backoffice/src/pages/kitchen/KdsChannelTriggers.tsx`

**Interfaces:**
- Consumes: `triggers` (pour calculer les combos libres), `canWrite`, `activeSiteId`, `load()`
- Produces: formulaire "+ Ajouter un déclencheur" avec selects contraints, `handleAdd`

- [ ] **Step 1: Ajouter les constantes des valeurs connues**

Après les déclarations de `TRIGGER_LABEL`, avant `export default` :

```tsx
const CHANNELS = ['caisse', 'kiosk', 'drive', 'delivery']
const ORDER_TYPES = ['eat_in', 'takeaway', 'drive', 'delivery', 'click_and_collect']
```

- [ ] **Step 2: Ajouter les états du formulaire**

Après `const [deleting, ...]` :

```tsx
const [newChannel, setNewChannel] = useState('')
const [newOrderType, setNewOrderType] = useState('')
const [newTriggerOn, setNewTriggerOn] = useState<'order' | 'payment' | 'both'>('order')
const [newOrbType, setNewOrbType] = useState<'' | 'client' | 'livreur'>('')
const [adding, setAdding] = useState(false)
```

- [ ] **Step 3: Ajouter les dérivations des combos disponibles**

Juste avant le `return` (après les handlers) :

```tsx
const takenKeys = new Set(triggers.map(rowKey))

const availableChannels = CHANNELS.filter(ch =>
  ORDER_TYPES.some(ot => !takenKeys.has(`${ch}:${ot}`))
)

const availableOrderTypes = newChannel
  ? ORDER_TYPES.filter(ot => !takenKeys.has(`${newChannel}:${ot}`))
  : []
```

- [ ] **Step 4: Ajouter le handler `handleAdd`**

Après `handleDelete` :

```tsx
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
```

- [ ] **Step 5: Ajouter le formulaire dans le JSX**

Après la balise fermante `</table>`, avant la balise fermante `</div>` principale :

```tsx
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
```

- [ ] **Step 6: Vérifier en navigateur**

- Le formulaire est visible pour `pos_admin` / `regional_director`
- Le select Canal ne liste que les canaux ayant au moins un type libre
- Choisir un canal → le select Type commande se remplit avec les combos disponibles
- Choisir un canal → changer de canal → le type commande se réinitialise
- Ajouter un déclencheur → il apparaît dans le tableau, le formulaire se réinitialise
- Supprimer toutes les lignes d'un canal → ce canal disparaît du select Canal
- Si toutes les 20 combos possibles sont prises → le formulaire disparaît entièrement

- [ ] **Step 7: Commit**

```bash
git add backoffice/src/pages/kitchen/KdsChannelTriggers.tsx
git commit -m "feat(backoffice): add form with constrained selects for kds_channel_triggers"
```
