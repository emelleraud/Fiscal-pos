# KDS Backoffice & Supabase — Implementation Plan (Plan 3/3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Créer les tables Supabase cloud KDS (migration 021), les pages backoffice CRUD pour configurer stations et règles de routage, et le module `kds_puller` dans sync-client qui pull la config cloud vers SQLite local.

**Architecture:** Les tables Supabase (`kds_station_configs`, `kds_routing_configs`, etc.) stockent la config par site avec RLS standard. Le backoffice lit/écrit ces tables via `supabase-js`. Le sync-client les pull vers les tables SQLite locales (`kds_stations`, `kds_routing_rules`, etc.) créées par Plan 1. La table `kds_active_profile` (profil actif local) n'est pas touchée par le pull — elle est gérée localement via la config page de Plan 2.

**Tech Stack:** Supabase PostgreSQL (migration SQL), React 19 + supabase-js (backoffice, même pattern que SiteList/SiteForm), Rust/sqlx (sync-client kds_puller).

**Plans connexes :**
- Plan 1 — `kds-engine` produit les tables SQLite locales ciblées par le pull
- Plan 2 — `kds-app` consomme les stations et routage via edge-api

**Spec de référence :** `docs/superpowers/specs/2026-07-02-kds-design.md` §10

## Global Constraints

- Migrations Supabase : préfixe `021_` (dernier appliqué : `020_fix_list_admin_users_casts.sql`)
- RLS : `pos_admin` / `regional_director` en écriture, `service_role` full access (pour sync-client)
- Backoffice : styles inline uniquement (pas de Tailwind ni styled-components), pattern SiteList/SiteForm
- Backoffice : filtrage par `activeSiteId` depuis `useSite()` — chaque site voit uniquement ses propres stations
- sync-client : pull uniquement — aucune donnée KDS n'est poussée vers Supabase
- Clippy pedantic : `#[allow(clippy::too_many_lines)]` si un handler dépasse 100 lignes
- **CI avant chaque commit :** `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace` (pour les commits Rust)

---

## Fichiers créés / modifiés

```
supabase/migrations/
  021_kds_cloud_tables.sql           ← tables kds_* cloud + RLS

backoffice/src/
  pages/kitchen/
    KdsStations.tsx                  ← liste stations + suppression
    KdsStationForm.tsx               ← create/edit station
    KdsRoutingRules.tsx              ← règles de routage par profil
    KdsTimerThresholds.tsx           ← seuils timer par station
  components/Layout.tsx              ← + section nav "Cuisine"
  App.tsx                            ← + routes /kitchen/*

sync-client/
  src/
    client.rs                        ← + get_rest_table() + pull_kds_config()
    kds_puller.rs                    ← pull KDS config Supabase → SQLite
    lib.rs                           ← + pub mod kds_puller
    sync_loop.rs                     ← + appel pull_kds_config dans le cycle
```

---

## Task 1 : Migration Supabase 021 — tables KDS cloud

**Files:**
- Create: `supabase/migrations/021_kds_cloud_tables.sql`

**Interfaces:**
- Produit: tables `kds_station_configs`, `kds_routing_profiles`, `kds_routing_configs`, `kds_channel_triggers`, `kds_timer_thresholds` — lues par le backoffice via anon/authenticated, lues par sync-client via service_role

- [ ] **Créer `supabase/migrations/021_kds_cloud_tables.sql`**

```sql
-- =============================================================================
-- 021_kds_cloud_tables.sql
-- Tables cloud KDS : config stations, profils, règles, déclencheurs, seuils.
-- Écrites depuis le backoffice (pos_admin / regional_director).
-- Lues par sync-client (service_role) pour pull → SQLite local.
-- =============================================================================

-- ---------------------------------------------------------------------------
-- kds_routing_profiles  (profils de routage par site)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.kds_routing_profiles (
    id      TEXT NOT NULL,
    site_id UUID NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    name    TEXT NOT NULL,
    description TEXT,
    PRIMARY KEY (site_id, id)
);

CREATE INDEX IF NOT EXISTS idx_kds_routing_profiles_site
    ON public.kds_routing_profiles(site_id);

ALTER TABLE public.kds_routing_profiles ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "kds_rp_read" ON public.kds_routing_profiles;
CREATE POLICY "kds_rp_read" ON public.kds_routing_profiles
    FOR SELECT TO authenticated USING (public.can_access_site(site_id));

DROP POLICY IF EXISTS "kds_rp_write" ON public.kds_routing_profiles;
CREATE POLICY "kds_rp_write" ON public.kds_routing_profiles
    FOR ALL TO authenticated
    USING    (public.auth_app_role() IN ('pos_admin','regional_director'))
    WITH CHECK (public.auth_app_role() IN ('pos_admin','regional_director'));

DROP POLICY IF EXISTS "kds_rp_service" ON public.kds_routing_profiles;
CREATE POLICY "kds_rp_service" ON public.kds_routing_profiles
    FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.kds_routing_profiles TO authenticated;

-- Seed profils normaux pour tous les sites existants
INSERT INTO public.kds_routing_profiles (id, site_id, name, description)
SELECT 'normal', id, 'Service normal', 'Stations polyvalentes'
FROM public.sites
ON CONFLICT DO NOTHING;

INSERT INTO public.kds_routing_profiles (id, site_id, name, description)
SELECT 'rush', id, 'Rush', 'Stations spécialisées, flux élevé'
FROM public.sites
ON CONFLICT DO NOTHING;

-- ---------------------------------------------------------------------------
-- kds_station_configs  (stations par site)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.kds_station_configs (
    id                  TEXT    NOT NULL,
    site_id             UUID    NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    name                TEXT    NOT NULL,
    role                TEXT    NOT NULL CHECK (role IN ('preparation','holding','assembly','ready_board')),
    temperature_group   TEXT    CHECK (temperature_group IN ('hot','cold','other')),
    output_type         TEXT    NOT NULL CHECK (output_type IN ('screen','printer','both')),
    printer_address     TEXT,
    printer_type        TEXT    CHECK (printer_type IN ('tcpip','usb','file')),
    printer_mode        TEXT    CHECK (printer_mode IN ('receipt','linerless_label')),
    paper_width_mm      INTEGER CHECK (paper_width_mm IN (50, 80)),
    fallback_station_id TEXT,
    active_in_profiles  TEXT    NOT NULL DEFAULT '["normal"]',
    sort_order          INTEGER NOT NULL DEFAULT 0,
    enabled             INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (site_id, id)
);

CREATE INDEX IF NOT EXISTS idx_kds_station_configs_site
    ON public.kds_station_configs(site_id);

ALTER TABLE public.kds_station_configs ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "kds_sc_read" ON public.kds_station_configs;
CREATE POLICY "kds_sc_read" ON public.kds_station_configs
    FOR SELECT TO authenticated USING (public.can_access_site(site_id));

DROP POLICY IF EXISTS "kds_sc_write" ON public.kds_station_configs;
CREATE POLICY "kds_sc_write" ON public.kds_station_configs
    FOR ALL TO authenticated
    USING    (public.auth_app_role() IN ('pos_admin','regional_director'))
    WITH CHECK (public.auth_app_role() IN ('pos_admin','regional_director'));

DROP POLICY IF EXISTS "kds_sc_service" ON public.kds_station_configs;
CREATE POLICY "kds_sc_service" ON public.kds_station_configs
    FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.kds_station_configs TO authenticated;

-- ---------------------------------------------------------------------------
-- kds_routing_configs  (règles de routage par site + profil)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.kds_routing_configs (
    id          TEXT    NOT NULL,
    site_id     UUID    NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    profile_id  TEXT    NOT NULL,
    rule_type   TEXT    NOT NULL CHECK (rule_type IN ('category','product','tag')),
    match_value TEXT    NOT NULL,
    station_ids TEXT    NOT NULL,
    priority    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (site_id, id)
);

CREATE INDEX IF NOT EXISTS idx_kds_routing_configs_site
    ON public.kds_routing_configs(site_id);

ALTER TABLE public.kds_routing_configs ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "kds_rc_read" ON public.kds_routing_configs;
CREATE POLICY "kds_rc_read" ON public.kds_routing_configs
    FOR SELECT TO authenticated USING (public.can_access_site(site_id));

DROP POLICY IF EXISTS "kds_rc_write" ON public.kds_routing_configs;
CREATE POLICY "kds_rc_write" ON public.kds_routing_configs
    FOR ALL TO authenticated
    USING    (public.auth_app_role() IN ('pos_admin','regional_director'))
    WITH CHECK (public.auth_app_role() IN ('pos_admin','regional_director'));

DROP POLICY IF EXISTS "kds_rc_service" ON public.kds_routing_configs;
CREATE POLICY "kds_rc_service" ON public.kds_routing_configs
    FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.kds_routing_configs TO authenticated;

-- ---------------------------------------------------------------------------
-- kds_channel_triggers  (déclencheurs KDS par canal × order_type × site)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.kds_channel_triggers (
    site_id    UUID NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    channel    TEXT NOT NULL,
    order_type TEXT NOT NULL,
    trigger_on TEXT NOT NULL CHECK (trigger_on IN ('order','payment','both')),
    orb_type   TEXT CHECK (orb_type IN ('client','livreur')),
    PRIMARY KEY (site_id, channel, order_type)
);

CREATE INDEX IF NOT EXISTS idx_kds_channel_triggers_site
    ON public.kds_channel_triggers(site_id);

ALTER TABLE public.kds_channel_triggers ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "kds_ct_read" ON public.kds_channel_triggers;
CREATE POLICY "kds_ct_read" ON public.kds_channel_triggers
    FOR SELECT TO authenticated USING (public.can_access_site(site_id));

DROP POLICY IF EXISTS "kds_ct_write" ON public.kds_channel_triggers;
CREATE POLICY "kds_ct_write" ON public.kds_channel_triggers
    FOR ALL TO authenticated
    USING    (public.auth_app_role() IN ('pos_admin','regional_director'))
    WITH CHECK (public.auth_app_role() IN ('pos_admin','regional_director'));

DROP POLICY IF EXISTS "kds_ct_service" ON public.kds_channel_triggers;
CREATE POLICY "kds_ct_service" ON public.kds_channel_triggers
    FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.kds_channel_triggers TO authenticated;

-- Seed déclencheurs par défaut pour les sites existants
INSERT INTO public.kds_channel_triggers (site_id, channel, order_type, trigger_on, orb_type)
SELECT s.id, c.channel, c.order_type, c.trigger_on, c.orb_type
FROM public.sites s
CROSS JOIN (VALUES
    ('caisse',   'eat_in',          'payment', NULL),
    ('caisse',   'takeaway',        'payment', 'client'),
    ('kiosk',    'eat_in',          'order',   NULL),
    ('kiosk',    'takeaway',        'order',   'client'),
    ('drive',    'drive',           'payment', NULL),
    ('delivery', 'delivery',        'order',   'livreur'),
    ('delivery', 'click_and_collect','order',  'client')
) AS c(channel, order_type, trigger_on, orb_type)
ON CONFLICT DO NOTHING;

-- ---------------------------------------------------------------------------
-- kds_timer_thresholds  (seuils timer par station × site)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.kds_timer_thresholds (
    site_id       UUID    NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
    station_id    TEXT    NOT NULL,
    warning_secs  INTEGER NOT NULL DEFAULT 120,
    critical_secs INTEGER NOT NULL DEFAULT 300,
    PRIMARY KEY (site_id, station_id)
);

CREATE INDEX IF NOT EXISTS idx_kds_timer_thresholds_site
    ON public.kds_timer_thresholds(site_id);

ALTER TABLE public.kds_timer_thresholds ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "kds_tt_read" ON public.kds_timer_thresholds;
CREATE POLICY "kds_tt_read" ON public.kds_timer_thresholds
    FOR SELECT TO authenticated USING (public.can_access_site(site_id));

DROP POLICY IF EXISTS "kds_tt_write" ON public.kds_timer_thresholds;
CREATE POLICY "kds_tt_write" ON public.kds_timer_thresholds
    FOR ALL TO authenticated
    USING    (public.auth_app_role() IN ('pos_admin','regional_director'))
    WITH CHECK (public.auth_app_role() IN ('pos_admin','regional_director'));

DROP POLICY IF EXISTS "kds_tt_service" ON public.kds_timer_thresholds;
CREATE POLICY "kds_tt_service" ON public.kds_timer_thresholds
    FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.kds_timer_thresholds TO authenticated;
```

- [ ] **Appliquer la migration via Supabase CLI**

```bash
supabase db push
```
Attendu : `Applied migration 021_kds_cloud_tables.sql` sans erreur.

Si Supabase CLI non disponible, appliquer manuellement via le Dashboard SQL Editor.

- [ ] **Commit**

```bash
git add supabase/migrations/021_kds_cloud_tables.sql
git commit -m "feat(supabase): migration 021 — tables KDS cloud + RLS"
```

---

## Task 2 : Page `KdsStations` (liste + suppression)

**Files:**
- Create: `backoffice/src/pages/kitchen/KdsStations.tsx`

**Interfaces:**
- Consomme: `supabase` (supabaseClient.ts), `useSite()` (SiteContext), `useAuth()` (AuthContext)
- Produit: `KdsStations` — liste des stations pour `activeSiteId`, avec liens Éditer et bouton Supprimer

- [ ] **Créer `backoffice/src/pages/kitchen/KdsStations.tsx`**

```typescript
import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { supabase } from '../../supabaseClient'
import { useAuth } from '../../context/AuthContext'
import { useSite } from '../../context/SiteContext'

interface KdsStation {
  id: string
  name: string
  role: string
  output_type: string
  enabled: number
  sort_order: number
}

export default function KdsStations() {
  const [stations, setStations] = useState<KdsStation[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()
  const { role } = useAuth()
  const { activeSiteId } = useSite()
  const canWrite = role === 'pos_admin' || role === 'regional_director'

  const load = () => {
    if (!activeSiteId) { setLoading(false); return }
    setLoading(true)
    supabase
      .from('kds_station_configs')
      .select('id,name,role,output_type,enabled,sort_order')
      .eq('site_id', activeSiteId)
      .order('sort_order')
      .then(({ data, error: e }) => {
        if (e) setError(e.message)
        else setStations((data as KdsStation[]) ?? [])
        setLoading(false)
      })
  }

  useEffect(load, [activeSiteId])

  const handleDelete = async (id: string, name: string) => {
    if (!window.confirm(`Supprimer la station "${name}" ?`)) return
    const { error: e } = await supabase
      .from('kds_station_configs')
      .delete()
      .eq('site_id', activeSiteId!)
      .eq('id', id)
    if (e) { setError(e.message); return }
    load()
  }

  if (!activeSiteId) return <p style={{ padding: '1.5rem', color: '#888' }}>Sélectionner un site</p>
  if (loading) return <p style={{ padding: '1.5rem', color: '#888' }}>Chargement…</p>

  const ROLE_LABEL: Record<string, string> = {
    preparation: 'Préparation',
    holding: 'Rassemblement',
    assembly: 'Assemblage Expo',
    ready_board: 'ORB',
  }

  return (
    <div style={{ padding: '1.5rem' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0 }}>
          Stations cuisine{' '}
          <span style={{ color: '#888', fontWeight: 400, fontSize: '0.9rem' }}>{stations.length} station(s)</span>
        </h2>
        {canWrite && (
          <button
            onClick={() => navigate('/kitchen/stations/new')}
            style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}
          >
            + Nouvelle station
          </button>
        )}
      </div>

      {error && <p style={{ color: '#e53e3e', marginBottom: '1rem' }}>{error}</p>}

      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['ID', 'NOM', 'RÔLE', 'OUTPUT', 'ÉTAT', 'ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {stations.map(s => (
            <tr key={s.id} style={{ borderBottom: '1px solid #f0f0f0' }}>
              <td style={{ padding: '0.7rem 0.8rem', fontFamily: 'monospace', fontSize: '0.85rem', color: '#666' }}>{s.id}</td>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500 }}>{s.name}</td>
              <td style={{ padding: '0.7rem 0.8rem', fontSize: '0.85rem' }}>{ROLE_LABEL[s.role] ?? s.role}</td>
              <td style={{ padding: '0.7rem 0.8rem', fontSize: '0.85rem' }}>{s.output_type}</td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                <span style={{
                  display: 'inline-block', padding: '0.2rem 0.5rem', borderRadius: 4,
                  fontSize: '0.75rem', fontWeight: 600,
                  background: s.enabled ? '#d4edda' : '#f8d7da',
                  color: s.enabled ? '#155724' : '#721c24',
                }}>
                  {s.enabled ? 'Actif' : 'Inactif'}
                </span>
              </td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                {canWrite && (
                  <>
                    <Link to={`/kitchen/stations/${s.id}`} style={{ color: '#4f8ef7', textDecoration: 'none', marginRight: 12 }}>
                      Éditer
                    </Link>
                    <button
                      onClick={() => handleDelete(s.id, s.name)}
                      style={{ background: 'none', border: 'none', color: '#e53e3e', cursor: 'pointer', fontSize: '0.85rem', padding: 0 }}
                    >
                      Supprimer
                    </button>
                  </>
                )}
              </td>
            </tr>
          ))}
          {stations.length === 0 && (
            <tr>
              <td colSpan={6} style={{ padding: '2rem', textAlign: 'center', color: '#888' }}>
                Aucune station configurée. Créez la première station.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  )
}
```

- [ ] **Vérifier TypeScript**

```bash
cd backoffice && npm run build 2>&1 | grep -E "error|Error" | head -10
```
Attendu : 0 erreurs.

- [ ] **Commit**

```bash
git add backoffice/src/pages/kitchen/KdsStations.tsx
git commit -m "feat(backoffice): KdsStations — liste stations avec CRUD"
```

---

## Task 3 : `KdsStationForm` (create / edit)

**Files:**
- Create: `backoffice/src/pages/kitchen/KdsStationForm.tsx`

**Interfaces:**
- Consomme: `supabase`, `useSite()`, `useParams`, `useNavigate`
- Produit: `KdsStationForm` — formulaire create/edit complet (rôle, output, imprimante, profils actifs, fallback)

- [ ] **Créer `backoffice/src/pages/kitchen/KdsStationForm.tsx`**

```typescript
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
```

- [ ] **Vérifier TypeScript**

```bash
cd backoffice && npm run build 2>&1 | grep -E "error|Error" | head -10
```
Attendu : 0 erreurs.

- [ ] **Commit**

```bash
git add backoffice/src/pages/kitchen/KdsStationForm.tsx
git commit -m "feat(backoffice): KdsStationForm — create/edit station cuisine"
```

---

## Task 4 : `KdsRoutingRules`

**Files:**
- Create: `backoffice/src/pages/kitchen/KdsRoutingRules.tsx`

**Interfaces:**
- Consomme: `supabase`, `useSite()`
- Produit: `KdsRoutingRules` — liste des règles groupées par profil, CRUD en ligne (ajout + suppression)

- [ ] **Créer `backoffice/src/pages/kitchen/KdsRoutingRules.tsx`**

```typescript
import { useEffect, useState } from 'react'
import { supabase } from '../../supabaseClient'
import { useAuth } from '../../context/AuthContext'
import { useSite } from '../../context/SiteContext'

interface RoutingProfile { id: string; name: string }
interface RoutingRule {
  id: string
  profile_id: string
  rule_type: string
  match_value: string
  station_ids: string
  priority: number
}

const inputStyle = { padding: '0.35rem 0.6rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem' }
const selectStyle = { ...inputStyle }

export default function KdsRoutingRules() {
  const [profiles, setProfiles] = useState<RoutingProfile[]>([])
  const [rules, setRules] = useState<RoutingRule[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const [newProfileId, setNewProfileId] = useState('')
  const [newRuleType, setNewRuleType] = useState('category')
  const [newMatchValue, setNewMatchValue] = useState('')
  const [newStationIds, setNewStationIds] = useState('')
  const [newPriority, setNewPriority] = useState('0')
  const [adding, setAdding] = useState(false)

  const { role } = useAuth()
  const { activeSiteId } = useSite()
  const canWrite = role === 'pos_admin' || role === 'regional_director'

  const load = () => {
    if (!activeSiteId) { setLoading(false); return }
    setLoading(true)
    Promise.all([
      supabase.from('kds_routing_profiles').select('id,name').eq('site_id', activeSiteId).order('id'),
      supabase.from('kds_routing_configs').select('*').eq('site_id', activeSiteId).order('priority', { ascending: false }),
    ]).then(([pRes, rRes]) => {
      if (pRes.error) setError(pRes.error.message)
      else setProfiles((pRes.data as RoutingProfile[]) ?? [])
      if (rRes.error) setError(e => e ?? rRes.error!.message)
      else setRules((rRes.data as RoutingRule[]) ?? [])
      setLoading(false)
      if (!newProfileId && (pRes.data as RoutingProfile[])?.[0]) {
        setNewProfileId((pRes.data as RoutingProfile[])[0].id)
      }
    })
  }

  useEffect(load, [activeSiteId])

  const handleAdd = async () => {
    if (!activeSiteId || !newMatchValue.trim() || !newStationIds.trim()) return
    setAdding(true); setError(null)
    // station_ids stocké en JSON array
    const stationArr = newStationIds.split(',').map(s => s.trim()).filter(Boolean)
    const id = `${newProfileId}-${newRuleType}-${newMatchValue.trim()}-${Date.now()}`
    const { error: e } = await supabase.from('kds_routing_configs').insert({
      id,
      site_id: activeSiteId,
      profile_id: newProfileId,
      rule_type: newRuleType,
      match_value: newMatchValue.trim(),
      station_ids: JSON.stringify(stationArr),
      priority: parseInt(newPriority, 10) || 0,
    })
    if (e) setError(e.message)
    else { setNewMatchValue(''); setNewStationIds(''); setNewPriority('0'); load() }
    setAdding(false)
  }

  const handleDelete = async (id: string) => {
    if (!window.confirm('Supprimer cette règle ?')) return
    const { error: e } = await supabase.from('kds_routing_configs')
      .delete().eq('site_id', activeSiteId!).eq('id', id)
    if (e) setError(e.message)
    else load()
  }

  if (!activeSiteId) return <p style={{ padding: '1.5rem', color: '#888' }}>Sélectionner un site</p>
  if (loading) return <p style={{ padding: '1.5rem', color: '#888' }}>Chargement…</p>

  const RULE_TYPE_LABEL: Record<string, string> = { category: 'Catégorie', product: 'Produit', tag: 'Tag' }

  return (
    <div style={{ padding: '1.5rem' }}>
      <h2 style={{ marginTop: 0 }}>Règles de routage cuisine</h2>
      {error && <p style={{ color: '#e53e3e', marginBottom: '1rem' }}>{error}</p>}

      {profiles.map(profile => {
        const profileRules = rules.filter(r => r.profile_id === profile.id)
        return (
          <div key={profile.id} style={{ marginBottom: '2rem' }}>
            <h3 style={{ color: '#4f8ef7', marginBottom: '0.5rem', textTransform: 'uppercase', fontSize: '0.9rem' }}>
              Profil : {profile.name}
            </h3>
            <table style={{ width: '100%', borderCollapse: 'collapse', marginBottom: '0.5rem' }}>
              <thead>
                <tr style={{ background: '#f5f6fa' }}>
                  {['TYPE', 'VALEUR', 'STATIONS CIBLES', 'PRIORITÉ', 'ACTIONS'].map(h => (
                    <th key={h} style={{ textAlign: 'left', padding: '0.5rem 0.7rem', fontSize: '0.75rem', color: '#666' }}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {profileRules.map(r => (
                  <tr key={r.id} style={{ borderBottom: '1px solid #f0f0f0' }}>
                    <td style={{ padding: '0.6rem 0.7rem', fontSize: '0.85rem' }}>{RULE_TYPE_LABEL[r.rule_type] ?? r.rule_type}</td>
                    <td style={{ padding: '0.6rem 0.7rem', fontFamily: 'monospace', fontSize: '0.85rem' }}>{r.match_value}</td>
                    <td style={{ padding: '0.6rem 0.7rem', fontSize: '0.8rem', color: '#555' }}>
                      {(JSON.parse(r.station_ids) as string[]).join(', ')}
                    </td>
                    <td style={{ padding: '0.6rem 0.7rem', fontSize: '0.85rem' }}>{r.priority}</td>
                    <td style={{ padding: '0.6rem 0.7rem' }}>
                      {canWrite && (
                        <button
                          onClick={() => handleDelete(r.id)}
                          style={{ background: 'none', border: 'none', color: '#e53e3e', cursor: 'pointer', fontSize: '0.85rem', padding: 0 }}
                        >
                          Supprimer
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
                {profileRules.length === 0 && (
                  <tr>
                    <td colSpan={5} style={{ padding: '1rem', color: '#aaa', fontSize: '0.85rem' }}>
                      Aucune règle pour ce profil
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        )
      })}

      {canWrite && (
        <div style={{ background: '#f9f9fb', border: '1px solid #eee', borderRadius: 8, padding: '1rem' }}>
          <h4 style={{ marginTop: 0, marginBottom: '0.75rem', fontSize: '0.9rem' }}>+ Ajouter une règle</h4>
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', alignItems: 'flex-end' }}>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Profil</label>
              <select style={selectStyle} value={newProfileId} onChange={e => setNewProfileId(e.target.value)}>
                {profiles.map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
              </select>
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Type</label>
              <select style={selectStyle} value={newRuleType} onChange={e => setNewRuleType(e.target.value)}>
                <option value="category">Catégorie</option>
                <option value="product">Produit (SKU)</option>
                <option value="tag">Tag</option>
              </select>
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Valeur</label>
              <input style={{ ...inputStyle, width: 140 }} value={newMatchValue}
                onChange={e => setNewMatchValue(e.target.value)} placeholder="Burgers" />
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Stations (IDs, virgule)</label>
              <input style={{ ...inputStyle, width: 180 }} value={newStationIds}
                onChange={e => setNewStationIds(e.target.value)} placeholder="grill-01, grill-02" />
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Priorité</label>
              <input style={{ ...inputStyle, width: 60 }} type="number" value={newPriority}
                onChange={e => setNewPriority(e.target.value)} min="0" />
            </div>
            <button
              onClick={handleAdd}
              disabled={adding || !newMatchValue.trim() || !newStationIds.trim()}
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
```

- [ ] **Vérifier TypeScript**

```bash
cd backoffice && npm run build 2>&1 | grep -E "error|Error" | head -10
```
Attendu : 0 erreurs.

- [ ] **Commit**

```bash
git add backoffice/src/pages/kitchen/KdsRoutingRules.tsx
git commit -m "feat(backoffice): KdsRoutingRules — règles de routage par profil"
```

---

## Task 5 : `KdsTimerThresholds`

**Files:**
- Create: `backoffice/src/pages/kitchen/KdsTimerThresholds.tsx`

**Interfaces:**
- Consomme: `supabase`, `useSite()`
- Produit: `KdsTimerThresholds` — seuils warning/critical par station, édition inline

- [ ] **Créer `backoffice/src/pages/kitchen/KdsTimerThresholds.tsx`**

```typescript
import { useEffect, useState } from 'react'
import { supabase } from '../../supabaseClient'
import { useAuth } from '../../context/AuthContext'
import { useSite } from '../../context/SiteContext'

interface Threshold {
  station_id: string
  warning_secs: number
  critical_secs: number
}

interface Station { id: string; name: string }

const inputStyle = { padding: '0.3rem 0.5rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem', width: 70 }

export default function KdsTimerThresholds() {
  const [thresholds, setThresholds] = useState<Threshold[]>([])
  const [stations, setStations] = useState<Station[]>([])
  const [edited, setEdited] = useState<Record<string, Threshold>>({})
  const [saving, setSaving] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [newStationId, setNewStationId] = useState('')
  const [newWarn, setNewWarn] = useState('120')
  const [newCrit, setNewCrit] = useState('300')
  const [adding, setAdding] = useState(false)

  const { role } = useAuth()
  const { activeSiteId } = useSite()
  const canWrite = role === 'pos_admin' || role === 'regional_director'

  const load = () => {
    if (!activeSiteId) { setLoading(false); return }
    setLoading(true)
    Promise.all([
      supabase.from('kds_timer_thresholds').select('*').eq('site_id', activeSiteId).order('station_id'),
      supabase.from('kds_station_configs').select('id,name').eq('site_id', activeSiteId).order('sort_order'),
    ]).then(([tRes, sRes]) => {
      if (tRes.error) setError(tRes.error.message)
      else setThresholds((tRes.data as Threshold[]) ?? [])
      if (sRes.error) setError(e => e ?? sRes.error!.message)
      else setStations((sRes.data as Station[]) ?? [])
      setLoading(false)
    })
  }

  useEffect(load, [activeSiteId])

  const getEdited = (t: Threshold): Threshold => edited[t.station_id] ?? t

  const handleChange = (stationId: string, field: 'warning_secs' | 'critical_secs', val: string) => {
    const num = parseInt(val, 10)
    if (isNaN(num)) return
    setEdited(prev => ({
      ...prev,
      [stationId]: { ...(edited[stationId] ?? thresholds.find(t => t.station_id === stationId)!), [field]: num },
    }))
  }

  const handleSave = async (stationId: string) => {
    const t = edited[stationId]
    if (!t || !activeSiteId) return
    setSaving(stationId); setError(null)
    const { error: e } = await supabase.from('kds_timer_thresholds')
      .upsert({ site_id: activeSiteId, ...t }, { onConflict: 'site_id,station_id' })
    if (e) setError(e.message)
    else { setEdited(prev => { const next = { ...prev }; delete next[stationId]; return next }); load() }
    setSaving(null)
  }

  const handleAdd = async () => {
    if (!activeSiteId || !newStationId.trim()) return
    setAdding(true); setError(null)
    const { error: e } = await supabase.from('kds_timer_thresholds').insert({
      site_id: activeSiteId,
      station_id: newStationId.trim(),
      warning_secs: parseInt(newWarn, 10) || 120,
      critical_secs: parseInt(newCrit, 10) || 300,
    })
    if (e) setError(e.message)
    else { setNewStationId(''); setNewWarn('120'); setNewCrit('300'); load() }
    setAdding(false)
  }

  if (!activeSiteId) return <p style={{ padding: '1.5rem', color: '#888' }}>Sélectionner un site</p>
  if (loading) return <p style={{ padding: '1.5rem', color: '#888' }}>Chargement…</p>

  const stationName = (id: string) => stations.find(s => s.id === id)?.name ?? id

  return (
    <div style={{ padding: '1.5rem' }}>
      <h2 style={{ marginTop: 0 }}>Seuils timer par station</h2>
      <p style={{ color: '#666', fontSize: '0.85rem', marginBottom: '1rem' }}>
        Vert &lt; warning, orange &lt; critical, rouge au-delà. Valeurs en secondes.
      </p>
      {error && <p style={{ color: '#e53e3e', marginBottom: '1rem' }}>{error}</p>}

      <table style={{ width: '100%', borderCollapse: 'collapse', marginBottom: '1.5rem' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['STATION', 'WARNING (s)', 'CRITICAL (s)', 'ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {thresholds.map(t => {
            const e = getEdited(t)
            const dirty = JSON.stringify(e) !== JSON.stringify(t)
            return (
              <tr key={t.station_id} style={{ borderBottom: '1px solid #f0f0f0' }}>
                <td style={{ padding: '0.6rem 0.8rem', fontWeight: 500 }}>{stationName(t.station_id)}</td>
                <td style={{ padding: '0.6rem 0.8rem' }}>
                  <input style={inputStyle} type="number" value={e.warning_secs} min="10"
                    onChange={ev => handleChange(t.station_id, 'warning_secs', ev.target.value)}
                    disabled={!canWrite} />
                </td>
                <td style={{ padding: '0.6rem 0.8rem' }}>
                  <input style={inputStyle} type="number" value={e.critical_secs} min="10"
                    onChange={ev => handleChange(t.station_id, 'critical_secs', ev.target.value)}
                    disabled={!canWrite} />
                </td>
                <td style={{ padding: '0.6rem 0.8rem' }}>
                  {canWrite && dirty && (
                    <button
                      onClick={() => handleSave(t.station_id)}
                      disabled={saving === t.station_id}
                      style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 4, padding: '0.3rem 0.7rem', cursor: 'pointer', fontSize: '0.8rem' }}
                    >
                      {saving === t.station_id ? '…' : 'Enregistrer'}
                    </button>
                  )}
                </td>
              </tr>
            )
          })}
          {thresholds.length === 0 && (
            <tr>
              <td colSpan={4} style={{ padding: '1.5rem', textAlign: 'center', color: '#aaa', fontSize: '0.85rem' }}>
                Aucun seuil configuré
              </td>
            </tr>
          )}
        </tbody>
      </table>

      {canWrite && stations.length > 0 && (
        <div style={{ background: '#f9f9fb', border: '1px solid #eee', borderRadius: 8, padding: '1rem' }}>
          <h4 style={{ marginTop: 0, marginBottom: '0.75rem', fontSize: '0.9rem' }}>+ Ajouter un seuil</h4>
          <div style={{ display: 'flex', gap: 8, alignItems: 'flex-end' }}>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Station</label>
              <select style={{ padding: '0.35rem 0.6rem', border: '1px solid #ddd', borderRadius: 4, fontSize: '0.85rem' }}
                value={newStationId} onChange={e => setNewStationId(e.target.value)}>
                <option value="">— Choisir —</option>
                {stations.filter(s => !thresholds.some(t => t.station_id === s.id)).map(s => (
                  <option key={s.id} value={s.id}>{s.name}</option>
                ))}
              </select>
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Warning (s)</label>
              <input style={inputStyle} type="number" value={newWarn} onChange={e => setNewWarn(e.target.value)} min="10" />
            </div>
            <div>
              <label style={{ display: 'block', fontSize: '0.75rem', color: '#666', marginBottom: 3 }}>Critical (s)</label>
              <input style={inputStyle} type="number" value={newCrit} onChange={e => setNewCrit(e.target.value)} min="10" />
            </div>
            <button
              onClick={handleAdd}
              disabled={adding || !newStationId}
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
```

- [ ] **Vérifier TypeScript**

```bash
cd backoffice && npm run build 2>&1 | grep -E "error|Error" | head -10
```
Attendu : 0 erreurs.

- [ ] **Commit**

```bash
git add backoffice/src/pages/kitchen/KdsTimerThresholds.tsx
git commit -m "feat(backoffice): KdsTimerThresholds — seuils timer éditables par station"
```

---

## Task 6 : Navigation + Routes (Layout.tsx + App.tsx)

**Files:**
- Modify: `backoffice/src/components/Layout.tsx`
- Modify: `backoffice/src/App.tsx`

**Interfaces:**
- Produit: routes `/kitchen/stations`, `/kitchen/stations/new`, `/kitchen/stations/:id`, `/kitchen/routing`, `/kitchen/thresholds` accessibles via la nav "Cuisine"

- [ ] **Modifier `backoffice/src/components/Layout.tsx`**

Trouver le tableau `navItems` (ligne ~8) et ajouter la section Cuisine après `{ divider: true, label: 'Marketing' }` :

```typescript
// Ajouter après la ligne { to: '/promotions', label: '🏷️ Promotions' } :
{ divider: true, label: 'Cuisine' },
{ to: '/kitchen/stations',   label: '📺 Stations KDS' },
{ to: '/kitchen/routing',    label: '🔀 Règles routage' },
{ to: '/kitchen/thresholds', label: '⏱ Seuils timer' },
```

La liste complète `navItems` devient :
```typescript
const navItems: NavItem[] = [
  { to: '/dashboard',          label: '📊 Dashboard CA' },
  { to: '/fiscal-journal',     label: '📋 Journal fiscal' },
  { to: '/z-reports',          label: '🧾 Rapports Z' },
  { divider: true, label: 'Catalogue' },
  { to: '/categories',         label: '🏷️ Catégories' },
  { to: '/products',           label: '🍔 Produits' },
  { to: '/combos',             label: '🎁 Combos' },
  { to: '/modifiers',          label: '🔧 Modificateurs' },
  { divider: true, label: 'Config site' },
  { to: '/menu',               label: '📤 Config publiée' },
  { divider: true, label: 'Marketing' },
  { to: '/promotions',         label: '🏷️ Promotions' },
  { divider: true, label: 'Cuisine' },
  { to: '/kitchen/stations',   label: '📺 Stations KDS' },
  { to: '/kitchen/routing',    label: '🔀 Règles routage' },
  { to: '/kitchen/thresholds', label: '⏱ Seuils timer' },
]
```

- [ ] **Modifier `backoffice/src/App.tsx`**

Ajouter les imports en haut du fichier (après les imports existants) :
```typescript
import KdsStations from './pages/kitchen/KdsStations'
import KdsStationForm from './pages/kitchen/KdsStationForm'
import KdsRoutingRules from './pages/kitchen/KdsRoutingRules'
import KdsTimerThresholds from './pages/kitchen/KdsTimerThresholds'
```

Dans le bloc `<Route element={<Layout />}>`, ajouter avant la fermeture `</Route>` :
```typescript
<Route path="/kitchen/stations"          element={<KdsStations />} />
<Route path="/kitchen/stations/new"      element={<KdsStationForm />} />
<Route path="/kitchen/stations/:id"      element={<KdsStationForm />} />
<Route path="/kitchen/routing"           element={<KdsRoutingRules />} />
<Route path="/kitchen/thresholds"        element={<KdsTimerThresholds />} />
```

- [ ] **Vérifier le build complet**

```bash
cd backoffice && npm run build 2>&1
```
Attendu : `✓ built` sans erreur TypeScript.

- [ ] **Commit**

```bash
git add backoffice/src/components/Layout.tsx backoffice/src/App.tsx
git commit -m "feat(backoffice): navigation Cuisine + routes /kitchen/* (stations, routing, thresholds)"
```

---

## Task 7 : sync-client — `client.rs` + `kds_puller.rs`

**Files:**
- Modify: `sync-client/src/client.rs`
- Create: `sync-client/src/kds_puller.rs`
- Modify: `sync-client/src/lib.rs`

**Interfaces:**
- Consomme: `SupabaseClient` (client.rs), `SyncConfig` (config.rs), `SyncError` (error.rs), `SqlitePool` (sqlx)
- Produit:
  - `SupabaseClient::pull_kds_config(site_id: &str) -> Result<KdsCloudData, SyncError>` — 5 GET parallèles
  - `pull_kds_config(client, config, pool) -> Result<usize, SyncError>` — pull + upsert SQLite
  - `KdsCloudData { stations, profiles, rules, triggers, thresholds: Vec<Value> }`

- [ ] **Modifier `sync-client/src/client.rs`**

Ajouter après les imports existants :
```rust
pub use serde_json::Value as JsonValue;
```

Ajouter à la fin de l'impl `SupabaseClient` (avant la section `#[cfg(test)]`) :

```rust
    // -----------------------------------------------------------------------
    // Pull config KDS
    // -----------------------------------------------------------------------

    /// Données KDS cloud agrégées pour un site.
    pub struct KdsCloudData {
        pub stations: Vec<serde_json::Value>,
        pub profiles: Vec<serde_json::Value>,
        pub rules: Vec<serde_json::Value>,
        pub triggers: Vec<serde_json::Value>,
        pub thresholds: Vec<serde_json::Value>,
    }

    /// Helper GET générique — retourne `Vec<Value>` ou vide si la table est vide / inaccessible.
    async fn get_rest_table(&self, endpoint: &str) -> Result<Vec<serde_json::Value>, SyncError> {
        let url = format!("{}/rest/v1/{}", self.base_url, endpoint);
        let resp = self
            .client
            .get(&url)
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .header("Accept", "application/json")
            .send()
            .await?;

        if !resp.status().is_success() {
            warn!(
                status = %resp.status(),
                endpoint = %endpoint,
                "get_rest_table: HTTP error — retour vide"
            );
            return Ok(vec![]);
        }

        Ok(resp.json().await?)
    }

    /// Récupère en parallèle toutes les tables KDS cloud pour ce site.
    ///
    /// # Errors
    /// `SyncError::Network` si Supabase est inaccessible.
    pub async fn pull_kds_config(&self, site_id: &str) -> Result<KdsCloudData, SyncError> {
        let (stations, profiles, rules, triggers, thresholds) = tokio::try_join!(
            self.get_rest_table(&format!(
                "kds_station_configs?site_id=eq.{site_id}&select=*"
            )),
            self.get_rest_table(&format!(
                "kds_routing_profiles?site_id=eq.{site_id}&select=*"
            )),
            self.get_rest_table(&format!(
                "kds_routing_configs?site_id=eq.{site_id}&select=*"
            )),
            self.get_rest_table(&format!(
                "kds_channel_triggers?site_id=eq.{site_id}&select=*"
            )),
            self.get_rest_table(&format!(
                "kds_timer_thresholds?site_id=eq.{site_id}&select=*"
            )),
        )?;

        Ok(KdsCloudData { stations, profiles, rules, triggers, thresholds })
    }
```

> Note : `KdsCloudData` et `get_rest_table` sont définis à l'intérieur de l'impl — déplacer `KdsCloudData` au niveau module si clippy signale un problème de visibilité. En pratique, la struct doit être déclarée **avant** l'impl, pas dedans. Voici la structure correcte :

```rust
// AVANT l'impl SupabaseClient { ... }
pub struct KdsCloudData {
    pub stations: Vec<serde_json::Value>,
    pub profiles: Vec<serde_json::Value>,
    pub rules: Vec<serde_json::Value>,
    pub triggers: Vec<serde_json::Value>,
    pub thresholds: Vec<serde_json::Value>,
}

// Dans l'impl SupabaseClient :
//   async fn get_rest_table(...)  [privé]
//   pub async fn pull_kds_config(...)  [public]
```

- [ ] **Créer `sync-client/src/kds_puller.rs`**

```rust
//! # `kds_puller`
//!
//! Pull la configuration KDS depuis Supabase et upsert dans SQLite local.
//!
//! ## Tables cloud → SQLite
//! | Cloud (Supabase)         | Local (SQLite)          |
//! |--------------------------|-------------------------|
//! | `kds_station_configs`    | `kds_stations`          |
//! | `kds_routing_profiles`   | `kds_routing_profiles`  |
//! | `kds_routing_configs`    | `kds_routing_rules`     |
//! | `kds_channel_triggers`   | `kds_channel_triggers`  |
//! | `kds_timer_thresholds`   | `kds_timer_thresholds`  |
//!
//! La table `kds_active_profile` n'est pas touchée — gérée localement.

use sqlx::SqlitePool;
use tracing::{debug, info, warn};

use crate::{client::SupabaseClient, config::SyncConfig, error::SyncError};

fn str_val(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(str::to_string)
}

fn str_req(v: &serde_json::Value, key: &str, ctx: &str) -> Option<String> {
    let s = str_val(v, key);
    if s.is_none() {
        warn!(key = %key, context = %ctx, "Champ obligatoire absent — ligne ignorée");
    }
    s
}

fn int_val(v: &serde_json::Value, key: &str) -> Option<i64> {
    v.get(key).and_then(|x| x.as_i64())
}

/// Pull la config KDS depuis Supabase et upsert dans SQLite local.
///
/// # Returns
/// Nombre total de lignes traitées (toutes tables confondues).
///
/// # Errors
/// `SyncError::Network` si Supabase est inaccessible.
/// `SyncError::Database` si un upsert SQLite échoue.
pub async fn pull_kds_config(
    client: &SupabaseClient,
    config: &SyncConfig,
    pool: &SqlitePool,
) -> Result<usize, SyncError> {
    let cloud = client.pull_kds_config(&config.site_id).await?;
    let mut total = 0usize;

    // --- kds_routing_profiles ---
    for v in &cloud.profiles {
        let Some(id) = str_req(v, "id", "kds_routing_profiles") else { continue };
        let Some(name) = str_req(v, "name", "kds_routing_profiles") else { continue };
        let desc = str_val(v, "description");

        sqlx::query(
            "INSERT INTO kds_routing_profiles (id, name, description)
             VALUES (?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description",
        )
        .bind(&id)
        .bind(&name)
        .bind(desc.as_deref())
        .execute(pool)
        .await
        .map_err(SyncError::Database)?;

        total += 1;
    }
    debug!(count = cloud.profiles.len(), "kds_routing_profiles upserted");

    // --- kds_stations ---
    for v in &cloud.stations {
        let Some(id) = str_req(v, "id", "kds_station_configs") else { continue };
        let Some(name) = str_req(v, "name", "kds_station_configs") else { continue };
        let Some(role) = str_req(v, "role", "kds_station_configs") else { continue };
        let Some(output_type) = str_req(v, "output_type", "kds_station_configs") else { continue };

        let active_in_profiles = str_val(v, "active_in_profiles")
            .unwrap_or_else(|| r#"["normal"]"#.to_string());
        let sort_order = int_val(v, "sort_order").unwrap_or(0);
        let enabled = int_val(v, "enabled").unwrap_or(1);

        sqlx::query(
            "INSERT INTO kds_stations
             (id, name, role, temperature_group, output_type, printer_address,
              printer_type, printer_mode, paper_width_mm, fallback_station_id,
              active_in_profiles, sort_order, enabled)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, role=excluded.role,
               temperature_group=excluded.temperature_group,
               output_type=excluded.output_type,
               printer_address=excluded.printer_address,
               printer_type=excluded.printer_type,
               printer_mode=excluded.printer_mode,
               paper_width_mm=excluded.paper_width_mm,
               fallback_station_id=excluded.fallback_station_id,
               active_in_profiles=excluded.active_in_profiles,
               sort_order=excluded.sort_order,
               enabled=excluded.enabled",
        )
        .bind(&id)
        .bind(&name)
        .bind(&role)
        .bind(str_val(v, "temperature_group").as_deref())
        .bind(&output_type)
        .bind(str_val(v, "printer_address").as_deref())
        .bind(str_val(v, "printer_type").as_deref())
        .bind(str_val(v, "printer_mode").as_deref())
        .bind(int_val(v, "paper_width_mm"))
        .bind(str_val(v, "fallback_station_id").as_deref())
        .bind(&active_in_profiles)
        .bind(sort_order)
        .bind(enabled)
        .execute(pool)
        .await
        .map_err(SyncError::Database)?;

        total += 1;
    }
    debug!(count = cloud.stations.len(), "kds_stations upserted");

    // --- kds_routing_rules ---
    for v in &cloud.rules {
        let Some(id) = str_req(v, "id", "kds_routing_configs") else { continue };
        let Some(profile_id) = str_req(v, "profile_id", "kds_routing_configs") else { continue };
        let Some(rule_type) = str_req(v, "rule_type", "kds_routing_configs") else { continue };
        let Some(match_value) = str_req(v, "match_value", "kds_routing_configs") else { continue };
        let Some(station_ids) = str_req(v, "station_ids", "kds_routing_configs") else { continue };
        let priority = int_val(v, "priority").unwrap_or(0);

        sqlx::query(
            "INSERT INTO kds_routing_rules (id, profile_id, rule_type, match_value, station_ids, priority)
             VALUES (?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               profile_id=excluded.profile_id, rule_type=excluded.rule_type,
               match_value=excluded.match_value, station_ids=excluded.station_ids,
               priority=excluded.priority",
        )
        .bind(&id)
        .bind(&profile_id)
        .bind(&rule_type)
        .bind(&match_value)
        .bind(&station_ids)
        .bind(priority)
        .execute(pool)
        .await
        .map_err(SyncError::Database)?;

        total += 1;
    }
    debug!(count = cloud.rules.len(), "kds_routing_rules upserted");

    // --- kds_channel_triggers ---
    for v in &cloud.triggers {
        let Some(channel) = str_req(v, "channel", "kds_channel_triggers") else { continue };
        let Some(order_type) = str_req(v, "order_type", "kds_channel_triggers") else { continue };
        let Some(trigger_on) = str_req(v, "trigger_on", "kds_channel_triggers") else { continue };
        let orb_type = str_val(v, "orb_type");

        sqlx::query(
            "INSERT INTO kds_channel_triggers (channel, order_type, trigger_on, orb_type)
             VALUES (?,?,?,?)
             ON CONFLICT(channel, order_type) DO UPDATE SET
               trigger_on=excluded.trigger_on, orb_type=excluded.orb_type",
        )
        .bind(&channel)
        .bind(&order_type)
        .bind(&trigger_on)
        .bind(orb_type.as_deref())
        .execute(pool)
        .await
        .map_err(SyncError::Database)?;

        total += 1;
    }
    debug!(count = cloud.triggers.len(), "kds_channel_triggers upserted");

    // --- kds_timer_thresholds ---
    for v in &cloud.thresholds {
        let Some(station_id) = str_req(v, "station_id", "kds_timer_thresholds") else { continue };
        let warning_secs = int_val(v, "warning_secs").unwrap_or(120);
        let critical_secs = int_val(v, "critical_secs").unwrap_or(300);

        sqlx::query(
            "INSERT INTO kds_timer_thresholds (station_id, warning_secs, critical_secs)
             VALUES (?,?,?)
             ON CONFLICT(station_id) DO UPDATE SET
               warning_secs=excluded.warning_secs, critical_secs=excluded.critical_secs",
        )
        .bind(&station_id)
        .bind(warning_secs)
        .bind(critical_secs)
        .execute(pool)
        .await
        .map_err(SyncError::Database)?;

        total += 1;
    }
    debug!(count = cloud.thresholds.len(), "kds_timer_thresholds upserted");

    info!(total = total, "Config KDS synchronisée depuis Supabase");
    Ok(total)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn str_req_missing_field_returns_none() {
        let v = json!({ "name": "Grill" });
        assert!(str_req(&v, "id", "test").is_none());
    }

    #[test]
    fn str_req_present_field_returns_some() {
        let v = json!({ "id": "grill-01" });
        assert_eq!(str_req(&v, "id", "test"), Some("grill-01".to_string()));
    }

    #[test]
    fn int_val_returns_correct_value() {
        let v = json!({ "priority": 5 });
        assert_eq!(int_val(&v, "priority"), Some(5));
    }

    #[test]
    fn int_val_missing_returns_none() {
        let v = json!({});
        assert_eq!(int_val(&v, "priority"), None);
    }
}
```

- [ ] **Modifier `sync-client/src/lib.rs`**

Ajouter `pub mod kds_puller;` après `pub mod promo_puller;` :
```rust
pub mod client;
pub mod config;
pub mod config_puller;
pub mod error;
pub mod kds_puller;
pub mod promo_puller;
pub mod serializer;
pub mod sync_loop;
```

- [ ] **Vérifier compilation**

```bash
cargo build -p sync-client 2>&1 | grep -E "^error" | head -20
```
Attendu : 0 erreurs.

- [ ] **Lancer les tests unitaires kds_puller**

```bash
cargo test -p sync-client kds_puller -- --nocapture
```
Attendu : 4 tests passent.

- [ ] **Commit**

```bash
git add sync-client/src/client.rs sync-client/src/kds_puller.rs sync-client/src/lib.rs
git commit -m "feat(sync-client): kds_puller — pull KDS config Supabase → SQLite"
```

---

## Task 8 : Intégration dans `sync_loop.rs` + CI verte

**Files:**
- Modify: `sync-client/src/sync_loop.rs`

**Interfaces:**
- Consomme: `kds_puller::pull_kds_config` (kds_puller.rs)
- Produit: `run_sync_cycle` appelle `pull_kds_config` après les promotions, résultat logué

- [ ] **Modifier `sync-client/src/sync_loop.rs`**

Ajouter l'import après les imports existants :
```rust
use crate::kds_puller::pull_kds_config;
```

Dans `run_sync_cycle`, ajouter après le bloc `pull_promotions` (step 7, ligne ~224) :

```rust
    // 8. Pull de la config KDS depuis Supabase → SQLite local
    match pull_kds_config(client, config, store.pool_ref()).await {
        Ok(count) if count > 0 => {
            info!(count = count, "Config KDS mise à jour depuis Supabase");
        }
        Ok(_) => {
            debug!("Config KDS : aucun changement");
        }
        Err(e) => {
            warn!(error = %e, "Échec du pull config KDS (non fatal)");
        }
    }
```

- [ ] **Lancer la CI complète**

```bash
cargo fmt --check && \
cargo clippy --workspace -- -D warnings && \
cargo test --workspace && \
cargo build --release
```
Attendu : aucune erreur, aucun warning.

- [ ] **Corriger les warnings clippy pedantic éventuels**

Patterns courants dans ce crate :
- `struct KdsCloudData` doit être définie au niveau module (pas dans l'impl)
- `dead_code` sur `JsonValue` re-export → supprimer le re-export si non utilisé
- `clippy::missing_errors_doc` → ajouter `# Errors` si absent

- [ ] **Test unitaire `sync_loop` — vérifier que le cycle ne régresse pas**

```bash
cargo test -p sync-client -- --nocapture 2>&1 | tail -15
```
Attendu : tous les tests passent.

- [ ] **Commit final**

```bash
git add sync-client/src/sync_loop.rs
git commit -m "feat(sync-client): pull_kds_config intégré dans run_sync_cycle — Plan 3 complet"
```

---

## Résumé des livrables Plan 3

| Livrable | Statut cible |
|---|---|
| Migration Supabase 021 — 5 tables KDS cloud + RLS + seed defaults | ✅ |
| Page `KdsStations` — liste CRUD par site | ✅ |
| Page `KdsStationForm` — create/edit station (tous champs imprimante) | ✅ |
| Page `KdsRoutingRules` — règles de routage groupées par profil | ✅ |
| Page `KdsTimerThresholds` — seuils warning/critical éditables | ✅ |
| Navigation backoffice — section "Cuisine" dans Layout.tsx | ✅ |
| Routes `/kitchen/*` dans App.tsx | ✅ |
| `SupabaseClient::pull_kds_config` — 5 GET parallèles | ✅ |
| `kds_puller::pull_kds_config` — upsert 5 tables SQLite | ✅ |
| `sync_loop::run_sync_cycle` — appel pull KDS intégré | ✅ |
| CI verte (`fmt` + `clippy -D warnings` + `test`) | ✅ |

## Hors scope (Plan 4 éventuel)

- Page `/kitchen/triggers` — gestion des déclencheurs canal × order_type (les defaults sont seedés en migration)
- Overrides locaux avec `source = 'local'` pour les stations (protection du pull Supabase)
- Page d'analytics production `/kitchen/analytics`
- Serveur de fichiers statiques kds-app dans edge-api (Axum `ServeDir` à `/kds/*`)
