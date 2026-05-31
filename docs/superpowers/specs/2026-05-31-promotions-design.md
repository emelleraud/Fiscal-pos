# Spec — Mécaniques de promotions
_2026-05-31_

## Contexte

Le POS fiscal NF525 ne dispose d'aucune gestion de promotions. Le `fiscal-engine` expose déjà un `OperationType::Discount` (montant TTC négatif), mais l'edge-api ne l'utilise pas encore. Ce document spécifie la mécanique complète : catalogue back-office, évaluation serveur, enregistrement fiscal, UX caisse, groupes de restaurants et workflow d'approbation par rôle.

---

## Types de remises supportés

| Type | Description |
|---|---|
| `fixed_amount` | Remise fixe en centimes sur le total TTC |
| `percentage` | Remise en % sur le total TTC (stocké en basis points : 1000 = 10 %) |
| `item_discount` | Remise fixe ou % sur un article identifié par SKU |
| `bogo` | Buy-one-get-one : 1 article SKU cible offert si ≥ 1 dans le panier |
| `happy_hour` | Remise % ou fixe conditionnée à une plage horaire |

Toutes les mécaniques sont combinables. Le cumul est paramétrable par promo via un `exclusion_group`.

---

## Déclenchement

- **Auto** : évalué automatiquement par le serveur à chaque commande (happy hour, promos date-limited). Aucune action caissier.
- **Manuel** : le caissier sélectionne la promo dans une modal POS. Les IDs sélectionnés sont envoyés dans le payload `POST /api/v1/orders`.

---

## Portée (scope)

| Valeur | Description | Condition sync |
|---|---|---|
| `chain` | Tous les sites | Tous les sites tirent cette promo |
| `group` | Groupe de restaurants (statique ou dynamique) | Sites membres du groupe |
| `site` | Un seul restaurant | `site_id` exact |

Le sync-client tire : `scope = 'chain' OR site_id = $SITE_ID OR (scope = 'group' AND $SITE_ID IN group_members)`.

---

## Hiérarchie de rôles

| Rôle | Périmètre | Accès back-office |
|---|---|---|
| `cashier` | POS uniquement | Aucun |
| `manager` | Site | Son site uniquement |
| `director` | Site | Son site avec droits étendus |
| `regional_director` | Chaîne / franchise | Tous les sites de sa région |

Les rôles sont stockés dans `auth.users.app_metadata.role` (Supabase Auth). Jamais dans `user_metadata` (modifiable par l'utilisateur).

---

## Workflow d'approbation

### Statuts d'une promotion

```
draft → pending_approval → approved → active
                        ↘ rejected
```

- `draft` : créée, pas encore soumise
- `pending_approval` : soumise pour validation
- `approved` : validée par le bon rôle, peut être activée
- `active` : en production (sync-client la tire)
- `rejected` : refusée (motif obligatoire)

### Règles d'approbation (combinaison scope + montant)

| Scope | Montant max remise | Rôle minimum pour approuver |
|---|---|---|
| `site` | ≤ seuil_manager (configurable, défaut 10 €) | `manager` |
| `site` | > seuil_manager | `director` |
| `group` | tout montant | `director` |
| `chain` | tout montant | `regional_director` |

Les seuils sont configurables dans la table `promotion_approval_thresholds`.

### Contraintes
- Un `manager` ne peut créer que des promos `scope = site` pour son propre site.
- Un `director` peut créer des promos `site` et `group` pour son site/périmètre, et approuver les promos de ses managers.
- Un `regional_director` peut créer des promos `chain`, `group`, et `site` pour tous les sites de sa région, et approuver toute promo de son périmètre.
- Seul le rôle requis (ou supérieur) peut faire passer une promo de `pending_approval` à `approved`.
- Seule une promo `approved` peut être mise `active`.

---

## Section 1 — Modèle de données (Supabase + SQLite)

### Table `restaurant_groups`

| Colonne | Type SQL | Description |
|---|---|---|
| `id` | uuid PK | Identifiant |
| `name` | text NOT NULL | Libellé (ex : "Île-de-France", "Franchisés Nord") |
| `group_type` | text CHECK IN ('static','dynamic','mixed') | Mode de définition |
| `criteria` | jsonb nullable | Critères dynamiques (ex : `{"ville": "Paris"}`, `{"ca_min": 500000}`) |
| `created_by` | uuid REFERENCES auth.users | Créateur |
| `created_at` | timestamptz | |

### Table `restaurant_group_members` (membres statiques)

| Colonne | Type SQL | Description |
|---|---|---|
| `group_id` | uuid REFERENCES restaurant_groups | |
| `site_id` | uuid REFERENCES sites | |
| PK | (group_id, site_id) | |

Les membres dynamiques sont calculés à la volée via les `criteria` JSONB appliqués à la table `sites`. Le sync-client évalue la requête au moment du pull et ne stocke que le résultat booléen (appartient/n'appartient pas).

### Table `promotion_approval_thresholds`

| Colonne | Type SQL | Description |
|---|---|---|
| `id` | uuid PK | |
| `scope` | text | `site` / `group` / `chain` |
| `max_cents` | integer nullable | Seuil (null = illimité) |
| `required_role` | text | `manager` / `director` / `regional_director` |

Seedée avec les valeurs par défaut. Modifiable par `regional_director` uniquement.

### Table `promotions`

| Colonne | Type SQL | Contrainte | Description |
|---|---|---|---|
| `id` | uuid | PK | |
| `name` | text | NOT NULL | Libellé affiché caisse + ticket |
| `scope` | text | CHECK IN ('chain','group','site') | Portée |
| `site_id` | uuid | REFERENCES sites(id), nullable | Null si chain/group |
| `group_id` | uuid | REFERENCES restaurant_groups(id), nullable | Null si chain/site |
| `promo_type` | text | CHECK IN ('fixed_amount','percentage','item_discount','bogo','happy_hour') | Mécanique |
| `value_cents` | integer | nullable | Montant fixe en centimes |
| `value_bps` | integer | nullable | % en basis points (1000 = 10 %) |
| `target_sku` | text | nullable | SKU cible pour item_discount et bogo |
| `trigger` | text | CHECK IN ('auto','manual') | Mode de déclenchement |
| `exclusion_group` | text | nullable | Promos du même groupe = mutuellement exclusives |
| `priority` | integer | NOT NULL DEFAULT 0 | Départage dans un groupe exclusif |
| `valid_from` | date | nullable | Début de la fenêtre calendaire |
| `valid_to` | date | nullable | Fin de la fenêtre calendaire |
| `days_of_week` | integer[] | nullable | 1=Lun … 7=Dim |
| `time_from` | time | nullable | Début plage horaire |
| `time_to` | time | nullable | Fin plage horaire |
| `status` | text | CHECK IN ('draft','pending_approval','approved','active','rejected') NOT NULL DEFAULT 'draft' | Workflow |
| `required_approval_role` | text | nullable | Calculé à la création selon les seuils |
| `approved_by` | uuid | REFERENCES auth.users, nullable | |
| `approved_at` | timestamptz | nullable | |
| `rejected_by` | uuid | REFERENCES auth.users, nullable | |
| `rejection_reason` | text | nullable | Obligatoire si rejected |
| `created_by` | uuid | REFERENCES auth.users NOT NULL | |
| `active` | boolean | NOT NULL DEFAULT false | Activation manuelle post-approval |
| `created_at` | timestamptz | DEFAULT now() | |
| `updated_at_ms` | bigint | DEFAULT epoch*1000 | Utilisé par sync-client |

**Contrainte** : `CHECK (scope = 'site' AND site_id IS NOT NULL AND group_id IS NULL) OR (scope = 'group' AND group_id IS NOT NULL AND site_id IS NULL) OR (scope = 'chain' AND site_id IS NULL AND group_id IS NULL)`

### RLS

| Rôle | Lecture | Écriture |
|---|---|---|
| `service_role` | tout | tout |
| `regional_director` | tout | tout |
| `director` | site + group de son périmètre | site + group de son périmètre |
| `manager` | son site | son site (scope=site, montant ≤ seuil) |
| `anon` | promos `active=true` only | aucune |

### Calcul fiscal des remises
La TVA d'une remise est ventilée **proportionnellement au panier** :
si 60 % du panier est à 10 % TVA et 40 % à 20 %, une remise de 5 € sera ventilée 3 €@10 % + 2 €@20 %.
Chaque promo appliquée génère **une entrée `DISCOUNT` distincte** dans le journal fiscal.

---

## Section 2 — Crate `promo-engine`

### Emplacement
`pos-fiscal/promo-engine/` — crate Rust dans le workspace, dépendance de `edge-api`.

### Interface publique

```rust
pub fn evaluate(
    cart: &Cart,
    promos: &[Promotion],
    manual_selected_ids: &[Uuid],
    now: DateTime<Utc>,
) -> EvalResult

pub struct Cart {
    pub line_items: Vec<CartItem>,
    pub total_ttc_cents: i64,
}

pub struct CartItem {
    pub sku: String,
    pub amount_ttc_cents: i64,
    pub tva_rate: TvaRate,
}

pub struct EvalResult {
    pub applied: Vec<PromoApplication>,
    pub rejected: Vec<PromoApplication>,
}

pub struct PromoApplication {
    pub promo_id: Uuid,
    pub promo_name: String,
    pub discount_cents: i64,         // montant TTC positif (remise)
    pub tva_breakdown: TvaBreakdown, // ventilation TVA proportionnelle
}
```

Le `promo-engine` ne reçoit que des promos déjà filtrées sur `status = 'active'` — le filtrage de statut est fait par l'edge-api avant l'appel.

### Algorithme d'évaluation

1. Filtrer `active = true` ET dans la fenêtre de validité (`valid_from`/`valid_to`, `days_of_week`, `time_from`/`time_to`)
2. Séparer auto (toutes éligibles) et manual (filtrées sur `manual_selected_ids`)
3. Vérifier conditions métier : SKU présent dans le panier pour `item_discount`/`bogo`, panier non vide
4. Calculer `discount_cents` pour chaque candidate
5. Résoudre les groupes exclusifs : par `exclusion_group`, ne garder que la promo avec la `priority` la plus haute (égalité → plus grand `discount_cents`)
6. Ventiler la TVA de chaque remise proportionnellement au panier
7. Retourner `EvalResult { applied, rejected }`

### Tests unitaires (~20 cas)
- Promo hors fenêtre date → rejetée
- Promo hors plage horaire → rejetée
- Promo hors jour de semaine → rejetée
- BOGO sur SKU absent du panier → rejetée
- Groupe exclusif : seule la priorité la plus haute retenue
- Groupe exclusif égalité : la plus avantageuse retenue
- Promos sans groupe : toutes cumulées
- Ventilation TVA proportionnelle (multi-taux)
- `manual_selected_ids` vide → seules les auto appliquées
- Promo inactive → ignorée

---

## Section 3 — Sync-client + edge-api

### sync-client
Nouvelle étape de pull après `fiscal_entries` :
```
sessions → z_reports → fiscal_entries → promotions
```
Requête : promos `status = 'active'` pour `scope = 'chain'`, ou `site_id = $SITE_ID`, ou `group_id IN (groupes dont $SITE_ID est membre)`.
Upsert dans la table SQLite locale `promotions` (même schéma, colonnes JSON pour `days_of_week`).

Le sync-client évalue également les critères dynamiques des groupes au moment du pull pour déterminer l'appartenance du site.

### Nouvelle route edge-api

```
GET /api/v1/promotions/available
```
Retourne les promos `status = 'active'` dans la fenêtre de validité pour `now()` :
- `auto` éligibles avec `discount_cents` indicatif (calculé sur panier vide)
- `manual` actives (pour affichage dans la modal caissier)

### Modification `POST /api/v1/orders`

Nouveau champ optionnel dans `CreateOrderRequest` :
```json
{ "manual_promo_ids": ["uuid1", "uuid2"] }
```

Flux dans le handler :
1. Valider le panier (inchangé)
2. Charger les promos `active` depuis SQLite local
3. Appeler `promo_engine::evaluate(cart, promos, manual_ids, now)`
4. Pour chaque `PromoApplication` → enregistrer une entrée `DISCOUNT` dans le journal fiscal
5. Enregistrer la `SALE` habituelle
6. Retourner le ticket avec la liste des remises appliquées

---

## Section 4 — pos-app

### Nouveaux fichiers

| Fichier | Rôle |
|---|---|
| `src/api/client.ts` | `getAvailablePromotions()` + `manual_promo_ids` dans `createOrder()` |
| `src/components/PromoModal.tsx` | Modal sélection promos manuelles |
| `src/store/orderStore.ts` | Ajouter `selectedPromoIds: string[]` dans le state Zustand |

### Modifications

**`OrderPage.tsx`** : bouton "Promos" → ouvre `PromoModal`. Affiche le total remisé prévisualisé localement (indicatif — l'edge-api fait autorité).

**`PromoModal.tsx`** :
- Promos `auto` éligibles : affichées cochées, non-décochables (label vert)
- Promos `manual` : checkboxes, caissier sélectionne
- Confirmation → `selectedPromoIds` mis à jour dans le store

**Ticket** : chaque `PromoApplication` retournée par l'edge-api est affichée en ligne négative avec libellé et montant.

---

## Section 5 — Back-office

### Nouvelles pages

| Route | Composant | Rôle | Accès minimum |
|---|---|---|---|
| `/promotions` | `PromotionList.tsx` | Tableau + filtres + file d'approbation | `manager` |
| `/promotions/new` | `PromotionForm.tsx` | Création | `manager` |
| `/promotions/:id` | `PromotionForm.tsx` | Édition / approbation | `manager` |
| `/groups` | `GroupList.tsx` | Liste des groupes de restaurants | `director` |
| `/groups/new` | `GroupForm.tsx` | Création groupe statique/dynamique/mixte | `director` |
| `/groups/:id` | `GroupForm.tsx` | Édition groupe | `director` |

### `PromotionList.tsx`
Colonnes : Nom / Type / Scope / Déclencheur / Validité / Statut / Approuvé par / Actions.
Filtres : scope / statut / site.
**File d'approbation** : onglet "En attente" visible par `director` et `regional_director`, affiche les promos `pending_approval` du périmètre de l'utilisateur avec boutons "Approuver" / "Rejeter" (champ motif obligatoire si rejet).

### `PromotionForm.tsx` — sections

1. **Identité** : nom, scope → si `site`, sélecteur de site ; si `group`, sélecteur de groupe ; déclencheur (auto/manuel)
2. **Mécanique** : type → champs conditionnels selon le type
   - `fixed_amount` : montant €
   - `percentage` : % (0–100, converti en bps)
   - `item_discount` : montant ou %, SKU cible
   - `bogo` : SKU cible
   - `happy_hour` : % ou montant + plage horaire obligatoire
3. **Cumul** : groupe d'exclusion (texte libre, nullable) + priorité (int, défaut 0)
4. **Validité** : date début/fin + checkboxes jours de semaine + heure début/fin
5. **Statut & approbation** : bouton "Soumettre pour approbation" (passe à `pending_approval`). Si l'utilisateur est lui-même habilité, bouton "Approuver directement". Affiche le rôle requis calculé.

### `GroupForm.tsx`
- Nom du groupe + type (static / dynamic / mixed)
- Si static ou mixed : sélecteur multi-sites (liste de tous les sites)
- Si dynamic ou mixed : éditeur de critères JSONB (champs clé/valeur : `ville`, `region`, `ca_min`, `type`, etc.)
- Aperçu en temps réel : "X sites correspondent à ces critères"

### Gestion des rôles utilisateurs
Les rôles sont affectés via le dashboard Supabase Auth (`app_metadata.role`). Pas d'UI back-office pour la gestion des utilisateurs dans ce sprint — trop sensible pour être exposé sans RBAC complet.

### Migrations Supabase

| Fichier | Contenu |
|---|---|
| `012_restaurant_groups.sql` | Tables `restaurant_groups` + `restaurant_group_members` + RLS |
| `013_promotion_approval_thresholds.sql` | Table + seed des seuils par défaut |
| `014_promotions.sql` | Table `promotions` complète + RLS par rôle |

### `App.tsx` + `Layout.tsx`
Ajout routes et entrées nav :
- "Promotions" sous CATALOGUE (visible par `manager` et au-dessus)
- "Groupes" sous CONFIG SITE (visible par `director` et au-dessus)

---

## Points d'attention

- `discount_cents` est toujours **positif** dans `PromoApplication` — c'est l'edge-api qui enregistre l'entrée DISCOUNT avec montant négatif dans le fiscal-engine.
- `promo-engine` ne dépend pas de `fiscal-engine` — zéro couplage entre les deux crates.
- Le hash NF525 n'est pas affecté : les DISCOUNT sont des entrées standard du journal.
- Les promos `chain` sans `site_id` doivent être synchées par **tous** les sites.
- La table SQLite locale `promotions` est en **lecture seule** côté caisse — aucune écriture locale.
- Les rôles sont stockés dans `app_metadata` (non modifiable par l'utilisateur) — jamais dans `user_metadata`.
- Un `manager` ne peut pas s'auto-approuver une promo qui dépasse son seuil.
- Les critères dynamiques de groupe sont évalués côté sync-client au moment du pull — pas en temps réel à la caisse.
- `required_approval_role` est calculé et stocké à la création de la promo (snapshot des seuils à ce moment) pour éviter qu'un changement de seuil rétroactif ne bloque des promos existantes.
