# Plan de développement — pos-fiscal
_Mis à jour le 2026-05-21_

---

## État du projet

| Composant        | État                                                                      | Tests           |
|------------------|---------------------------------------------------------------------------|-----------------|
| fiscal-engine    | Complet — moteur NF525, hash chain, multi-TVA, Z-reports, archives       | ~132 ✅          |
| sync-client      | Complet — sync sessions + z_reports + entries, idempotence, offline       | ~34 ✅ + 2 E2E   |
| edge-api         | Complet — sessions, orders, menu, health, archive NF525 §7               | 18 ✅            |
| backoffice       | Dashboard + Journal + Rapports Z — pas d'auth, pas de filtre site        | —               |
| pos-app          | Écrans Order/Payment/Ticket/ZReport/Cancel câblés, multi-TVA             | ~24 ✅           |
| Supabase         | 8 migrations, RLS, vues SECURITY DEFINER, site_configs seedé             | —               |

---

## Sprints 1-3 ✅ (terminés)

Voir historique git. Tous les items A-I complétés.

---

## Sprint 4 — Back-office : auth + multi-sites + gestion menu

### Ordre d'implémentation recommandé : J → K → L

---

### J — Authentification back-office (Supabase Auth)

**Problème** : le back-office est entièrement public (clé anon, aucune protection).
Toute personne connaissant l'URL peut voir le journal fiscal et les rapports Z.

**Solution** : Supabase Auth email/password. Un admin se connecte → token JWT →
role `authenticated` → accès complet. Le client Supabase passe déjà ce token
automatiquement sur toutes les requêtes.

**Fichiers à créer / modifier :**
- `backoffice/src/pages/LoginPage.tsx` — formulaire email + mot de passe
- `backoffice/src/contexts/AuthContext.tsx` — `useAuth()`, session Supabase, logout
- `backoffice/src/components/ProtectedRoute.tsx` — redirige vers /login si non connecté
- `backoffice/src/App.tsx` — wrap `<AuthProvider>`, ajouter route `/login`, protéger les autres
- `backoffice/src/components/Layout.tsx` — bouton "Déconnexion" en bas de la nav

**Migration Supabase :**
Aucune — Supabase Auth est activé par défaut. Il faut juste créer un utilisateur admin
dans le dashboard Supabase (Authentication → Users → Invite user).

**RLS existant :** les vues `daily_revenue_by_site`, `fiscal_entries_enriched` et la table
`z_reports` ont des policies `anon` (SELECT). Avec auth, le role `authenticated` hérite
aussi de ces accès via les policies existantes (ou via `TO authenticated` à ajouter
si les policies sont `TO anon` seulement — à vérifier).

**Comportement attendu :**
- `/login` accessible sans auth
- toutes les autres routes → redirect `/login` si non connecté
- après login → redirect vers `/dashboard`
- bouton logout dans la nav → `supabase.auth.signOut()`

---

### K — Filtre multi-sites

**Problème** : Dashboard, Journal fiscal et Rapports Z affichent les données de **tous**
les sites sans filtrage. Avec plusieurs restaurants, les données sont mélangées.

**Solution** : sélecteur de site dans le header/nav. L'UUID du site sélectionné est
passé en filtre `.eq('site_id', siteId)` sur chaque requête. Le site actif est stocké
dans un context React partagé.

**Fichiers à créer / modifier :**
- `backoffice/src/contexts/SiteContext.tsx` — `useSite()`, liste des sites, site actif
- `backoffice/src/components/Layout.tsx` — ajouter un `<select>` de site dans la nav
- `backoffice/src/pages/Dashboard.tsx` — ajouter `.eq('site_id', siteId)` sur la query
- `backoffice/src/pages/FiscalJournal.tsx` — idem
- `backoffice/src/pages/ZReports.tsx` — idem

**Chargement des sites :**
```ts
supabase.from('sites').select('id, site_code, name').order('site_code')
```
La table `sites` est accessible par `authenticated` (ou `anon` selon les policies).

**Comportement attendu :**
- Dropdown "Tous les sites" ou site spécifique
- Sélection persistée en mémoire pendant la session
- Toutes les pages réagissent au changement de site (via `useSite()`)

---

### L — Gestion du menu (CRUD site_configs)

**Problème** : le menu des caisses est seedé manuellement en SQL. Impossible de le
modifier depuis l'interface sans accès au dashboard Supabase.

**Solution** : nouvelle page back-office "Carte" — liste + formulaire d'édition des
articles. Les modifications sont écrites dans `site_configs` sur Supabase. Le sync-client
les pousse ensuite vers les caisses (menu.json).

**Migration Supabase nécessaire :**
`009_site_configs_authenticated_rw.sql` :
```sql
-- Lecture : authenticated peut lire la config de n'importe quel site
CREATE POLICY "authenticated_read" ON public.site_configs
  FOR SELECT TO authenticated USING (true);

-- Écriture : authenticated peut modifier la config de n'importe quel site
CREATE POLICY "authenticated_write" ON public.site_configs
  FOR ALL TO authenticated USING (true) WITH CHECK (true);
```

**Fichiers à créer / modifier :**
- `backoffice/src/pages/MenuManager.tsx` — page principale
  - liste les articles du site sélectionné (charge `site_configs.menu.items`)
  - formulaire d'ajout / édition inline
  - bouton de suppression par article
  - sauvegarde → `upsert` dans `site_configs` (version + 1)
- `backoffice/src/components/Layout.tsx` — ajouter entrée nav "🍔 Carte"
- `backoffice/src/App.tsx` — ajouter route `/menu`

**Format du JSON `site_configs.menu` (déjà défini) :**
```json
{
  "items": [
    {
      "id": "burger-001",
      "name": "Burger Classic",
      "price_ttc_cents": 1290,
      "tva_rate": "intermediaire10",
      "category": "Burgers",
      "available": true
    }
  ]
}
```

**Valeurs `tva_rate` valides :** `"reduit5_5"` | `"intermediaire10"` | `"normal20"`

**Comportement attendu :**
- Lecture : charge la dernière version depuis `site_configs` pour le site sélectionné
- Ajout article : génère un `id` UUID, append dans `items`, upsert `site_configs`
- Édition : modifie l'article in-place, upsert `site_configs`
- Suppression : retire l'article du tableau, upsert `site_configs`
- Chaque sauvegarde incrémente `version` de 1 et met à jour `updated_at_ms`

---

## Backlog long terme (hors MVP)

- Impression thermique réelle (ESC/POS via Electron IPC — `printText` IPC câblé, driver manquant)
- Intégration TPE (Ingenico/Verifone)
- Remboursements et remises via pos-app (routes edge-api existantes, UI manquante)
- Génération automatique de la clé `FISCAL_SIGNING_KEY_HEX` au premier démarrage edge-api
- Déclenchement automatique de l'archive au 1er janvier (tâche planifiée sync-client)

---

## Points d'attention permanents

- `SUPABASE_SERVICE_KEY` ne doit jamais être committée (`.env.test` ignoré par git)
- `FISCAL_SIGNING_KEY_HEX` ne doit jamais être committée
- `fiscal_entries` est immuable côté Supabase (trigger `prevent_delete` actif)
- Ordre de sync : **sessions → z_reports → fiscal_entries** (FK cloud)
- `run_migrations` utilise `_applied_migrations` pour être idempotent
- Site de test UUID : `9983f3ac-cde8-4838-9386-49ef24f57dad`
- Supabase project ref : `iawyngsvqjsogvkwkrxw`
- Hash NF525 figé pour certification LNE — ne pas modifier `HashInput`
- `site_configs` RLS : service_role only (migration 008) → à étendre avec migration 009 (item L)
