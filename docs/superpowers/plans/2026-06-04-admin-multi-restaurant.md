# Admin Multi-Restaurant — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ajouter une couche admin multi-restaurant au back-office existant : CRUD sites, gestion utilisateurs, config technique (clé Ed25519 via Vault), et matrice de permissions lock/unlock.

**Architecture:** Admin intégré dans le backoffice (`/admin/*`) avec `NetworkContext` distinct du `SiteContext` existant, `AdminRoute` guard, deux Edge Functions Supabase (`user-admin`, `config-provision`), trois nouvelles migrations SQL (016–018).

**Tech Stack:** React 19, React Router 7, `@supabase/supabase-js` v2, Vite, TypeScript strict, Supabase Edge Functions (Deno), PostgreSQL 15 (Supabase cloud)

---

## Structure de fichiers

**Créés :**
```
supabase/migrations/016_site_technical_configs.sql
supabase/migrations/017_network_permissions.sql
supabase/migrations/018_site_configs_device_type.sql
supabase/functions/user-admin/index.ts
supabase/functions/config-provision/index.ts
backoffice/src/context/NetworkContext.tsx
backoffice/src/hooks/useNetworkGuard.ts
backoffice/src/components/AdminRoute.tsx
backoffice/src/pages/admin/SiteList.tsx
backoffice/src/pages/admin/SiteForm.tsx
backoffice/src/pages/admin/UserList.tsx
backoffice/src/pages/admin/UserForm.tsx
backoffice/src/pages/admin/TechnicalConfigForm.tsx
backoffice/src/pages/admin/PermissionsMatrix.tsx
```

**Modifiés :**
```
backoffice/src/App.tsx          — NetworkProvider, AdminRoute, 8 nouvelles routes
backoffice/src/components/Layout.tsx  — section "Réseau" conditionnelle
```

---

## Task 1 : Migration 016 — `site_technical_configs` + helpers SQL

**Files:**
- Create: `supabase/migrations/016_site_technical_configs.sql`

- [ ] **Step 1 : Créer le fichier de migration**

```sql
-- supabase/migrations/016_site_technical_configs.sql
-- Paramètres serveur par site/device. Clés Ed25519 en Vault (jamais ici).

CREATE TABLE public.site_technical_configs (
  id                       uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  site_id                  uuid        NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
  device_type              text        NOT NULL DEFAULT 'pos',
  edge_api_port            integer     NOT NULL DEFAULT 8080,
  sync_interval_s          integer     NOT NULL DEFAULT 300,
  fiscal_key_configured_at timestamptz,
  updated_at               timestamptz NOT NULL DEFAULT now(),
  UNIQUE (site_id, device_type)
);

-- Helper RLS : accès d'un user à un site donné
CREATE OR REPLACE FUNCTION public.can_access_site(p_site_id uuid)
RETURNS boolean LANGUAGE sql STABLE AS $$
  SELECT CASE public.auth_app_role()
    WHEN 'pos_admin'         THEN true
    WHEN 'pos_auditeur'      THEN true
    WHEN 'regional_director' THEN EXISTS (
      SELECT 1 FROM restaurant_group_members rgm
      JOIN   restaurant_groups rg ON rg.id = rgm.group_id
      WHERE  rgm.site_id = p_site_id AND rg.created_by = auth.uid()
    )
    WHEN 'pos_caissier'      THEN p_site_id = public.auth_site_id()
    ELSE false
  END;
$$;

-- Provision clé Ed25519 → Vault (appelée par Edge Function config-provision via service_role)
CREATE OR REPLACE FUNCTION public.provision_fiscal_key(p_site_id uuid, p_key_hex text)
RETURNS timestamptz LANGUAGE plpgsql SECURITY DEFINER
SET search_path = public, vault AS $$
DECLARE
  v_secret_name text := 'fiscal_key_' || p_site_id::text;
  v_existing_id uuid;
BEGIN
  IF p_key_hex !~ '^[0-9a-fA-F]{64}$' THEN
    RAISE EXCEPTION 'invalid_key_format';
  END IF;
  SELECT id INTO v_existing_id FROM vault.secrets WHERE name = v_secret_name;
  IF v_existing_id IS NOT NULL THEN
    PERFORM vault.update_secret(v_existing_id, p_key_hex);
  ELSE
    PERFORM vault.create_secret(p_key_hex, v_secret_name);
  END IF;
  INSERT INTO public.site_technical_configs (site_id, fiscal_key_configured_at, updated_at)
    VALUES (p_site_id, now(), now())
    ON CONFLICT (site_id, device_type) DO UPDATE
      SET fiscal_key_configured_at = now(), updated_at = now();
  RETURN now();
END; $$;

REVOKE EXECUTE ON FUNCTION public.provision_fiscal_key FROM PUBLIC, authenticated;
GRANT  EXECUTE ON FUNCTION public.provision_fiscal_key TO service_role;

-- Fonction helper pour lister les users admin (lit auth.users en SECURITY DEFINER)
CREATE OR REPLACE FUNCTION public.list_admin_users()
RETURNS TABLE (
  id            uuid,
  email         text,
  role          text,
  site_id       text,
  display_name  text,
  is_banned     boolean,
  created_at    timestamptz
)
LANGUAGE plpgsql SECURITY DEFINER
SET search_path = public, auth AS $$
DECLARE
  v_role text := public.auth_app_role();
BEGIN
  IF v_role NOT IN ('pos_admin', 'regional_director') THEN
    RAISE EXCEPTION 'forbidden';
  END IF;
  RETURN QUERY
    SELECT
      u.id,
      u.email,
      (u.app_metadata ->> 'role')::text,
      (u.app_metadata ->> 'site_id')::text,
      (u.app_metadata ->> 'display_name')::text,
      (u.banned_until IS NOT NULL AND u.banned_until > now()),
      u.created_at
    FROM auth.users u
    ORDER BY u.created_at DESC;
END; $$;

GRANT EXECUTE ON FUNCTION public.list_admin_users TO authenticated;

-- RLS site_technical_configs
ALTER TABLE public.site_technical_configs ENABLE ROW LEVEL SECURITY;

CREATE POLICY "stc_read" ON public.site_technical_configs
  FOR SELECT TO authenticated USING (public.can_access_site(site_id));

CREATE POLICY "stc_admin_write" ON public.site_technical_configs
  FOR ALL TO authenticated
  USING    (public.auth_app_role() = 'pos_admin')
  WITH CHECK (public.auth_app_role() = 'pos_admin');

CREATE POLICY "stc_service_role" ON public.site_technical_configs
  FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.site_technical_configs TO authenticated;
```

- [ ] **Step 2 : Appliquer la migration**

```bash
cd pos-fiscal
supabase db push
```

Expected output (extrait) :
```
Applying migration 016_site_technical_configs.sql...
Done.
```

- [ ] **Step 3 : Vérifier les tables et fonctions dans Supabase Studio**

Ouvrir [https://supabase.com/dashboard/project/iawyngsvqjsogvkwkrxw/editor](https://supabase.com/dashboard/project/iawyngsvqjsogvkwkrxw/editor) et exécuter :

```sql
SELECT column_name, data_type FROM information_schema.columns
WHERE table_name = 'site_technical_configs' ORDER BY ordinal_position;

SELECT proname FROM pg_proc WHERE proname IN
  ('can_access_site', 'provision_fiscal_key', 'list_admin_users');
```

Expected : 7 colonnes pour `site_technical_configs`, 3 lignes pour les fonctions.

- [ ] **Step 4 : Commit**

```bash
git add supabase/migrations/016_site_technical_configs.sql
git commit -m "feat(db): migration 016 site_technical_configs + helpers SQL"
```

---

## Task 2 : Migration 017 — `network_permissions`

**Files:**
- Create: `supabase/migrations/017_network_permissions.sql`

- [ ] **Step 1 : Créer le fichier de migration**

```sql
-- supabase/migrations/017_network_permissions.sql
-- Matrice lock/unlock : dimension × rôle cible × scope × fenêtre temporelle

CREATE TABLE public.network_permissions (
  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  -- Scope mutuellement exclusif :
  --   (null, null)      → réseau entier
  --   (null, group_id)  → per-groupe
  --   (site_id, null)   → per-site (priorité max)
  site_id     uuid REFERENCES public.sites(id)              ON DELETE CASCADE,
  group_id    uuid REFERENCES public.restaurant_groups(id)  ON DELETE CASCADE,
  dimension   text NOT NULL,
    -- 'menu' | 'prices' | 'promotions' | 'discounts' | 'user_management' | 'z_reports'
  target_role text NOT NULL,
    -- 'manager' | 'director' | 'regional_director'
  locked      boolean NOT NULL DEFAULT false,
  lock_from   time,
  lock_until  time,
  reason      text,
  updated_by  uuid REFERENCES auth.users(id),
  updated_at  timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT chk_scope CHECK (
    NOT (site_id IS NOT NULL AND group_id IS NOT NULL)
  ),
  CONSTRAINT chk_time_window CHECK (
    (lock_from IS NULL) = (lock_until IS NULL)
    AND (lock_from IS NULL OR lock_from < lock_until)
  ),
  UNIQUE NULLS NOT DISTINCT (site_id, group_id, dimension, target_role)
);

ALTER TABLE public.network_permissions ENABLE ROW LEVEL SECURITY;

-- Lecture : tout authentifié (nécessaire pour charger les règles héritées)
CREATE POLICY "np_read" ON public.network_permissions
  FOR SELECT TO authenticated USING (true);

-- Écriture pos_admin : toutes les lignes
CREATE POLICY "np_admin_write" ON public.network_permissions
  FOR ALL TO authenticated
  USING    (public.auth_app_role() = 'pos_admin')
  WITH CHECK (public.auth_app_role() = 'pos_admin');

-- Écriture regional_director : per-site (ses sites) et per-group (ses groupes) uniquement
CREATE POLICY "np_rd_write" ON public.network_permissions
  FOR ALL TO authenticated
  USING (
    public.auth_app_role() = 'regional_director' AND (
      (site_id  IS NOT NULL AND group_id IS NULL AND public.can_access_site(site_id)) OR
      (group_id IS NOT NULL AND site_id  IS NULL AND EXISTS (
        SELECT 1 FROM restaurant_groups WHERE id = group_id AND created_by = auth.uid()
      ))
    )
  )
  WITH CHECK (
    public.auth_app_role() = 'regional_director' AND (
      (site_id  IS NOT NULL AND group_id IS NULL AND public.can_access_site(site_id)) OR
      (group_id IS NOT NULL AND site_id  IS NULL AND EXISTS (
        SELECT 1 FROM restaurant_groups WHERE id = group_id AND created_by = auth.uid()
      ))
    )
  );

CREATE POLICY "np_service_role" ON public.network_permissions
  FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.network_permissions TO authenticated;
```

- [ ] **Step 2 : Appliquer et vérifier**

```bash
supabase db push
```

Vérifier dans l'éditeur SQL :
```sql
SELECT column_name, data_type FROM information_schema.columns
WHERE table_name = 'network_permissions' ORDER BY ordinal_position;
```

Expected : 11 colonnes.

- [ ] **Step 3 : Commit**

```bash
git add supabase/migrations/017_network_permissions.sql
git commit -m "feat(db): migration 017 network_permissions + RLS"
```

---

## Task 3 : Migration 018 — `site_configs` device_type

**Files:**
- Create: `supabase/migrations/018_site_configs_device_type.sql`

- [ ] **Step 1 : Créer le fichier**

```sql
-- supabase/migrations/018_site_configs_device_type.sql
-- Prépare site_configs pour KDS/Kiosk (roadmap). V1 = 'pos' uniquement.

ALTER TABLE public.site_configs
  ADD COLUMN IF NOT EXISTS device_type text NOT NULL DEFAULT 'pos';

ALTER TABLE public.site_configs
  ADD CONSTRAINT uq_site_configs_site_device UNIQUE (site_id, device_type);
```

- [ ] **Step 2 : Appliquer**

```bash
supabase db push
```

- [ ] **Step 3 : Commit**

```bash
git add supabase/migrations/018_site_configs_device_type.sql
git commit -m "feat(db): migration 018 site_configs device_type"
```

---

## Task 4 : Edge Function `user-admin`

**Files:**
- Create: `supabase/functions/user-admin/index.ts`

- [ ] **Step 1 : Créer le répertoire et le fichier**

```bash
mkdir -p supabase/functions/user-admin
```

```typescript
// supabase/functions/user-admin/index.ts
import { createClient } from 'https://esm.sh/@supabase/supabase-js@2'

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
}

function generatePassword(length = 16): string {
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'
  const bytes = crypto.getRandomValues(new Uint8Array(length))
  return Array.from(bytes, (b: number) => chars[b % chars.length]).join('')
}

function toSlug(employeeId: string): string {
  return employeeId.toLowerCase().replace(/[^a-z0-9]/g, '')
}

Deno.serve(async (req) => {
  if (req.method === 'OPTIONS') return new Response('ok', { headers: corsHeaders })

  const authHeader = req.headers.get('Authorization')
  if (!authHeader) {
    return new Response(JSON.stringify({ error: 'missing_auth' }), { status: 401, headers: corsHeaders })
  }

  const supabaseUser = createClient(
    Deno.env.get('SUPABASE_URL')!,
    Deno.env.get('SUPABASE_ANON_KEY')!,
    { global: { headers: { Authorization: authHeader } } }
  )
  const { data: { user }, error: authErr } = await supabaseUser.auth.getUser()
  if (authErr || !user) {
    return new Response(JSON.stringify({ error: 'unauthorized' }), { status: 401, headers: corsHeaders })
  }

  const callerRole = user.app_metadata?.role as string | undefined
  if (callerRole !== 'pos_admin' && callerRole !== 'regional_director') {
    return new Response(JSON.stringify({ error: 'forbidden' }), { status: 403, headers: corsHeaders })
  }

  const supabaseAdmin = createClient(
    Deno.env.get('SUPABASE_URL')!,
    Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!
  )

  const body = await req.json() as { action: string; payload: Record<string, string> }
  const { action, payload } = body

  // Pour regional_director : vérifie que le site cible est dans son périmètre
  async function assertScope(targetSiteId: string | undefined): Promise<Response | null> {
    if (callerRole === 'pos_admin') return null
    if (!targetSiteId) {
      return new Response(JSON.stringify({ error: 'scope_forbidden' }), { status: 403, headers: corsHeaders })
    }
    const { data: ok } = await supabaseUser.rpc('can_access_site', { p_site_id: targetSiteId })
    if (!ok) {
      return new Response(JSON.stringify({ error: 'scope_forbidden' }), { status: 403, headers: corsHeaders })
    }
    return null
  }

  try {
    switch (action) {
      case 'list': {
        // Les deux rôles voient tous les users (filtrage scope dans l'UI)
        const { data, error } = await supabaseAdmin.auth.admin.listUsers({ perPage: 1000 })
        if (error) throw error
        return new Response(JSON.stringify({ users: data.users }), {
          status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        })
      }

      case 'invite': {
        const scopeErr = await assertScope(payload.site_id)
        if (scopeErr) return scopeErr
        const appMeta: Record<string, string> = { role: payload.role, display_name: payload.display_name ?? '' }
        if (payload.site_id) appMeta.site_id = payload.site_id
        const { data, error } = await supabaseAdmin.auth.admin.inviteUserByEmail(payload.email, { data: appMeta })
        if (error) throw error
        return new Response(JSON.stringify({ user_id: data.user.id }), {
          status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        })
      }

      case 'create_direct': {
        const scopeErr = await assertScope(payload.site_id)
        if (scopeErr) return scopeErr
        const slug = toSlug(payload.employee_id)
        const email = `emp_${slug}@internal.pos-fiscal.local`
        const tempPassword = generatePassword()
        const appMeta: Record<string, string> = { role: payload.role, display_name: payload.display_name ?? '' }
        if (payload.site_id) appMeta.site_id = payload.site_id
        const { data, error } = await supabaseAdmin.auth.admin.createUser({
          email, password: tempPassword, app_metadata: appMeta, email_confirm: true,
        })
        if (error) throw error
        return new Response(JSON.stringify({ user_id: data.user.id, email, temp_password: tempPassword }), {
          status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        })
      }

      case 'update_role': {
        // Charger le site_id actuel du user cible pour la vérification de scope
        const { data: { user: targetUser }, error: getErr } = await supabaseAdmin.auth.admin.getUserById(payload.user_id)
        if (getErr) throw getErr
        const currentSiteId = targetUser.app_metadata?.site_id as string | undefined
        const scopeErr = await assertScope(currentSiteId)
        if (scopeErr) return scopeErr
        const appMeta: Record<string, string> = { role: payload.role }
        if (payload.site_id) appMeta.site_id = payload.site_id
        const { error } = await supabaseAdmin.auth.admin.updateUserById(payload.user_id, { app_metadata: appMeta })
        if (error) throw error
        return new Response(JSON.stringify({ ok: true }), {
          status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        })
      }

      case 'revoke': {
        const { data: { user: targetUser }, error: getErr } = await supabaseAdmin.auth.admin.getUserById(payload.user_id)
        if (getErr) throw getErr
        const scopeErr = await assertScope(targetUser.app_metadata?.site_id as string | undefined)
        if (scopeErr) return scopeErr
        const { error } = await supabaseAdmin.auth.admin.updateUserById(payload.user_id, { ban_duration: '876600h' })
        if (error) throw error
        return new Response(JSON.stringify({ ok: true }), {
          status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
        })
      }

      default:
        return new Response(JSON.stringify({ error: 'unknown_action' }), { status: 400, headers: corsHeaders })
    }
  } catch (e: unknown) {
    const msg = e instanceof Error ? e.message : 'internal_error'
    return new Response(JSON.stringify({ error: msg }), { status: 400, headers: corsHeaders })
  }
})
```

- [ ] **Step 2 : Déployer**

```bash
supabase functions deploy user-admin
```

Expected output :
```
Deployed Function user-admin on project iawyngsvqjsogvkwkrxw
```

- [ ] **Step 3 : Tester avec curl (nécessite un JWT pos_admin valide)**

```bash
# Récupérer un token pos_admin depuis le Studio → Auth → Users → "Generate token"
# Remplacer <TOKEN> ci-dessous
curl -s -X POST \
  https://iawyngsvqjsogvkwkrxw.supabase.co/functions/v1/user-admin \
  -H "Authorization: Bearer <TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"action":"list","payload":{}}' | jq '.users | length'
```

Expected : un entier ≥ 0 (nombre de users).

- [ ] **Step 4 : Commit**

```bash
git add supabase/functions/user-admin/index.ts
git commit -m "feat(functions): user-admin — list/invite/create_direct/update_role/revoke"
```

---

## Task 5 : Edge Function `config-provision`

**Files:**
- Create: `supabase/functions/config-provision/index.ts`

- [ ] **Step 1 : Créer le répertoire et le fichier**

```bash
mkdir -p supabase/functions/config-provision
```

```typescript
// supabase/functions/config-provision/index.ts
import { createClient } from 'https://esm.sh/@supabase/supabase-js@2'

const corsHeaders = {
  'Access-Control-Allow-Origin': '*',
  'Access-Control-Allow-Headers': 'authorization, x-client-info, apikey, content-type',
}

Deno.serve(async (req) => {
  if (req.method === 'OPTIONS') return new Response('ok', { headers: corsHeaders })

  const authHeader = req.headers.get('Authorization')
  if (!authHeader) {
    return new Response(JSON.stringify({ error: 'missing_auth' }), { status: 401, headers: corsHeaders })
  }

  const supabaseUser = createClient(
    Deno.env.get('SUPABASE_URL')!,
    Deno.env.get('SUPABASE_ANON_KEY')!,
    { global: { headers: { Authorization: authHeader } } }
  )
  const { data: { user }, error: authErr } = await supabaseUser.auth.getUser()
  if (authErr || !user) {
    return new Response(JSON.stringify({ error: 'unauthorized' }), { status: 401, headers: corsHeaders })
  }
  if (user.app_metadata?.role !== 'pos_admin') {
    return new Response(JSON.stringify({ error: 'forbidden' }), { status: 403, headers: corsHeaders })
  }

  const { site_id, key_hex } = await req.json() as { site_id: string; key_hex: string }

  const supabaseAdmin = createClient(
    Deno.env.get('SUPABASE_URL')!,
    Deno.env.get('SUPABASE_SERVICE_ROLE_KEY')!
  )

  const { data: configured_at, error } = await supabaseAdmin.rpc('provision_fiscal_key', {
    p_site_id: site_id,
    p_key_hex: key_hex,
  })

  if (error) {
    const status = error.message === 'invalid_key_format' ? 400 : 500
    return new Response(JSON.stringify({ error: error.message }), { status, headers: corsHeaders })
  }

  return new Response(JSON.stringify({ configured_at }), {
    status: 200, headers: { ...corsHeaders, 'Content-Type': 'application/json' },
  })
})
```

- [ ] **Step 2 : Déployer**

```bash
supabase functions deploy config-provision
```

- [ ] **Step 3 : Commit**

```bash
git add supabase/functions/config-provision/index.ts
git commit -m "feat(functions): config-provision — écriture Vault clé Ed25519"
```

---

## Task 6 : `NetworkContext.tsx`

**Files:**
- Create: `backoffice/src/context/NetworkContext.tsx`

- [ ] **Step 1 : Créer le fichier**

```typescript
// backoffice/src/context/NetworkContext.tsx
import { createContext, useContext, useEffect, useState, type ReactNode } from 'react'
import { supabase } from '../supabaseClient'
import { useAuth } from './AuthContext'

export interface NetworkSite {
  id: string
  site_code: string
  name: string
  address: string | null
  siret: string | null
}

export interface GroupWithMembers {
  id: string
  name: string
  site_ids: string[]
}

export interface NetworkPermission {
  id: string
  site_id: string | null
  group_id: string | null
  dimension: string
  target_role: string
  locked: boolean
  lock_from: string | null
  lock_until: string | null
  reason: string | null
  updated_by: string | null
  updated_at: string
}

interface NetworkContextValue {
  allSites: NetworkSite[]
  groups: GroupWithMembers[]
  permissions: NetworkPermission[]
  isLocked: (dimension: string, targetRole: string, siteId: string | null) => boolean
  reload: () => void
}

const NetworkContext = createContext<NetworkContextValue>({
  allSites: [],
  groups: [],
  permissions: [],
  isLocked: () => false,
  reload: () => {},
})

function timeToMinutes(t: string): number {
  const [h, m] = t.split(':').map(Number)
  return h * 60 + m
}

function isInWindow(lockFrom: string, lockUntil: string): boolean {
  const now = new Date()
  const cur = now.getHours() * 60 + now.getMinutes()
  return cur >= timeToMinutes(lockFrom) && cur <= timeToMinutes(lockUntil)
}

function checkLocked(rule: NetworkPermission): boolean {
  if (!rule.locked) return false
  if (rule.lock_from && rule.lock_until) return isInWindow(rule.lock_from, rule.lock_until)
  return true
}

export function NetworkProvider({ children }: { children: ReactNode }) {
  const { session, role } = useAuth()
  const [allSites, setAllSites] = useState<NetworkSite[]>([])
  const [groups, setGroups] = useState<GroupWithMembers[]>([])
  const [permissions, setPermissions] = useState<NetworkPermission[]>([])
  const [tick, setTick] = useState(0)

  const isAdmin = role === 'pos_admin' || role === 'regional_director'

  useEffect(() => {
    if (!session || !isAdmin) return

    supabase
      .from('sites')
      .select('id,site_code,name,address,siret')
      .order('site_code')
      .then(({ data }) => setAllSites((data as NetworkSite[]) ?? []))

    supabase
      .from('restaurant_groups')
      .select('id,name,restaurant_group_members(site_id)')
      .order('name')
      .then(({ data }) => {
        setGroups(
          (data ?? []).map((g: { id: string; name: string; restaurant_group_members: Array<{ site_id: string }> }) => ({
            id: g.id,
            name: g.name,
            site_ids: g.restaurant_group_members.map((m) => m.site_id),
          }))
        )
      })

    supabase
      .from('network_permissions')
      .select('*')
      .then(({ data }) => setPermissions((data as NetworkPermission[]) ?? []))
  }, [session, isAdmin, tick])

  function isLocked(dimension: string, targetRole: string, siteId: string | null): boolean {
    if (siteId) {
      const siteRule = permissions.find(
        (p) => p.site_id === siteId && p.group_id === null && p.dimension === dimension && p.target_role === targetRole
      )
      if (siteRule) return checkLocked(siteRule)

      for (const group of groups) {
        if (!group.site_ids.includes(siteId)) continue
        const groupRule = permissions.find(
          (p) => p.group_id === group.id && p.site_id === null && p.dimension === dimension && p.target_role === targetRole
        )
        if (groupRule) return checkLocked(groupRule)
      }
    }

    const networkRule = permissions.find(
      (p) => p.site_id === null && p.group_id === null && p.dimension === dimension && p.target_role === targetRole
    )
    if (networkRule) return checkLocked(networkRule)

    return false
  }

  return (
    <NetworkContext.Provider value={{ allSites, groups, permissions, isLocked, reload: () => setTick((t) => t + 1) }}>
      {children}
    </NetworkContext.Provider>
  )
}

export const useNetwork = () => useContext(NetworkContext)
```

- [ ] **Step 2 : Vérifier la compilation TypeScript**

```bash
cd backoffice && npm run build
```

Expected : `✓ built in XXs` sans erreur.

- [ ] **Step 3 : Commit**

```bash
git add backoffice/src/context/NetworkContext.tsx
git commit -m "feat(backoffice): NetworkContext — données réseau + isLocked"
```

---

## Task 7 : `useNetworkGuard.ts` + `AdminRoute.tsx`

**Files:**
- Create: `backoffice/src/hooks/useNetworkGuard.ts`
- Create: `backoffice/src/components/AdminRoute.tsx`

- [ ] **Step 1 : Créer `useNetworkGuard.ts`**

```typescript
// backoffice/src/hooks/useNetworkGuard.ts
import { useNetwork } from '../context/NetworkContext'
import { useAuth } from '../context/AuthContext'
import { useSite } from '../context/SiteContext'

export function useNetworkGuard(dimension: string): { locked: boolean } {
  const { isLocked } = useNetwork()
  const { role } = useAuth()
  const { activeSiteId } = useSite()
  return { locked: isLocked(dimension, role ?? '', activeSiteId) }
}
```

- [ ] **Step 2 : Créer `AdminRoute.tsx`**

```typescript
// backoffice/src/components/AdminRoute.tsx
import { Navigate, Outlet } from 'react-router-dom'
import { useAuth } from '../context/AuthContext'

export default function AdminRoute() {
  const { role } = useAuth()
  const allowed = role === 'pos_admin' || role === 'regional_director'
  return allowed ? <Outlet /> : <Navigate to="/dashboard" replace />
}
```

- [ ] **Step 3 : Vérifier la compilation**

```bash
cd backoffice && npm run build
```

Expected : `✓ built in XXs` sans erreur.

- [ ] **Step 4 : Commit**

```bash
git add backoffice/src/hooks/useNetworkGuard.ts backoffice/src/components/AdminRoute.tsx
git commit -m "feat(backoffice): useNetworkGuard + AdminRoute"
```

---

## Task 8 : `App.tsx` + `Layout.tsx` — câblage

**Files:**
- Modify: `backoffice/src/App.tsx`
- Modify: `backoffice/src/components/Layout.tsx`

- [ ] **Step 1 : Mettre à jour `App.tsx`**

Remplacer le contenu entier de `backoffice/src/App.tsx` :

```typescript
// backoffice/src/App.tsx
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { AuthProvider } from './context/AuthContext'
import { SiteProvider } from './context/SiteContext'
import { NetworkProvider } from './context/NetworkContext'
import ProtectedRoute from './components/ProtectedRoute'
import AdminRoute from './components/AdminRoute'
import Layout from './components/Layout'
import LoginPage from './pages/LoginPage'
import Dashboard from './pages/Dashboard'
import FiscalJournal from './pages/FiscalJournal'
import ZReports from './pages/ZReports'
import MenuManager from './pages/MenuManager'
import CategoryManager from './pages/CategoryManager'
import ModifierGroupManager from './pages/ModifierGroupManager'
import ProductList from './pages/ProductList'
import ProductForm from './pages/ProductForm'
import ComboList from './pages/ComboList'
import ComboForm from './pages/ComboForm'
import GroupList from './pages/GroupList'
import GroupForm from './pages/GroupForm'
import PromotionList from './pages/PromotionList'
import PromotionForm from './pages/PromotionForm'
import SiteList from './pages/admin/SiteList'
import SiteForm from './pages/admin/SiteForm'
import TechnicalConfigForm from './pages/admin/TechnicalConfigForm'
import UserList from './pages/admin/UserList'
import UserForm from './pages/admin/UserForm'
import PermissionsMatrix from './pages/admin/PermissionsMatrix'

export default function App() {
  return (
    <BrowserRouter>
      <AuthProvider>
        <SiteProvider>
          <NetworkProvider>
            <Routes>
              <Route path="/login" element={<LoginPage />} />
              <Route element={<ProtectedRoute />}>
                <Route element={<Layout />}>
                  <Route path="/" element={<Navigate to="/dashboard" replace />} />
                  <Route path="/dashboard"      element={<Dashboard />} />
                  <Route path="/fiscal-journal" element={<FiscalJournal />} />
                  <Route path="/z-reports"      element={<ZReports />} />
                  <Route path="/categories"     element={<CategoryManager />} />
                  <Route path="/products"       element={<ProductList />} />
                  <Route path="/products/new"   element={<ProductForm />} />
                  <Route path="/products/:id"   element={<ProductForm />} />
                  <Route path="/combos"         element={<ComboList />} />
                  <Route path="/combos/new"     element={<ComboForm />} />
                  <Route path="/combos/:id"     element={<ComboForm />} />
                  <Route path="/modifiers"      element={<ModifierGroupManager />} />
                  <Route path="/menu"           element={<MenuManager />} />
                  <Route path="/promotions"     element={<PromotionList />} />
                  <Route path="/promotions/new" element={<PromotionForm />} />
                  <Route path="/promotions/:id" element={<PromotionForm />} />
                  <Route path="/groups"         element={<GroupList />} />
                  <Route path="/groups/new"     element={<GroupForm />} />
                  <Route path="/groups/:id"     element={<GroupForm />} />

                  <Route element={<AdminRoute />}>
                    <Route path="/admin/sites"            element={<SiteList />} />
                    <Route path="/admin/sites/new"        element={<SiteForm />} />
                    <Route path="/admin/sites/:id"        element={<SiteForm />} />
                    <Route path="/admin/sites/:id/config" element={<TechnicalConfigForm />} />
                    <Route path="/admin/users"            element={<UserList />} />
                    <Route path="/admin/users/new"        element={<UserForm />} />
                    <Route path="/admin/users/:id"        element={<UserForm />} />
                    <Route path="/admin/permissions"      element={<PermissionsMatrix />} />
                  </Route>
                </Route>
              </Route>
            </Routes>
          </NetworkProvider>
        </SiteProvider>
      </AuthProvider>
    </BrowserRouter>
  )
}
```

- [ ] **Step 2 : Mettre à jour `Layout.tsx` — section "Réseau"**

Ajouter après l'import de `useRole` :
```typescript
import { useAuth } from '../context/AuthContext'
```

(Note : `useAuth` est déjà importé via `signOut` — vérifier d'abord qu'il est bien destructuré depuis le contexte existant. Si `useAuth` est déjà importé, il suffit d'extraire `role` de son résultat.)

Dans le corps du composant `Layout`, ajouter `role` :
```typescript
const { signOut, role } = useAuth()
```

Ajouter avant le bouton `Déconnexion` la section admin conditionnelle :

```typescript
{(role === 'pos_admin' || role === 'regional_director') && (
  <>
    <div style={{ marginTop: '0.75rem', marginBottom: '0.25rem' }}>
      <div style={{ borderTop: '1px solid #2a2a4a', marginBottom: '0.5rem' }} />
      <span style={{ fontSize: '0.65rem', textTransform: 'uppercase', letterSpacing: '0.12em', color: '#444', paddingLeft: '0.75rem' }}>
        Réseau
      </span>
    </div>
    {[
      { to: '/admin/sites',       label: '🏢 Restaurants' },
      { to: '/admin/users',       label: '👥 Utilisateurs' },
      { to: '/admin/permissions', label: '🔐 Permissions' },
    ].map(item => (
      <NavLink
        key={item.to}
        to={item.to}
        style={({ isActive }) => ({
          display: 'block',
          padding: '0.55rem 0.75rem',
          borderRadius: 6,
          textDecoration: 'none',
          color: isActive ? '#fff' : '#888',
          background: isActive ? '#16213e' : 'transparent',
          borderLeft: isActive ? '3px solid #4f8ef7' : '3px solid transparent',
          transition: 'all 0.15s',
          fontSize: '0.85rem',
        })}
      >
        {item.label}
      </NavLink>
    ))}
  </>
)}
```

- [ ] **Step 3 : Vérifier la compilation**

```bash
cd backoffice && npm run build
```

Expected : `✓ built in XXs` — les imports des pages admin (encore inexistantes) feront échouer la build ; créer des fichiers stub vides si nécessaire.

> Note : si le build échoue à cause des imports des pages admin, créer des fichiers stub temporaires :
> ```bash
> mkdir -p backoffice/src/pages/admin
> for f in SiteList SiteForm TechnicalConfigForm UserList UserForm PermissionsMatrix; do
>   echo "export default function $f() { return null }" > backoffice/src/pages/admin/$f.tsx
> done
> ```
> Lancer `npm run build` à nouveau, puis supprimer les stubs au fur et à mesure des tâches 9–12.

- [ ] **Step 4 : Commit**

```bash
git add backoffice/src/App.tsx backoffice/src/components/Layout.tsx
git commit -m "feat(backoffice): câblage NetworkProvider + AdminRoute + nav Réseau"
```

---

## Task 9 : `SiteList.tsx` + `SiteForm.tsx`

**Files:**
- Create: `backoffice/src/pages/admin/SiteList.tsx`
- Create: `backoffice/src/pages/admin/SiteForm.tsx`

- [ ] **Step 1 : Créer `SiteList.tsx`**

```typescript
// backoffice/src/pages/admin/SiteList.tsx
import { useEffect, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { supabase } from '../../supabaseClient'

interface Site { id: string; site_code: string; name: string; address: string | null; siret: string | null }

export default function SiteList() {
  const [sites, setSites] = useState<Site[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()

  useEffect(() => {
    setLoading(true)
    supabase.from('sites').select('id,site_code,name,address,siret').order('site_code')
      .then(({ data, error: e }) => {
        if (e) setError(e.message)
        else setSites((data as Site[]) ?? [])
        setLoading(false)
      })
  }, [])

  if (loading) return <p style={{ color: '#888' }}>Chargement…</p>

  return (
    <div style={{ padding: '1.5rem' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0 }}>
          Restaurants{' '}
          <span style={{ color: '#888', fontWeight: 400, fontSize: '0.9rem' }}>{sites.length} site(s)</span>
        </h2>
        <button
          onClick={() => navigate('/admin/sites/new')}
          style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}
        >
          + Nouveau restaurant
        </button>
      </div>
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}
      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['CODE', 'NOM', 'ADRESSE', 'SIRET', 'ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {sites.map(s => (
            <tr key={s.id} style={{ borderBottom: '1px solid #f0f0f0' }}>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 600, fontFamily: 'monospace' }}>{s.site_code}</td>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500 }}>{s.name}</td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#666', fontSize: '0.85rem' }}>{s.address ?? '—'}</td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#888', fontSize: '0.8rem', fontFamily: 'monospace' }}>{s.siret ?? '—'}</td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                <Link to={`/admin/sites/${s.id}`} style={{ color: '#4f8ef7', textDecoration: 'none', marginRight: 12 }}>Éditer</Link>
                <Link to={`/admin/sites/${s.id}/config`} style={{ color: '#666', textDecoration: 'none' }}>Config</Link>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
```

- [ ] **Step 2 : Créer `SiteForm.tsx`**

```typescript
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
```

- [ ] **Step 3 : Vérifier**

```bash
cd backoffice && npm run build
```

Expected : `✓ built in XXs`.

- [ ] **Step 4 : Commit**

```bash
git add backoffice/src/pages/admin/SiteList.tsx backoffice/src/pages/admin/SiteForm.tsx
git commit -m "feat(backoffice): SiteList + SiteForm (CRUD restaurants)"
```

---

## Task 10 : `UserList.tsx` + `UserForm.tsx`

**Files:**
- Create: `backoffice/src/pages/admin/UserList.tsx`
- Create: `backoffice/src/pages/admin/UserForm.tsx`

- [ ] **Step 1 : Créer `UserList.tsx`**

```typescript
// backoffice/src/pages/admin/UserList.tsx
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { supabase } from '../../supabaseClient'

interface AdminUser {
  id: string
  email: string
  role: string | null
  site_id: string | null
  display_name: string | null
  is_banned: boolean
  created_at: string
}

export default function UserList() {
  const [users, setUsers] = useState<AdminUser[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [revoking, setRevoking] = useState<string | null>(null)
  const navigate = useNavigate()

  const load = () => {
    setLoading(true)
    supabase.rpc('list_admin_users')
      .then(({ data, error: e }) => {
        if (e) setError(e.message)
        else setUsers((data as AdminUser[]) ?? [])
        setLoading(false)
      })
  }

  useEffect(() => { load() }, [])

  const handleRevoke = async (userId: string) => {
    if (!confirm('Révoquer cet utilisateur ? Il ne pourra plus se connecter.')) return
    setRevoking(userId)
    const { error: e } = await supabase.functions.invoke('user-admin', {
      body: { action: 'revoke', payload: { user_id: userId } },
    })
    if (e) { alert('Erreur : ' + e.message) }
    else { load() }
    setRevoking(null)
  }

  if (loading) return <p style={{ color: '#888' }}>Chargement…</p>

  return (
    <div style={{ padding: '1.5rem' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0 }}>
          Utilisateurs{' '}
          <span style={{ color: '#888', fontWeight: 400, fontSize: '0.9rem' }}>{users.length} user(s)</span>
        </h2>
        <button
          onClick={() => navigate('/admin/users/new')}
          style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}
        >
          + Ajouter
        </button>
      </div>
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}
      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['NOM', 'EMAIL', 'RÔLE', 'SITE', 'STATUT', 'ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {users.map(u => (
            <tr key={u.id} style={{ borderBottom: '1px solid #f0f0f0', opacity: u.is_banned ? 0.5 : 1 }}>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500 }}>{u.display_name ?? '—'}</td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#666', fontSize: '0.85rem' }}>{u.email}</td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                <span style={{ background: '#e8f0fe', color: '#1a56db', borderRadius: 4, padding: '2px 6px', fontSize: '0.78rem' }}>
                  {u.role ?? '—'}
                </span>
              </td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#888', fontSize: '0.8rem', fontFamily: 'monospace' }}>{u.site_id ?? '—'}</td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                {u.is_banned
                  ? <span style={{ color: '#e53e3e', fontSize: '0.8rem' }}>Révoqué</span>
                  : <span style={{ color: '#48bb78', fontSize: '0.8rem' }}>Actif</span>}
              </td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                <button
                  onClick={() => navigate(`/admin/users/${u.id}`, { state: { user: u } })}
                  style={{ color: '#4f8ef7', background: 'none', border: 'none', cursor: 'pointer', marginRight: 8, fontSize: '0.85rem' }}
                >
                  Éditer
                </button>
                {!u.is_banned && (
                  <button
                    onClick={() => handleRevoke(u.id)}
                    disabled={revoking === u.id}
                    style={{ color: '#e53e3e', background: 'none', border: 'none', cursor: 'pointer', fontSize: '0.85rem' }}
                  >
                    {revoking === u.id ? '…' : 'Révoquer'}
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
```

- [ ] **Step 2 : Créer `UserForm.tsx`**

```typescript
// backoffice/src/pages/admin/UserForm.tsx
import { useEffect, useState } from 'react'
import { useLocation, useNavigate, useParams } from 'react-router-dom'
import { supabase } from '../../supabaseClient'

const inputStyle = { padding: '0.5rem 0.75rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.9rem', width: '100%', boxSizing: 'border-box' as const }
const btnPrimary = { background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.6rem 1.2rem', cursor: 'pointer', fontWeight: 600 } as const
const btnSecondary = { background: '#f5f6fa', border: '1px solid #ddd', borderRadius: 6, padding: '0.6rem 1rem', cursor: 'pointer' } as const

function FieldLabel({ txt }: { txt: string }) {
  return <label style={{ display: 'block', fontWeight: 600, marginBottom: 4, fontSize: '0.85rem' }}>{txt}</label>
}

const ROLES = ['pos_admin', 'regional_director', 'director', 'manager', 'pos_caissier', 'pos_auditeur']
const SITE_ROLES = ['pos_caissier', 'manager']

interface Site { id: string; site_code: string; name: string }

interface AdminUser {
  id: string; email: string; role: string | null
  site_id: string | null; display_name: string | null
}

export default function UserForm() {
  const { id } = useParams<{ id: string }>()
  const { state } = useLocation()
  const navigate = useNavigate()
  const isEdit = id !== undefined
  const existing = (state as { user?: AdminUser } | null)?.user ?? null

  const [mode, setMode] = useState<'invite' | 'direct'>('invite')
  const [email, setEmail] = useState(existing?.email ?? '')
  const [displayName, setDisplayName] = useState(existing?.display_name ?? '')
  const [employeeId, setEmployeeId] = useState('')
  const [role, setRole] = useState(existing?.role ?? 'pos_caissier')
  const [siteId, setSiteId] = useState(existing?.site_id ?? '')
  const [sites, setSites] = useState<Site[]>([])
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [createdCreds, setCreatedCreds] = useState<{ email: string; temp_password: string } | null>(null)

  const needsSite = SITE_ROLES.includes(role)

  useEffect(() => {
    supabase.from('sites').select('id,site_code,name').order('site_code')
      .then(({ data }) => setSites((data as Site[]) ?? []))
  }, [])

  const handleSubmit = async () => {
    setSaving(true); setError(null)
    try {
      if (isEdit) {
        // Mettre à jour le rôle
        const payload: Record<string, string> = { user_id: id!, role }
        if (needsSite && siteId) payload.site_id = siteId
        const { error: e } = await supabase.functions.invoke('user-admin', {
          body: { action: 'update_role', payload },
        })
        if (e) throw e
        navigate('/admin/users')
        return
      }

      if (mode === 'invite') {
        const payload: Record<string, string> = { email, role, display_name: displayName }
        if (needsSite && siteId) payload.site_id = siteId
        const { error: e } = await supabase.functions.invoke('user-admin', {
          body: { action: 'invite', payload },
        })
        if (e) throw e
        navigate('/admin/users')
      } else {
        const payload: Record<string, string> = { display_name: displayName, employee_id: employeeId, role }
        if (needsSite && siteId) payload.site_id = siteId
        const { data, error: e } = await supabase.functions.invoke<{ email: string; temp_password: string }>('user-admin', {
          body: { action: 'create_direct', payload },
        })
        if (e) throw e
        setCreatedCreds({ email: data!.email, temp_password: data!.temp_password })
      }
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Erreur')
    } finally { setSaving(false) }
  }

  // Dialog mot de passe temporaire
  if (createdCreds) {
    return (
      <div style={{ padding: '1.5rem', maxWidth: 460 }}>
        <div style={{ background: '#fffbeb', border: '1px solid #f6c90e', borderRadius: 8, padding: '1.5rem' }}>
          <h3 style={{ marginTop: 0, color: '#92400e' }}>Utilisateur créé — copiez ce mot de passe</h3>
          <p style={{ marginBottom: 4, color: '#666', fontSize: '0.85rem' }}>Email de connexion :</p>
          <code style={{ display: 'block', background: '#f5f6fa', padding: '0.4rem 0.6rem', borderRadius: 4, marginBottom: '1rem', fontSize: '0.85rem' }}>
            {createdCreds.email}
          </code>
          <p style={{ marginBottom: 4, color: '#666', fontSize: '0.85rem' }}>Mot de passe temporaire :</p>
          <code style={{ display: 'block', background: '#f5f6fa', padding: '0.6rem', borderRadius: 4, fontSize: '1.2rem', fontWeight: 700, letterSpacing: '0.1em', marginBottom: '0.75rem' }}>
            {createdCreds.temp_password}
          </code>
          <p style={{ color: '#e53e3e', fontSize: '0.85rem', marginBottom: '1rem' }}>
            Ce mot de passe ne sera plus affiché après fermeture.
          </p>
          <button onClick={() => navigate('/admin/users')} style={btnPrimary}>
            J'ai copié le mot de passe
          </button>
        </div>
      </div>
    )
  }

  return (
    <div style={{ padding: '1.5rem', maxWidth: 500 }}>
      <h2 style={{ marginTop: 0 }}>{isEdit ? 'Éditer utilisateur' : 'Nouvel utilisateur'}</h2>
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}

      {!isEdit && (
        <div style={{ display: 'flex', gap: 4, marginBottom: '1.5rem', background: '#f5f6fa', borderRadius: 8, padding: 4 }}>
          {(['invite', 'direct'] as const).map(m => (
            <button
              key={m}
              onClick={() => setMode(m)}
              style={{ flex: 1, padding: '0.5rem', borderRadius: 6, border: 'none', cursor: 'pointer',
                background: mode === m ? '#fff' : 'transparent',
                fontWeight: mode === m ? 600 : 400,
                boxShadow: mode === m ? '0 1px 3px rgba(0,0,0,0.1)' : 'none' }}
            >
              {m === 'invite' ? 'Invitation email' : 'Création directe'}
            </button>
          ))}
        </div>
      )}

      {mode === 'invite' && !isEdit && (
        <div style={{ marginBottom: '1rem' }}>
          <FieldLabel txt="Email *" />
          <input style={inputStyle} type="email" value={email} onChange={e => setEmail(e.target.value)} />
        </div>
      )}

      {(mode === 'direct' || isEdit) && (
        <div style={{ marginBottom: '1rem' }}>
          <FieldLabel txt="Nom affiché *" />
          <input style={inputStyle} value={displayName} onChange={e => setDisplayName(e.target.value)} placeholder="Jean Martin" />
        </div>
      )}

      {mode === 'direct' && !isEdit && (
        <div style={{ marginBottom: '1rem' }}>
          <FieldLabel txt="ID employé *" />
          <input style={inputStyle} value={employeeId} onChange={e => setEmployeeId(e.target.value)} placeholder="EMP-0042" />
        </div>
      )}

      <div style={{ marginBottom: '1rem' }}>
        <FieldLabel txt="Rôle *" />
        <select style={inputStyle} value={role} onChange={e => setRole(e.target.value)}>
          {ROLES.map(r => <option key={r} value={r}>{r}</option>)}
        </select>
      </div>

      {needsSite && (
        <div style={{ marginBottom: '1.5rem' }}>
          <FieldLabel txt="Site *" />
          <select style={inputStyle} value={siteId} onChange={e => setSiteId(e.target.value)}>
            <option value="">— Sélectionner un site —</option>
            {sites.map(s => <option key={s.id} value={s.id}>{s.name} ({s.site_code})</option>)}
          </select>
        </div>
      )}

      <div style={{ display: 'flex', gap: 8 }}>
        <button
          onClick={handleSubmit}
          disabled={saving || (!isEdit && mode === 'invite' && !email.trim()) || (!isEdit && mode === 'direct' && (!displayName.trim() || !employeeId.trim()))}
          style={btnPrimary}
        >
          {saving ? '…' : isEdit ? 'Mettre à jour' : mode === 'invite' ? 'Envoyer l\'invitation' : 'Créer'}
        </button>
        <button onClick={() => navigate('/admin/users')} style={btnSecondary}>Annuler</button>
      </div>
    </div>
  )
}
```

- [ ] **Step 3 : Vérifier**

```bash
cd backoffice && npm run build
```

Expected : `✓ built in XXs`.

- [ ] **Step 4 : Commit**

```bash
git add backoffice/src/pages/admin/UserList.tsx backoffice/src/pages/admin/UserForm.tsx
git commit -m "feat(backoffice): UserList + UserForm (invite + création directe)"
```

---

## Task 11 : `TechnicalConfigForm.tsx`

**Files:**
- Create: `backoffice/src/pages/admin/TechnicalConfigForm.tsx`

- [ ] **Step 1 : Créer le fichier**

```typescript
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
      .select('*')
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
        <input style={inputStyle} type="number" value={syncInterval} onChange={e => setSyncInterval(e.target.value)} min={30} />
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
```

- [ ] **Step 2 : Vérifier**

```bash
cd backoffice && npm run build
```

Expected : `✓ built in XXs`.

- [ ] **Step 3 : Commit**

```bash
git add backoffice/src/pages/admin/TechnicalConfigForm.tsx
git commit -m "feat(backoffice): TechnicalConfigForm — config serveur + clé Ed25519"
```

---

## Task 12 : `PermissionsMatrix.tsx`

**Files:**
- Create: `backoffice/src/pages/admin/PermissionsMatrix.tsx`

- [ ] **Step 1 : Créer le fichier**

```typescript
// backoffice/src/pages/admin/PermissionsMatrix.tsx
import { useState } from 'react'
import { supabase } from '../../supabaseClient'
import { useNetwork, type NetworkPermission } from '../../context/NetworkContext'
import { useAuth } from '../../context/AuthContext'

const DIMENSIONS = ['menu', 'prices', 'promotions', 'discounts', 'user_management', 'z_reports'] as const
const TARGET_ROLES = ['manager', 'director', 'regional_director'] as const
const DIM_LABELS: Record<string, string> = {
  menu: 'Carte / menu',
  prices: 'Prix TTC',
  promotions: 'Promotions',
  discounts: 'Remises caisse',
  user_management: 'Utilisateurs',
  z_reports: 'Rapports Z',
}

type Scope = 'network' | 'group' | 'site'

interface CellEdit {
  dimension: string
  target_role: string
  locked: boolean
  lock_from: string
  lock_until: string
  reason: string
}

function ruleForScope(
  permissions: NetworkPermission[],
  scope: Scope,
  scopeId: string | null,
  dimension: string,
  targetRole: string
): NetworkPermission | null {
  return (
    permissions.find(
      (p) =>
        p.dimension === dimension &&
        p.target_role === targetRole &&
        (scope === 'network'
          ? p.site_id === null && p.group_id === null
          : scope === 'group'
          ? p.group_id === scopeId && p.site_id === null
          : p.site_id === scopeId && p.group_id === null)
    ) ?? null
  )
}

export default function PermissionsMatrix() {
  const { allSites, groups, permissions, reload } = useNetwork()
  const { role } = useAuth()

  const [scope, setScope] = useState<Scope>('network')
  const [scopeId, setScopeId] = useState<string | null>(null)
  const [editing, setEditing] = useState<CellEdit | null>(null)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const canEditNetwork = role === 'pos_admin'

  function openCell(dimension: string, targetRole: string) {
    const rule = ruleForScope(permissions, scope, scopeId, dimension, targetRole)
    setEditing({
      dimension,
      target_role: targetRole,
      locked: rule?.locked ?? false,
      lock_from: rule?.lock_from ?? '',
      lock_until: rule?.lock_until ?? '',
      reason: rule?.reason ?? '',
    })
    setError(null)
  }

  async function saveCell() {
    if (!editing) return
    if (editing.lock_from && !editing.lock_until) { setError('Heure de fin requise'); return }
    if (!editing.lock_from && editing.lock_until) { setError('Heure de début requise'); return }
    setSaving(true); setError(null)
    try {
      const existing = ruleForScope(permissions, scope, scopeId, editing.dimension, editing.target_role)
      const payload = {
        dimension: editing.dimension,
        target_role: editing.target_role,
        locked: editing.locked,
        lock_from: editing.lock_from || null,
        lock_until: editing.lock_until || null,
        reason: editing.reason || null,
        site_id: scope === 'site' ? scopeId : null,
        group_id: scope === 'group' ? scopeId : null,
        updated_at: new Date().toISOString(),
      }
      if (existing) {
        const { error: e } = await supabase.from('network_permissions').update(payload).eq('id', existing.id)
        if (e) throw e
      } else {
        const { error: e } = await supabase.from('network_permissions').insert(payload)
        if (e) throw e
      }
      reload()
      setEditing(null)
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Erreur')
    } finally { setSaving(false) }
  }

  return (
    <div style={{ padding: '1.5rem' }}>
      <h2 style={{ marginTop: 0 }}>Matrice de permissions</h2>

      {/* Sélecteur de scope */}
      <div style={{ display: 'flex', gap: 12, alignItems: 'center', marginBottom: '1.5rem', flexWrap: 'wrap' }}>
        {(['network', 'group', 'site'] as Scope[]).map((s) => (
          <label key={s} style={{ display: 'flex', alignItems: 'center', gap: 6, cursor: 'pointer' }}>
            <input
              type="radio"
              checked={scope === s}
              onChange={() => { setScope(s); setScopeId(null); setEditing(null) }}
              disabled={s === 'network' && !canEditNetwork}
            />
            {s === 'network' ? 'Réseau entier' : s === 'group' ? 'Groupe' : 'Site'}
          </label>
        ))}

        {scope === 'group' && (
          <select
            style={{ padding: '0.4rem 0.6rem', border: '1px solid #ddd', borderRadius: 6 }}
            value={scopeId ?? ''}
            onChange={(e) => setScopeId(e.target.value || null)}
          >
            <option value="">— Choisir un groupe —</option>
            {groups.map((g) => <option key={g.id} value={g.id}>{g.name}</option>)}
          </select>
        )}

        {scope === 'site' && (
          <select
            style={{ padding: '0.4rem 0.6rem', border: '1px solid #ddd', borderRadius: 6 }}
            value={scopeId ?? ''}
            onChange={(e) => setScopeId(e.target.value || null)}
          >
            <option value="">— Choisir un site —</option>
            {allSites.map((s) => <option key={s.id} value={s.id}>{s.name} ({s.site_code})</option>)}
          </select>
        )}
      </div>

      {/* Matrice */}
      <table style={{ width: '100%', borderCollapse: 'collapse', marginBottom: '1.5rem' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            <th style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666', width: 160 }}>DIMENSION</th>
            {TARGET_ROLES.map((r) => (
              <th key={r} style={{ textAlign: 'center', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>
                {r.replace('_', ' ').toUpperCase()}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {DIMENSIONS.map((dim) => (
            <tr key={dim} style={{ borderBottom: '1px solid #f0f0f0' }}>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500, fontSize: '0.88rem' }}>{DIM_LABELS[dim]}</td>
              {TARGET_ROLES.map((tr) => {
                const rule = ruleForScope(permissions, scope, scopeId, dim, tr)
                const isLocked = rule?.locked ?? false
                const hasWindow = !!(rule?.lock_from && rule?.lock_until)
                return (
                  <td
                    key={tr}
                    style={{ textAlign: 'center', padding: '0.6rem' }}
                  >
                    <button
                      onClick={() => openCell(dim, tr)}
                      disabled={scope === 'network' && !canEditNetwork}
                      style={{
                        background: isLocked ? '#fee2e2' : '#dcfce7',
                        color: isLocked ? '#991b1b' : '#166534',
                        border: 'none', borderRadius: 6,
                        padding: '0.35rem 0.65rem',
                        cursor: scope === 'network' && !canEditNetwork ? 'default' : 'pointer',
                        fontSize: '0.8rem', fontWeight: 600,
                        minWidth: 90,
                      }}
                    >
                      {isLocked ? (hasWindow ? `🔒 ${rule!.lock_from!.slice(0, 5)}–${rule!.lock_until!.slice(0, 5)}` : '🔒 verrouillé') : '🔓 libre'}
                    </button>
                  </td>
                )
              })}
            </tr>
          ))}
        </tbody>
      </table>

      {/* Formulaire d'édition d'une cellule */}
      {editing && (
        <div style={{ background: '#fff', border: '1px solid #e2e8f0', borderRadius: 8, padding: '1.25rem', maxWidth: 440 }}>
          <h4 style={{ marginTop: 0, marginBottom: '0.75rem', fontSize: '0.95rem' }}>
            {DIM_LABELS[editing.dimension]} — {editing.target_role.replace('_', ' ')}
          </h4>

          <label style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: '1rem', cursor: 'pointer' }}>
            <input
              type="checkbox"
              checked={editing.locked}
              onChange={(e) => setEditing((prev) => prev ? { ...prev, locked: e.target.checked } : prev)}
            />
            <span style={{ fontWeight: 600 }}>Verrouillé</span>
          </label>

          {editing.locked && (
            <div style={{ display: 'flex', gap: 8, marginBottom: '1rem', alignItems: 'center' }}>
              <div style={{ flex: 1 }}>
                <label style={{ display: 'block', fontSize: '0.82rem', marginBottom: 3 }}>De</label>
                <input
                  type="time"
                  value={editing.lock_from}
                  onChange={(e) => setEditing((prev) => prev ? { ...prev, lock_from: e.target.value } : prev)}
                  style={{ padding: '0.4rem', border: '1px solid #ddd', borderRadius: 4, width: '100%' }}
                />
              </div>
              <div style={{ flex: 1 }}>
                <label style={{ display: 'block', fontSize: '0.82rem', marginBottom: 3 }}>À</label>
                <input
                  type="time"
                  value={editing.lock_until}
                  onChange={(e) => setEditing((prev) => prev ? { ...prev, lock_until: e.target.value } : prev)}
                  style={{ padding: '0.4rem', border: '1px solid #ddd', borderRadius: 4, width: '100%' }}
                />
              </div>
            </div>
          )}

          <div style={{ marginBottom: '1rem' }}>
            <label style={{ display: 'block', fontSize: '0.82rem', marginBottom: 3 }}>Motif (optionnel)</label>
            <input
              style={{ padding: '0.4rem 0.6rem', border: '1px solid #ddd', borderRadius: 4, width: '100%', boxSizing: 'border-box' }}
              value={editing.reason}
              onChange={(e) => setEditing((prev) => prev ? { ...prev, reason: e.target.value } : prev)}
              placeholder="ex: pas de remise pendant le service"
            />
          </div>

          {error && <p style={{ color: '#e53e3e', fontSize: '0.85rem', marginBottom: '0.75rem' }}>{error}</p>}

          <div style={{ display: 'flex', gap: 8 }}>
            <button
              onClick={saveCell}
              disabled={saving}
              style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}
            >
              {saving ? '…' : 'Enregistrer'}
            </button>
            <button
              onClick={() => setEditing(null)}
              style={{ background: '#f5f6fa', border: '1px solid #ddd', borderRadius: 6, padding: '0.5rem 0.8rem', cursor: 'pointer' }}
            >
              Annuler
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 2 : Vérifier la compilation complète**

```bash
cd backoffice && npm run build
```

Expected : `✓ built in XXs` sans erreur TypeScript.

- [ ] **Step 3 : Supprimer les éventuels fichiers stub créés à la Task 8**

Si des stubs ont été créés pour débloquer le build à Task 8, vérifier qu'ils ont bien été remplacés par les fichiers réels :
```bash
# Vérifier qu'aucun fichier ne contient encore "return null" seul
grep -rl "return null" backoffice/src/pages/admin/ && echo "stubs présents" || echo "ok"
```

- [ ] **Step 4 : Commit final**

```bash
git add backoffice/src/pages/admin/PermissionsMatrix.tsx
git commit -m "feat(backoffice): PermissionsMatrix — matrice lock/unlock par scope"

git add -A && git status
# Vérifier qu'il ne reste rien de non-commité lié à cette feature
```

---

## Checklist de vérification post-implémentation

Après avoir complété les 12 tâches :

- [ ] `supabase db push` sans erreur (3 migrations appliquées : 016, 017, 018)
- [ ] `supabase functions deploy user-admin` ✓
- [ ] `supabase functions deploy config-provision` ✓
- [ ] `cd backoffice && npm run build` ✓ (0 erreur TypeScript)
- [ ] Connexion avec un compte `pos_admin` → section "Réseau" visible dans le nav
- [ ] Connexion avec un compte `manager` → section "Réseau" absente
- [ ] `/admin/sites` → liste les sites, bouton "+ Nouveau restaurant" fonctionne
- [ ] `/admin/sites/new` → formulaire crée un site, retour vers SiteList
- [ ] `/admin/sites/:id/config` → paramètres éditables, champ clé Ed25519 type password
- [ ] `/admin/users/new` → toggle invite/direct, dialog mot de passe après `create_direct`
- [ ] `/admin/permissions` → matrice s'affiche, clic sur une cellule ouvre le formulaire inline
- [ ] `AdminRoute` redirige `/admin/*` vers `/dashboard` pour les rôles non-admin
