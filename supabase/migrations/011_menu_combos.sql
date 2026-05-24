-- supabase/migrations/011_menu_combos.sql
-- Combos/menus composés : en-tête + items fixes + slots configurables + options de slots.
-- Réseau-wide (pas de site_id) — même pattern que migration 010.

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

CREATE TABLE public.menu_combo_slots (
  id            uuid    PRIMARY KEY DEFAULT gen_random_uuid(),
  combo_id      uuid    NOT NULL REFERENCES public.menu_combos(id) ON DELETE CASCADE,
  name          text    NOT NULL,
  display_order int     NOT NULL DEFAULT 0,
  min_select    int     NOT NULL DEFAULT 1,
  max_select    int     NOT NULL DEFAULT 1,
  is_required   boolean NOT NULL DEFAULT true
);

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

CREATE INDEX idx_menu_combos_category      ON public.menu_combos(category_id);
CREATE INDEX idx_menu_combos_sku           ON public.menu_combos(sku) WHERE sku IS NOT NULL;
CREATE INDEX idx_combo_fixed_items_combo   ON public.menu_combo_fixed_items(combo_id);
CREATE INDEX idx_combo_fixed_product       ON public.menu_combo_fixed_items(product_id) WHERE product_id IS NOT NULL;
CREATE INDEX idx_combo_fixed_variant       ON public.menu_combo_fixed_items(variant_id) WHERE variant_id IS NOT NULL;
CREATE INDEX idx_combo_slots_combo         ON public.menu_combo_slots(combo_id);
CREATE INDEX idx_combo_slot_options_slot   ON public.menu_combo_slot_options(slot_id);
CREATE INDEX idx_combo_option_product      ON public.menu_combo_slot_options(product_id) WHERE product_id IS NOT NULL;
CREATE INDEX idx_combo_option_variant      ON public.menu_combo_slot_options(variant_id) WHERE variant_id IS NOT NULL;

CREATE TRIGGER menu_combos_updated_at
  BEFORE UPDATE ON public.menu_combos
  FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();

ALTER TABLE public.menu_combos             ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.menu_combo_fixed_items  ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.menu_combo_slots        ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.menu_combo_slot_options ENABLE ROW LEVEL SECURITY;

GRANT SELECT, INSERT, UPDATE, DELETE ON public.menu_combos             TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.menu_combo_fixed_items  TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.menu_combo_slots        TO authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.menu_combo_slot_options TO authenticated;

CREATE POLICY "backoffice_menu_combos"
  ON public.menu_combos FOR ALL TO authenticated USING (true) WITH CHECK (true);
CREATE POLICY "backoffice_menu_combo_fixed_items"
  ON public.menu_combo_fixed_items FOR ALL TO authenticated USING (true) WITH CHECK (true);
CREATE POLICY "backoffice_menu_combo_slots"
  ON public.menu_combo_slots FOR ALL TO authenticated USING (true) WITH CHECK (true);
CREATE POLICY "backoffice_menu_combo_slot_options"
  ON public.menu_combo_slot_options FOR ALL TO authenticated USING (true) WITH CHECK (true);
