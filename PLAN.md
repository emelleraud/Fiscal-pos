# Plan de développement — pos-fiscal
_Mis à jour le 2026-05-21_

---

## État du projet

| Composant        | État                                                                      | Tests           |
|------------------|---------------------------------------------------------------------------|-----------------|
| fiscal-engine    | Complet — moteur NF525, hash chain, multi-TVA, Z-reports, archives       | ~132 ✅          |
| sync-client      | Complet — sync sessions + z_reports + entries, idempotence, offline       | ~34 ✅ + 2 E2E   |
| edge-api         | Complet — sessions, orders, menu, health, archive NF525 §7               | 18 ✅            |
| backoffice       | Dashboard + Journal + Rapports Z visibles via anon key                    | —               |
| pos-app          | Écrans Order/Payment/Ticket/ZReport/Cancel câblés, multi-TVA             | ~24 ✅           |
| Supabase         | 8 migrations, RLS, vues SECURITY DEFINER, site_configs seedé             | —               |

---

## Sprint 1 — Bugs bloquants + conformité fiscale ✅

### A — ZReportScreen : clôture câblée ✅
`useSession()` depuis `hooks/useOrder.ts`. Ventilation TVA par taux affichée.

### B — CancelScreen : annulation câblée ✅
`useOrder()` utilisé directement dans le composant.

### C — Conformité NF525 : multi-TVA ✅
`CreateOrderRequest` envoie `line_items[]` par article. Le handler agrège par taux et stocke
`tva_5_5_breakdown`, `tva_10_breakdown`, `tva_20_breakdown` dans `FiscalEntry`.
Hash NF525 inchangé (figé pour certification LNE).

---

## Sprint 2 — Qualité opérationnelle ✅

### D — Sécuriser delete_test_data() ✅
Migration 007 — `REVOKE EXECUTE FROM PUBLIC, anon, authenticated` + `GRANT TO service_role`.

### E — Menu de démo + seed Supabase ✅
- `data/menu.json` : 7 articles QSR (Burgers, Accompagnements, Boissons, Desserts)
- Migration 008 : table `site_configs` + seed pour le site de test `9983f3ac-...`
- `GET /api/v1/menu` sert le fichier local ; sync-client le met à jour depuis Supabase.

### F — Tests edge-api ✅
18 tests d'intégration Axum oneshot couvrant toutes les routes critiques.
SQLite tempfile pour éviter le deadlock `max_connections(1)`.

### G — CLAUDE.md ✅
Créé à la racine — architecture, commandes, variables d'env, migrations, conventions NF525.

---

## Sprint 3 — Complétion NF525 ✅

### H — Archive engine ✅
`POST /api/v1/archive/{year}` — génère le CSV annuel signé Ed25519 (NF525 §7).
- Clé lue depuis `FISCAL_SIGNING_KEY_HEX` (64 hex chars).
- CSV écrit dans `{DATA_DIR}/archives/{year}.csv`.
- Métadonnées dans `archive_metadata` SQLite (idempotent : 409 si déjà présent).

### I — Electron preload.ts ✅
- `electron/preload.ts` : `contextBridge` exposant `getApiUrl` et `printText`.
- `tsconfig.electron.json` : compilation CommonJS in-place → `electron/main.js` + `preload.js`.
- Scripts `electron:dev` et `electron:build` compilent le process Electron avant de démarrer.

---

## Backlog long terme (hors MVP)

- Authentification back-office (rôle admin vs auditeur)
- Multi-sites : le back-office affiche toutes les données sans filtre `site_id`
- Impression thermique réelle (ESC/POS via Electron IPC — `printText` IPC câblé, driver manquant)
- Intégration TPE (Ingenico/Verifone)
- Remboursements et remises via pos-app (routes edge-api existantes, UI manquante)
- Gestion du menu depuis le back-office (CRUD `site_configs`)
- Génération automatique de la clé `FISCAL_SIGNING_KEY_HEX` au premier démarrage
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
