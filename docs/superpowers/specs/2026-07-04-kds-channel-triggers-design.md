# Design — Page /kitchen/triggers (KdsChannelTriggers)

**Date:** 2026-07-04  
**Scope:** backoffice React19 uniquement — aucun impact sur edge-api, kds-engine, fiscal-engine  
**Statut:** Approuvé

---

## 1. Objectif

Créer la page `/kitchen/triggers` dans le backoffice pour permettre aux administrateurs de site de consulter et modifier les règles de déclenchement KDS par canal × type de commande (`kds_channel_triggers`).

---

## 2. Table cible

```sql
CREATE TABLE public.kds_channel_triggers (
    site_id    UUID NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    channel    TEXT NOT NULL,                               -- caisse | kiosk | drive | delivery
    order_type TEXT NOT NULL,                               -- eat_in | takeaway | drive | delivery | click_and_collect
    trigger_on TEXT NOT NULL CHECK (trigger_on IN ('order','payment','both')),
    orb_type   TEXT CHECK (orb_type IN ('client','livreur')), -- NULL = pas d'ORB
    PRIMARY KEY (site_id, channel, order_type)
);
```

RLS déjà en place :
- `kds_ct_read` : SELECT pour tout utilisateur authentifié avec accès au site
- `kds_ct_write` : ALL pour `pos_admin` et `regional_director`

Seeds par défaut (7 lignes) appliqués dans migration 021 pour chaque site existant.

---

## 3. Fichiers à créer / modifier

| Fichier | Action |
|---|---|
| `backoffice/src/pages/kitchen/KdsChannelTriggers.tsx` | Créer |
| `backoffice/src/App.tsx` | Ajouter route `/kitchen/triggers` |
| `backoffice/src/components/Layout.tsx` | Ajouter lien nav "🎯 Déclencheurs canal" |

---

## 4. Architecture du composant

### 4.1 Approche retenue

**Inline editing pur** — identique à `KdsTimerThresholds.tsx` :
- `<select>` directement dans les cellules du tableau
- Dirty detection par ligne → bouton "Enregistrer" per-ligne visible uniquement si modifié
- Formulaire d'ajout en bas de page (visible uniquement si des combinaisons libres existent)
- Suppression via `window.confirm()` (pattern de `KdsStations.tsx`)

### 4.2 Interface TypeScript

```ts
interface ChannelTrigger {
  channel: string
  order_type: string
  trigger_on: 'order' | 'payment' | 'both'
  orb_type: 'client' | 'livreur' | null
}

type TriggerKey = string  // `${channel}:${order_type}`
```

### 4.3 State

```ts
const [triggers, setTriggers] = useState<ChannelTrigger[]>([])
const [edited, setEdited] = useState<Record<TriggerKey, ChannelTrigger>>({})
const [saving, setSaving] = useState<TriggerKey | null>(null)
const [deleting, setDeleting] = useState<TriggerKey | null>(null)
const [loading, setLoading] = useState(true)
const [error, setError] = useState<string | null>(null)

// Add form
const [newChannel, setNewChannel] = useState('')
const [newOrderType, setNewOrderType] = useState('')
const [newTriggerOn, setNewTriggerOn] = useState<'order' | 'payment' | 'both'>('order')
const [newOrbType, setNewOrbType] = useState<'' | 'client' | 'livreur'>('')
const [adding, setAdding] = useState(false)
```

---

## 5. Données et mutations

### 5.1 Lecture

```ts
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
```

### 5.2 Save (upsert)

```ts
const handleSave = async (key: TriggerKey) => {
  const t = edited[key]; if (!t || !activeSiteId) return
  setSaving(key); setError(null)
  const { error: e } = await supabase
    .from('kds_channel_triggers')
    .upsert({ site_id: activeSiteId, ...t }, { onConflict: 'site_id,channel,order_type' })
  if (e) setError(e.message)
  else { setEdited(prev => { const n = { ...prev }; delete n[key]; return n }); load() }
  setSaving(null)
}
```

### 5.3 Delete

```ts
const handleDelete = async (key: TriggerKey, t: ChannelTrigger) => {
  if (!window.confirm(`Supprimer le trigger "${CHANNEL_LABEL[t.channel]} × ${ORDER_TYPE_LABEL[t.order_type]}" ?`)) return
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

Le bouton "Supprimer" est désactivé (`disabled={deleting === key}`) pendant la suppression pour éviter les doubles clics.

### 5.4 Add (insert)

```ts
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
  else { setNewChannel(''); setNewOrderType(''); setNewTriggerOn('order'); setNewOrbType(''); load() }
  setAdding(false)
}
```

---

## 6. Tableau principal

### 6.1 Colonnes

| En-tête | Contenu |
|---|---|
| CANAL | `CHANNEL_LABEL[t.channel]` |
| TYPE COMMANDE | `ORDER_TYPE_LABEL[t.order_type]` |
| DÉCLENCHE SUR | `<select>` : order / payment / both |
| ORB | `<select>` : — / client / livreur |
| ACTIONS | Bouton "Enregistrer" (si dirty) + bouton "Supprimer" |

### 6.2 Labels d'affichage

```ts
const CHANNEL_LABEL: Record<string, string> = {
  caisse: 'Caisse', kiosk: 'Kiosk', drive: 'Drive', delivery: 'Livraison',
}
const ORDER_TYPE_LABEL: Record<string, string> = {
  eat_in: 'Sur place', takeaway: 'À emporter', drive: 'Drive',
  delivery: 'Livraison', click_and_collect: 'Click & Collect',
}
const TRIGGER_LABEL: Record<string, string> = {
  order: 'Commande', payment: 'Paiement', both: 'Les deux',
}
```

### 6.3 Dirty detection

```ts
const rowKey = (t: ChannelTrigger) => `${t.channel}:${t.order_type}`
const getEdited = (t: ChannelTrigger) => edited[rowKey(t)] ?? t
const isDirty = (t: ChannelTrigger) => JSON.stringify(getEdited(t)) !== JSON.stringify(t)
```

### 6.4 Comportement des selects

- `disabled={!canWrite}` sur tous les selects de la table
- `orb_type` select : `<option value="">—</option>` pour NULL, `<option value="client">client</option>`, `<option value="livreur">livreur</option>`
- `onChange` met à jour `edited[key]` avec spread de la row existante ou éditée

---

## 7. Formulaire d'ajout

Affiché uniquement si `canWrite` et `availableCombos.length > 0`.

```ts
const CHANNELS = ['caisse', 'kiosk', 'drive', 'delivery']
const ORDER_TYPES = ['eat_in', 'takeaway', 'drive', 'delivery', 'click_and_collect']

// Combos déjà pris (en tenant compte des edits en cours)
const takenKeys = new Set(triggers.map(rowKey))

// Order types disponibles pour le canal sélectionné
const availableOrderTypes = ORDER_TYPES.filter(
  ot => !takenKeys.has(`${newChannel}:${ot}`)
)

// Canal select filtré : n'affiche que les canaux avec au moins un order_type libre
const availableChannels = CHANNELS.filter(
  ch => ORDER_TYPES.some(ot => !takenKeys.has(`${ch}:${ot}`))
)
```

- Quand `newChannel` change, réinitialiser `newOrderType` si la combo n'est plus disponible
- Bouton "Ajouter" désactivé si `!newChannel || !newOrderType || adding`

---

## 8. Routing et navigation

### App.tsx

Ajouter après la route `/kitchen/thresholds` :
```tsx
<Route path="/kitchen/triggers" element={<KdsChannelTriggers />} />
```

### Layout.tsx

Ajouter dans `navItems` après `{ to: '/kitchen/thresholds', label: '⏱ Seuils timer' }` :
```ts
{ to: '/kitchen/triggers', label: '🎯 Déclencheurs canal' },
```

---

## 9. Permissions

```ts
const { role } = useAuth()
const { activeSiteId } = useSite()
const canWrite = role === 'pos_admin' || role === 'regional_director'
```

Identique à toutes les pages kitchen. La RLS Supabase enforce côté serveur.

---

## 10. États de chargement / erreur

- `!activeSiteId` → `<p>Sélectionner un site</p>`
- `loading` → `<p>Chargement…</p>`
- `error` → bannière rouge sous le titre
- `triggers.length === 0` → ligne vide "Aucun trigger configuré"

---

## 11. Ce qui est explicitement hors scope

- Aucun test automatisé (le backoffice n'en a pas)
- Aucune modification de edge-api, kds-engine, fiscal-engine, sync-client
- Aucune migration Supabase (table déjà créée par 021)
- Pas de React Router interne (routing déjà dans App.tsx)
