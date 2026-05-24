# Spec — Gestion des combos / menus composés

**Date :** 2026-05-24  
**Statut :** Approuvé — prêt pour implémentation  
**Projet :** pos-fiscal · back-office React + Supabase

---

## Contexte

Le catalogue suit l'**Option A** : tables relationnelles réseau-wide (sans `site_id`), avec une étape future "Publish" qui compile vers `site_configs.menu`. Les tables existantes (`menu_products`, `menu_variants`, `menu_modifier_groups`…) sont en place via migration 010.

Un combo est un article vendu au client comme un ensemble, à un prix global, composé de :
- **Items fixes** : composants toujours inclus, non-modifiables par le client
- **Slots configurables** : le client choisit parmi plusieurs options dans chaque slot

---

## Décisions de design

| Question | Décision |
|---|---|
| Structure | Mix fixe + slots (C) |
| Prix | Base fixe + surcharges par slot-option (C) |
| TVA | Chaque composant garde sa propre TVA — le moteur fiscal NF525 ventile (C) |
| Contenu des slots | Polymorphe : product_id OU variant_id (C) |
| Organisation | Catégorie existante + page dédiée (C) |
| I18n / multi-noms | Déféré — spec séparé. `name`/`description` = valeurs fr par défaut |

---

## Schéma DB — Migration 011

### `menu_combos`

En-tête du combo. Rattaché à `menu_categories`. Mêmes flags de visibilité que `menu_products`.

```sql
CREATE TABLE public.menu_combos (
  id               uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  category_id      uuid        REFERENCES public.menu_categories(id) ON DELETE SET NULL,
  sku              text        UNIQUE,
  name             text        NOT NULL,
  description      text,
  base_price_cents int         NOT NULL DEFAULT 0,
  is_active        boolean     NOT NULL DEFAULT true,
  visible_caisse   boolean     NOT NULL DEFAULT true,
  visible_kiosk    boolean     NOT NULL DEFAULT true,
  visible_delivery boolean     NOT NULL DEFAULT true,
  visible_drive    boolean     NOT NULL DEFAULT true,
  visible_digital  boolean     NOT NULL DEFAULT true,
  created_at       timestamptz NOT NULL DEFAULT now(),
  updated_at       timestamptz NOT NULL DEFAULT now()
);
```

> Pas de `tva_rate` — la TVA est portée par chaque composant (produit ou variante référencé dans les items fixes et options de slots).

### `menu_combo_fixed_items`

Items non-modifiables inclus dans le combo. Polymorphe : pointe sur un produit OU une variante.  
`quantity` permet "4 × Burger Classic".

```sql
CREATE TABLE public.menu_combo_fixed_items (
  id            uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  combo_id      uuid NOT NULL REFERENCES public.menu_combos(id) ON DELETE CASCADE,
  product_id    uuid REFERENCES public.menu_products(id) ON DELETE RESTRICT,
  variant_id    uuid REFERENCES public.menu_variants(id) ON DELETE RESTRICT,
  quantity      int  NOT NULL DEFAULT 1,
  display_order int  NOT NULL DEFAULT 0,
  CONSTRAINT fixed_item_has_target
    CHECK (product_id IS NOT NULL OR variant_id IS NOT NULL)
);
```

> `ON DELETE RESTRICT` : impossible de supprimer un produit/variante référencé dans un combo. L'utilisateur doit d'abord le retirer du combo. Évite les lignes orphelines incompatibles avec le CHECK.

### `menu_combo_slots`

Slots configurables. Chaque slot a un nom affiché au client et des contraintes de sélection.

```sql
CREATE TABLE public.menu_combo_slots (
  id            uuid    PRIMARY KEY DEFAULT gen_random_uuid(),
  combo_id      uuid    NOT NULL REFERENCES public.menu_combos(id) ON DELETE CASCADE,
  name          text    NOT NULL,
  display_order int     NOT NULL DEFAULT 0,
  min_select    int     NOT NULL DEFAULT 1,
  max_select    int     NOT NULL DEFAULT 1,
  is_required   boolean NOT NULL DEFAULT true
);
```

### `menu_combo_slot_options`

Options disponibles dans un slot. Polymorphe. `price_delta_cents` = surcharge si cette option est choisie (0 = inclus dans le prix de base). `is_default` = présélection au kiosk/caisse.

```sql
CREATE TABLE public.menu_combo_slot_options (
  id                uuid    PRIMARY KEY DEFAULT gen_random_uuid(),
  slot_id           uuid    NOT NULL REFERENCES public.menu_combo_slots(id) ON DELETE CASCADE,
  product_id        uuid    REFERENCES public.menu_products(id) ON DELETE RESTRICT,
  variant_id        uuid    REFERENCES public.menu_variants(id) ON DELETE RESTRICT,
  price_delta_cents int     NOT NULL DEFAULT 0,
  display_order     int     NOT NULL DEFAULT 0,
  is_default        boolean NOT NULL DEFAULT false,
  CONSTRAINT slot_option_has_target
    CHECK (product_id IS NOT NULL OR variant_id IS NOT NULL)
);
```

> Même logique `ON DELETE RESTRICT` que `menu_combo_fixed_items`.

### RLS & GRANTs

Même pattern que migration 010 : RLS activé, `GRANT ALL TO authenticated`, policy `FOR ALL USING(true) WITH CHECK(true)`.

---

## Pages back-office

### Navigation (`Layout.tsx`)

Ajout du lien **🎁 Combos** (`/combos`) dans la section "Catalogue", entre Produits et Modificateurs.

### `ComboList` — `/combos`

Tableau de liste identique au style de `ProductList`.

**Colonnes :** Nom · SKU · Catégorie · Prix base · Slots · Items fixes · Statut · Actions (Éditer / Suppr.)

**Filtres :** recherche texte (nom/SKU) + filtre par catégorie.

**Header :** compteur `x / total` + bouton "+ Nouveau combo".

**Suppression :** `confirm()` → supprime le combo (CASCADE supprime fixed_items, slots, slot_options).

### `ComboForm` — `/combos/new` · `/combos/:id`

Formulaire en 3 sections (même composant `Card`/`Field` que `ProductForm`) :

**Section Informations**
- Nom, SKU (auto-généré `CMB-${Date.now()}` si vide), Prix base (€, converti en centimes), Catégorie (select), is_active (checkbox)
- Flags de visibilité : caisse, kiosk, livraison, drive, digital

**Section Items fixes**
- Liste de `FixedItemRow` : sélecteur produit/variante (select groupé), quantité (input number), bouton supprimer
- Bouton "+ Ajouter item fixe"
- Le select est un dropdown groupé : groupe "Produits" (tous les `menu_products`) + groupe "Variantes" (toutes les `menu_variants` avec leur nom de produit parent)
- Valeur encodée : `"product:uuid"` ou `"variant:uuid"` → décodé à la sauvegarde

**Section Slots configurables**
- Liste de `SlotBlock` (expandable inline) :
  - Nom du slot (input texte), min_select, max_select, is_required (checkbox), bouton supprimer slot
  - Liste d'`OptionRow` : sélecteur produit/variante (même select groupé), surcharge prix (input €), is_default (checkbox), bouton supprimer option
  - Bouton "+ Ajouter option"
- Bouton "+ Ajouter slot"

**Logique de sauvegarde (même pattern que `ProductForm`)**

1. INSERT ou UPDATE `menu_combos`
2. DELETE tous les `menu_combo_fixed_items` du combo → INSERT les lignes actuelles
3. DELETE tous les `menu_combo_slots` du combo (CASCADE supprime les options) → INSERT slots + options

> Stratégie delete-all + reinsert (simple, pas de diff) — acceptable car le catalogue est édité rarement.

**Validation**
- Nom obligatoire
- Prix base ≥ 0
- Chaque slot doit avoir ≥ 1 option
- Chaque fixed_item doit avoir un produit/variante sélectionné

---

## Routes à ajouter (`App.tsx`)

```tsx
<Route path="/combos"       element={<ComboList />} />
<Route path="/combos/new"   element={<ComboForm />} />
<Route path="/combos/:id"   element={<ComboForm />} />
```

---

## Fichiers à créer / modifier

| Fichier | Action |
|---|---|
| `supabase/migrations/011_menu_combos.sql` | Créer — 4 tables + RLS + index |
| `backoffice/src/pages/ComboList.tsx` | Créer |
| `backoffice/src/pages/ComboForm.tsx` | Créer |
| `backoffice/src/components/Layout.tsx` | Modifier — ajouter lien /combos |
| `backoffice/src/App.tsx` | Modifier — ajouter 3 routes |

---

## Non inclus dans ce spec (specs futurs)

- **I18n / multi-noms** : jusqu'à 5 noms par item selon destination et langue — spec dédié
- **Pricing clusters** : combos avec tarifs variables selon heure/site — spec dédié
- **Disponibilités & ruptures** : activation/désactivation par site — spec dédié
- **Étape Publish** : compilation catalogue → `site_configs.menu` — spec dédié
- Combos imbriqués (combo dans combo) : hors scope

---

## Checklist technique

- [ ] Migration 011 appliquée dans Supabase
- [ ] `ComboList` : liste avec filtres, suppression avec confirm
- [ ] `ComboForm` : 3 sections, sélecteur product/variant groupé, auto-SKU
- [ ] Routes et nav ajoutés
- [ ] Build TypeScript sans erreur (`npm run build`)
