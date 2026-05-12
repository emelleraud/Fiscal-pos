# Plan de développement — pos-fiscal
_Mis à jour le 2026-05-13_

---

## État du projet

| Composant        | État                                                                 | Tests          |
|------------------|----------------------------------------------------------------------|----------------|
| fiscal-engine    | Complet — moteur NF525, hash chain, TVA, Z-reports, archives        | 132 ✅          |
| sync-client      | Complet — sync sessions + z_reports + entries, idempotence, offline | 34 ✅ + 2 E2E   |
| edge-api         | Opérationnel — sessions, orders, menu, health                       | 0 tests        |
| backoffice       | Dashboard + Journal + Rapports Z visibles via anon key              | —              |
| pos-app          | Écrans Order/Payment/Ticket existants, wiring partiel               | 24 ✅           |
| Supabase         | 6 migrations, RLS, vues SECURITY DEFINER, z_reports anon            | —              |

---

## Sprint 1 — Bugs bloquants + conformité fiscale

### A — ZReportScreen : clôture non câblée
`ZReportScreen` (dans `TicketScreen.tsx`) accède à `useSessionStore` directement avec
`closeSession: null as unknown as () => void`. Le bouton "Clôturer" est mort.
**Fix** : remplacer par `useSession()` depuis `hooks/useOrder.ts`.

### B — CancelScreen : annulation non câblée
Même problème : `cancelOrder: null as unknown as (r: string) => void`.
Le bouton "Confirmer l'annulation" ne fait rien.
**Fix** : utiliser `useOrder()` directement dans le composant.

### C — Conformité NF525 : panier multi-TVA (une entrée par taux)
`useOrder.submitMutation` appelle `createOrder()` une seule fois avec `dominantTvaRate()`.
Un panier [Burger×2 @20%, Coca×1 @10%] génère une seule entrée au taux majoritaire — incorrect.
**Fix** : grouper les articles du panier par `tva_rate`, émettre un `POST /orders` par groupe.
```
panier: [Burger×2 @20%, Coca×1 @10%]
→ POST /orders {amount: 2400, tva_rate: "20"}  → entry #N
→ POST /orders {amount: 200,  tva_rate: "10"}  → entry #N+1
```
Impact : `TicketScreen` affiche `currentFiscalEntries[]` au lieu d'une seule entrée.

---

## Sprint 2 — Qualité opérationnelle

### D — Sécuriser delete_test_data()
`public.delete_test_data(p_site_id uuid)` accessible sans restriction via l'API REST.
**Fix** : migration 007 — renommer en `delete_test_data_dev()` + `REVOKE EXECUTE FROM anon, authenticated`.

### E — Menu de démo + seed Supabase
La caisse affiche une carte vide si `menu.json` est absent.
`config_puller` tire la carte de `site_configs` mais cette table est vide.
**Fix** : créer `data/menu.json` de démo (5-6 articles QSR) + seed `site_configs` pour le site de test.

### F — Tests edge-api
Zéro test dans `edge-api/src/`. Risque de régression silencieuse.
**Fix** : tester les 4 routes critiques (`open_session`, `close_session`, `create_order`, `cancel_order`)
avec un store SQLite in-memory (pattern identique aux tests fiscal-engine).

### G — CLAUDE.md
Fichier de contexte projet manquant — chaque session Claude repart de zéro.
**Contenu** : architecture des crates, variables d'env, commandes build/test/run,
procédure de lancement complète (edge-api + sync-client + backoffice + pos-app).

---

## Sprint 3 — Complétion NF525

### H — Archive engine
`archive_engine` existe dans `fiscal-engine` mais n'est pas déclenché.
L'archivage annuel CSV (NF525 §8) doit être activé : route manager dans edge-api
ou tâche planifiée dans sync-client (déclenchement au 1er janvier ou manuel).

### I — Electron preload.js manquant
`electron/main.ts` référence `preload.js` mais le fichier est absent du repo.
**Fix** : créer `electron/preload.ts` avec `contextBridge` minimal (même vide),
sinon le build Electron plante.

---

## Backlog long terme (hors MVP)

- Authentification back-office (rôle admin vs auditeur)
- Multi-sites : le back-office affiche toutes les données sans filtre `site_id`
- Impression thermique réelle (ESC/POS via Electron IPC)
- Intégration TPE (Ingenico/Verifone)
- Remboursements et remises via pos-app (routes edge-api existantes, UI manquante)
- Gestion du menu depuis le back-office (CRUD `site_configs`)

---

## Points d'attention permanents

- `SUPABASE_SERVICE_KEY` ne doit jamais être committée (`.env.test` ignoré par git)
- `fiscal_entries` est immuable côté Supabase (trigger `prevent_delete` actif)
- Ordre de sync : **sessions → z_reports → fiscal_entries** (FK cloud)
- `run_migrations` utilise `_applied_migrations` pour être idempotent (depuis 2026-05-13)
- Site de test UUID : `9983f3ac-cde8-4838-9386-49ef24f57dad`
- Supabase project ref : `iawyngsvqjsogvkwkrxw`
