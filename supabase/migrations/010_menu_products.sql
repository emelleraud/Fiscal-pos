-- =============================================================================
-- 010_menu_products.sql
-- Catalogue produits réseau : catégories, produits, variantes, modificateurs.
-- Source de vérité du menu. site_configs.menu est le snapshot publié (étape ultérieure).
-- Produits/catégories réseau-wide (pas de site_id) — personnalisation par site
-- via pricing et disponibilités (migrations futures).
-- =============================================================================

-- ---------------------------------------------------------------------------
-- menu_categories : arbre de catégories (2 niveaux recommandés)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.menu_categories (
  id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  parent_id       uuid        REFERENCES public.menu_categories(id) ON DELETE SET NULL,
  name            text        NOT NULL,
  display_order   int         NOT NULL DEFAULT 0,
  kds_station     text,
  printer_station text,
  created_at      timestamptz NOT NULL DEFAULT now(),
  updated_at      timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- menu_products : catalogue réseau
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.menu_products (
  id                uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  category_id       uuid        REFERENCES public.menu_categories(id) ON DELETE SET NULL,
  sku               text        UNIQUE,
  name              text        NOT NULL,
  description       text,
  tva_rate          text        NOT NULL DEFAULT 'intermediaire10'
                      CHECK (tva_rate IN ('reduit5_5', 'intermediaire10', 'normal20')),
  base_price_cents  int         NOT NULL DEFAULT 0,
  image_url         text,
  is_active         boolean     NOT NULL DEFAULT true,
  visible_caisse    boolean     NOT NULL DEFAULT true,
  visible_kiosk     boolean     NOT NULL DEFAULT true,
  visible_delivery  boolean     NOT NULL DEFAULT true,
  visible_drive     boolean     NOT NULL DEFAULT true,
  visible_digital   boolean     NOT NULL DEFAULT true,
  created_at        timestamptz NOT NULL DEFAULT now(),
  updated_at        timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- menu_variants : déclinaisons d'un produit
-- Champs nullables = hérite du produit parent.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.menu_variants (
  id                    uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  product_id            uuid        NOT NULL REFERENCES public.menu_products(id) ON DELETE CASCADE,
  name                  text        NOT NULL,
  sku                   text        UNIQUE,
  price_override_cents  int,
  display_order         int         NOT NULL DEFAULT 0,
  is_active             boolean     NOT NULL DEFAULT true,
  created_at            timestamptz NOT NULL DEFAULT now(),
  updated_at            timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- menu_modifier_groups : bibliothèque de groupes partagés
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.menu_modifier_groups (
  id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  name        text        NOT NULL,
  min_select  int         NOT NULL DEFAULT 0,
  max_select  int         NOT NULL DEFAULT 1,
  is_required boolean     NOT NULL DEFAULT false,
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- menu_modifiers : options dans un groupe
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.menu_modifiers (
  id                uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  group_id          uuid        NOT NULL REFERENCES public.menu_modifier_groups(id) ON DELETE CASCADE,
  name              text        NOT NULL,
  price_delta_cents int         NOT NULL DEFAULT 0,
  display_order     int         NOT NULL DEFAULT 0,
  is_active         boolean     NOT NULL DEFAULT true,
  created_at        timestamptz NOT NULL DEFAULT now(),
  updated_at        timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- menu_product_modifier_groups : liaison produit ↔ groupes de modificateurs
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS public.menu_product_modifier_groups (
  id            uuid  PRIMARY KEY DEFAULT gen_random_uuid(),
  product_id    uuid  NOT NULL REFERENCES public.menu_products(id) ON DELETE CASCADE,
  group_id      uuid  NOT NULL REFERENCES public.menu_modifier_groups(id) ON DELETE CASCADE,
  display_order int   NOT NULL DEFAULT 0,
  UNIQUE (product_id, group_id)
);

-- ---------------------------------------------------------------------------
-- Index
-- ---------------------------------------------------------------------------
CREATE INDEX IF NOT EXISTS idx_menu_products_category ON public.menu_products(category_id);
CREATE INDEX IF NOT EXISTS idx_menu_products_sku      ON public.menu_products(sku) WHERE sku IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_menu_variants_product  ON public.menu_variants(product_id);
CREATE INDEX IF NOT EXISTS idx_menu_modifiers_group   ON public.menu_modifiers(group_id);
CREATE INDEX IF NOT EXISTS idx_menu_pmg_product       ON public.menu_product_modifier_groups(product_id);

-- ---------------------------------------------------------------------------
-- Triggers updated_at (réutilise set_updated_at() de migration 001)
-- ---------------------------------------------------------------------------
CREATE TRIGGER menu_categories_updated_at
  BEFORE UPDATE ON public.menu_categories
  FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();

CREATE TRIGGER menu_products_updated_at
  BEFORE UPDATE ON public.menu_products
  FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();

CREATE TRIGGER menu_variants_updated_at
  BEFORE UPDATE ON public.menu_variants
  FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();

CREATE TRIGGER menu_modifier_groups_updated_at
  BEFORE UPDATE ON public.menu_modifier_groups
  FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();

CREATE TRIGGER menu_modifiers_updated_at
  BEFORE UPDATE ON public.menu_modifiers
  FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();

-- ---------------------------------------------------------------------------
-- RLS
-- ---------------------------------------------------------------------------
ALTER TABLE public.menu_categories              ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.menu_products                ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.menu_variants                ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.menu_modifier_groups         ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.menu_modifiers               ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.menu_product_modifier_groups ENABLE ROW LEVEL SECURITY;

GRANT SELECT, INSERT, UPDATE, DELETE ON public.menu_categories              TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.menu_products                TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.menu_variants                TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.menu_modifier_groups         TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.menu_modifiers               TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.menu_product_modifier_groups TO authenticated;

CREATE POLICY "backoffice_menu_categories"
  ON public.menu_categories FOR ALL TO authenticated USING (true) WITH CHECK (true);
CREATE POLICY "backoffice_menu_products"
  ON public.menu_products FOR ALL TO authenticated USING (true) WITH CHECK (true);
CREATE POLICY "backoffice_menu_variants"
  ON public.menu_variants FOR ALL TO authenticated USING (true) WITH CHECK (true);
CREATE POLICY "backoffice_menu_modifier_groups"
  ON public.menu_modifier_groups FOR ALL TO authenticated USING (true) WITH CHECK (true);
CREATE POLICY "backoffice_menu_modifiers"
  ON public.menu_modifiers FOR ALL TO authenticated USING (true) WITH CHECK (true);
CREATE POLICY "backoffice_menu_product_modifier_groups"
  ON public.menu_product_modifier_groups FOR ALL TO authenticated USING (true) WITH CHECK (true);
