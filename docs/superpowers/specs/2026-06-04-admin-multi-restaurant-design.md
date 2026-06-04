# Back-office admin multi-restaurant — Design

**Date :** 2026-06-04  
**Statut :** Approuvé — prêt pour plan d'implémentation

---

## Contexte

Projet pos-fiscal NF525 open-source, chaîne QSR France. Architecture offline-first :
`pos-app (Electron)` → `edge-api (Axum/SQLite)` → `sync-client` → `Supabase`.

Le back-office existant (`backoffice/`) est mono-restaurant. Ce chantier ajoute une couche admin multi-restaurant dans la même app.

### Ce qui existe déjà (pertinent pour ce chantier)

| Élément | État |
|---|---|
| Table `sites` (site_code, name, address, siret) | Existe, pas d'UI CRUD |
| Table `site_configs` (site_id, version, menu jsonb) | Existe, tirée par sync-client |
| Table `restaurant_groups` + `restaurant_group_members` | Existe, UI GroupList/GroupForm présente |
| `useRole` hook (manager / director / regional_director) | Existe |
| `SiteContext` (liste des sites accessibles, site actif) | Existe |
| `AuthContext` — `role` lu depuis `app_metadata.role` | Existe |
| Rôles app_metadata : `pos_admin`, `pos_caissier`, `pos_auditeur`, `manager`, `director`, `regional_director` | Existent |
| Migrations 001–015 appliquées | Existent |

---

## Périmètre V1

| # | Dimension | Description |
|---|---|---|
| A | **Restaurants** | CRUD sites (SiteList + SiteForm) |
| B | **Utilisateurs** | Invite email + création directe, attribution rôle + périmètre |
| F | **Config technique** | Paramètres serveur + clés Ed25519 par site, depuis le central |
| G | **Matrice permissions** | lock/unlock × dimension × rôle × fenêtre temporelle |

Hors V1 : carte chaîne, promotions centralisées, prix par région.

---

## Section 1 — Architecture globale

### Approche retenue : C — Admin intégré avec bascule NetworkContext

- Le `backoffice` existant accueille une section `/admin/*`
- Un `NetworkContext` est ajouté à côté du `SiteContext` existant (inchangé)
- Le menu "Réseau" apparaît uniquement pour `pos_admin` et `regional_director`
- `NetworkContext` filtre automatiquement le périmètre selon le rôle :
  - `pos_admin` → tous les sites, tous les groupes
  - `regional_director` → ses groupes + leurs sites membres uniquement

### Topologie de déploiement

```
Cloud Supabase (source de vérité absolue)
  ├── site_configs (site_id × device_type × config jsonb)
  ├── site_technical_configs (paramètres edge-api non-secrets)
  ├── vault.secrets (clés Ed25519, chiffrées pgsodium)
  └── network_permissions (matrice lock/unlock)

Serveur local par restaurant
  ├── edge-api Axum (port 8080)
  ├── SQLite WAL (journal fiscal)
  ├── sync-client (pull TOUT depuis Supabase, écrit en local)
  └── /etc/pos-fiscal/secrets chmod=600 (écrit par sync-client après pull vault)

Devices (HTTP LAN vers serveur local)
  POS (Caisse) | KDS | Kiosk | Tablette [KDS/Kiosk/Tablette = roadmap]
```

**Principe :** tout paramètre vient du central. Rien ne reste en `.env` permanent sur le serveur local.

### Sécurité des clés Ed25519 (FISCAL_SIGNING_KEY_HEX)

| Étape | Mécanisme |
|---|---|
| Stockage | Supabase Vault (`pgsodium`), nommée `fiscal_key_{site_id}`, jamais lisible via `authenticated` |
| Saisie | Edge Function `config-provision` : reçoit la clé, l'insère en Vault, ne la retourne jamais. L'UI affiche uniquement "configurée le JJ/MM/AAAA" |
| Distribution | sync-client via `service_role` → `vault.decrypted_secrets WHERE name = 'fiscal_key_{site_id}'` → écrit `/etc/pos-fiscal/secrets` → redémarre edge-api si changement |

### Extension site_configs pour device types

Ajout colonne `device_type text NOT NULL DEFAULT 'pos'`, contrainte unique `(site_id, device_type)`.  
V1 : uniquement `'pos'`. Roadmap : `'kds'`, `'kiosk'`, `'tablet'`.

---

## Section 2 — Schéma de données

Trois migrations : 016, 017, 018.

### Migration 016 — `site_technical_configs`

Paramètres serveur par site × device_type. Pas de secrets ici — les clés vivent en Vault.

```sql
CREATE TABLE public.site_technical_configs (
  id                       uuid    PRIMARY KEY DEFAULT gen_random_uuid(),
  site_id                  uuid    NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
  device_type              text    NOT NULL DEFAULT 'pos',
  edge_api_port            integer NOT NULL DEFAULT 8080,
  sync_interval_s          integer NOT NULL DEFAULT 300,
  -- Trace uniquement : NULL = clé pas encore configurée
  fiscal_key_configured_at timestamptz,
  updated_at               timestamptz NOT NULL DEFAULT now(),
  UNIQUE (site_id, device_type)
);
```

La colonne `fiscal_key_configured_at` est mise à jour par la fonction `provision_fiscal_key` (voir Section 3). L'UI affiche "configurée le JJ/MM/AAAA" ou "non configurée" — la clé elle-même n'est jamais relisible.

Également dans cette migration : la fonction SECURITY DEFINER `can_access_site` et la fonction `provision_fiscal_key` (voir Section 3 et Section 5).

### Migration 017 — `network_permissions`

Matrice lock/unlock. Chaque ligne = une règle `dimension × rôle cible` pour un scope donné.

**Scope :**
- `site_id IS NULL AND group_id IS NULL` → réseau entier (tous les sites)
- `group_id IS NOT NULL AND site_id IS NULL` → per-groupe
- `site_id IS NOT NULL AND group_id IS NULL` → per-site (surcharge la plus fine)

**Règle de priorité :** site > groupe > réseau (résolu côté client par `isLocked`).

```sql
CREATE TABLE public.network_permissions (
  id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  site_id     uuid REFERENCES public.sites(id) ON DELETE CASCADE,
  group_id    uuid REFERENCES public.restaurant_groups(id) ON DELETE CASCADE,
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
```

**Dimensions V1 :**

| Dimension | Ce qui est contrôlé |
|---|---|
| `menu` | Édition carte, catégories, items |
| `prices` | Modification prix TTC |
| `promotions` | Création / activation promotions |
| `discounts` | Remises en caisse |
| `user_management` | Création / modification utilisateurs |
| `z_reports` | Clôture session / rapport Z |

### Migration 018 — Extension `site_configs`

```sql
ALTER TABLE public.site_configs
  ADD COLUMN IF NOT EXISTS device_type text NOT NULL DEFAULT 'pos';

ALTER TABLE public.site_configs
  ADD CONSTRAINT uq_site_configs_site_device UNIQUE (site_id, device_type);
```

---

## Section 3 — Edge Functions

### `user-admin`

**Route :** `POST /functions/v1/user-admin`  
**Autorisation :** JWT `app_metadata.role = 'pos_admin'` obligatoire (vérifié en premier dans la fonction).

Quatre actions dans un seul endpoint :

| Action | Comportement |
|---|---|
| `invite` | `auth.admin.inviteUserByEmail(email, { data: { role, site_id?, display_name } })` |
| `create_direct` | `auth.admin.createUser()` avec email généré + mot de passe temporaire |
| `update_role` | `auth.admin.updateUserById(id, { app_metadata: { role, site_id? } })` |
| `revoke` | `auth.admin.updateUserById(id, { ban_duration: '876600h' })` — jamais de DELETE (traçabilité NF525) |

**Création directe (sans email pro) :**

- Email généré : `emp_{slug}@internal.pos-fiscal.local` (slug = employee_id normalisé)
- Mot de passe : 16 chars aléatoires (majuscules + chiffres), généré côté serveur
- Retourné **une seule fois** dans la réponse ; l'UI l'affiche dans un dialog "Copier avant de fermer"

```typescript
// Payload create_direct
{
  action: 'create_direct',
  payload: {
    display_name: 'Caissier 04',
    employee_id: 'EMP-0042',
    role: 'pos_caissier',
    site_id: 'uuid'
  }
}
// Réponse
{ user_id: 'uuid', temp_password: 'XK9R2M...', email: 'emp_emp0042@internal.pos-fiscal.local' }
```

`app_metadata` écrit dans tous les cas : `{ role, site_id?, display_name }`.  
Pour `regional_director` et `pos_admin`, `site_id` est omis.

### `config-provision`

**Route :** `POST /functions/v1/config-provision`  
**Autorisation :** JWT `app_metadata.role = 'pos_admin'` obligatoire.

**Flux :**

```
UI admin → config-provision (vérifie JWT)
         → RPC provision_fiscal_key(site_id, key_hex)  [service_role]
               ├── Valide : exactement 64 hex chars
               ├── Upsert vault.secrets WHERE name = 'fiscal_key_{site_id}'
               └── Upsert site_technical_configs.fiscal_key_configured_at = now()
         → Retourne { configured_at: "2026-06-04T..." }
         (jamais la clé)
```

**Fonction PostgreSQL SECURITY DEFINER** (définie dans migration 016) :

```sql
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
  -- Vault upsert via API officielle (évite ON CONFLICT sur l'index partiel)
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
```

---

## Section 4 — Pages et routing

### Intégration App.tsx

```tsx
<AuthProvider>
  <SiteProvider>
    <NetworkProvider>
      <Routes>
        <Route path="/login" element={<LoginPage />} />
        <Route element={<ProtectedRoute />}>
          <Route element={<Layout />}>
            {/* routes existantes inchangées */}

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
```

`AdminRoute` : vérifie `role === 'pos_admin' || role === 'regional_director'`, sinon `<Navigate to="/dashboard" />`.

### Nav — section "Réseau" (Layout.tsx)

Ajoutée conditionnellement si `role === 'pos_admin' || role === 'regional_director'` :

```
─────────────── Réseau ───────────────
🏢  Restaurants        → /admin/sites
👥  Utilisateurs       → /admin/users
🔐  Permissions        → /admin/permissions
```

Le lien "Groupes" existant (`/groups`, visible `director+`) reste inchangé.

### Pages — responsabilités

| Page | Données chargées | Actions |
|---|---|---|
| `SiteList` | Tous les sites (pos_admin) ou périmètre groupe (regional_director) | Éditer → SiteForm, Config → TechnicalConfigForm |
| `SiteForm` | CRUD : site_code, name, address, siret | Créer / Mettre à jour |
| `TechnicalConfigForm` | `site_technical_configs` du site | edge_api_port, sync_interval_s ; saisie clé Ed25519 (textarea 64 hex, non-relisible) |
| `UserList` | `auth.users` filtrés sur périmètre + app_metadata | Éditer → UserForm, Révoquer |
| `UserForm` | Modes **Invitation email** / **Création directe** (toggle) | Appelle Edge Function `user-admin` |
| `PermissionsMatrix` | `network_permissions` scope sélectionné | Toggle locked + fenêtre temporelle par cellule |

### UserForm — détail des deux modes

```
[ Invitation email ]  [ Création directe ]   ← toggle

Mode invitation :
  Email *        [________________________]
  Rôle *         [pos_admin / director / regional_director / manager / pos_caissier / pos_auditeur]
  Site           [dropdown — affiché si rôle = manager ou pos_caissier]

Mode création directe :
  Nom affiché *  [________________________]
  ID employé *   [EMP-____]
  Rôle *         [même dropdown]
  Site           [dropdown — affiché si rôle = manager ou pos_caissier]

→ "Créer" → appelle user-admin → dialog "Mot de passe temporaire : XK9...  [Copier]"
   Dialog fermé = mot de passe inaccessible à jamais.
```

### PermissionsMatrix — structure UI

```
Scope :  ● Réseau entier   ○ Groupe [dropdown]   ○ Site [dropdown]

              manager        director      regional_director
 menu          🔓 libre       🔓 libre        🔓 libre
 prices        🔒 11h–14h    🔓 libre        🔓 libre
 promotions    🔓 libre       🔓 libre        🔓 libre
 discounts     🔒 permanent  🔓 libre        🔓 libre
 user_mgmt     🔒 permanent  🔒 permanent    🔓 libre
 z_reports     🔓 libre       🔓 libre        🔓 libre
```

- Clic sur une cellule → popover : toggle lock + champs `lock_from` / `lock_until` + motif
- Règles héritées d'un scope supérieur affichées en grisé non-modifiable avec badge d'origine

---

## Section 5 — RLS + role guards

### Helper `can_access_site` (migration 016)

```sql
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
```

### RLS — `site_technical_configs` (migration 016)

```sql
ALTER TABLE public.site_technical_configs ENABLE ROW LEVEL SECURITY;

CREATE POLICY "stc_read" ON public.site_technical_configs
  FOR SELECT TO authenticated USING (public.can_access_site(site_id));

-- Écriture réservée à pos_admin (la clé Ed25519 ne se configure que depuis le central)
CREATE POLICY "stc_admin_write" ON public.site_technical_configs
  FOR ALL TO authenticated
  USING    (public.auth_app_role() = 'pos_admin')
  WITH CHECK (public.auth_app_role() = 'pos_admin');

CREATE POLICY "stc_service_role" ON public.site_technical_configs
  FOR ALL TO service_role USING (true) WITH CHECK (true);

GRANT SELECT, INSERT, UPDATE, DELETE ON public.site_technical_configs TO authenticated;
```

### RLS — `network_permissions` (migration 017)

```sql
ALTER TABLE public.network_permissions ENABLE ROW LEVEL SECURITY;

-- Lecture : tout authentifié (nécessaire pour charger les règles héritées côté client)
CREATE POLICY "np_read" ON public.network_permissions
  FOR SELECT TO authenticated USING (true);

-- Écriture pos_admin : toutes les lignes (réseau + groupe + site)
CREATE POLICY "np_admin_write" ON public.network_permissions
  FOR ALL TO authenticated
  USING    (public.auth_app_role() = 'pos_admin')
  WITH CHECK (public.auth_app_role() = 'pos_admin');

-- Écriture regional_director : per-site (ses sites) et per-group (ses groupes) uniquement
-- Les lignes réseau (site_id IS NULL AND group_id IS NULL) sont réservées à pos_admin.
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

### React — `NetworkContext`

```tsx
// context/NetworkContext.tsx
interface NetworkPermission {
  id: string
  site_id: string | null
  group_id: string | null
  dimension: string
  target_role: string
  locked: boolean
  lock_from: string | null   // "HH:MM"
  lock_until: string | null  // "HH:MM"
  reason: string | null
}

interface GroupWithMembers {
  id: string
  name: string
  site_ids: string[]
}

interface NetworkContextValue {
  allSites: Site[]
  groups: GroupWithMembers[]
  permissions: NetworkPermission[]
  isLocked: (dimension: string, targetRole: string, siteId: string | null) => boolean
}
```

`NetworkProvider` charge les trois collections au mount si `role === 'pos_admin' || role === 'regional_director'`. Sinon ne fait aucune requête.

**`isLocked(dimension, targetRole, siteId)` :**

```
1. Cherche ligne : site_id = siteId, group_id = null, dimension, target_role
2. Sinon cherche ligne per-groupe pour un groupe contenant siteId
3. Sinon cherche ligne réseau (null, null, dimension, target_role)
4. Sur la ligne trouvée :
   - locked = false → return false
   - lock_from défini → vérifie heure courante dans [lock_from, lock_until]
   - sinon → return true
5. Aucune ligne → return false (pas de règle = libre)
```

### React — `useNetworkGuard`

```tsx
// hooks/useNetworkGuard.ts
export function useNetworkGuard(dimension: string): { locked: boolean } {
  const { isLocked } = useNetworkContext()
  const { role } = useAuth()
  const { activeSiteId } = useSite()
  return { locked: isLocked(dimension, role ?? '', activeSiteId) }
}
```

Usage dans les pages existantes :
```tsx
const { locked } = useNetworkGuard('prices')
<button disabled={locked} title={locked ? 'Verrouillé par l\'admin réseau' : undefined}>
  Modifier le prix
</button>
```

### React — `AdminRoute`

```tsx
// components/AdminRoute.tsx
export default function AdminRoute() {
  const { role } = useAuth()
  const isAdmin = role === 'pos_admin' || role === 'regional_director'
  return isAdmin ? <Outlet /> : <Navigate to="/dashboard" replace />
}
```

---

## Structure de fichiers cible

```
backoffice/src/
  context/
    NetworkContext.tsx       ← nouveau
  hooks/
    useNetworkGuard.ts       ← nouveau
  components/
    AdminRoute.tsx           ← nouveau
  pages/admin/
    SiteList.tsx
    SiteForm.tsx
    UserList.tsx
    UserForm.tsx
    TechnicalConfigForm.tsx
    PermissionsMatrix.tsx

supabase/
  migrations/
    016_site_technical_configs.sql   ← table + can_access_site + provision_fiscal_key
    017_network_permissions.sql      ← table + RLS
    018_site_configs_device_type.sql ← ADD COLUMN + UNIQUE
  functions/
    user-admin/index.ts
    config-provision/index.ts
```
